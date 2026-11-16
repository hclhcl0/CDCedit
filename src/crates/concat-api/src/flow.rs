// SPDX-License-Identifier: AGPL-3.0-or-later
//! Google Flow Studio Track B automation endpoints behind [`crate::Api`].

use std::path::{Path, PathBuf};
use std::sync::Arc;

use concat_host::flow::{self, Account, Accounts, FlowReceipt};
use concat_host::media;
use concat_project::Command;
use serde_json::{Value, json};

use crate::message::{
    ApiError, DryRunInfo, ErrorCode, Event, FlowAccountInfo, FlowAccountRecentInfo,
    FlowAccountsInfo, FlowConnectionInfo, FlowReceiptInfo, FlowRecentInfo, FlowStatusInfo, Reply,
    Request,
};
use crate::{Api, PendingStep};

fn mode_name(mode: usize) -> &'static str {
    match mode {
        1 => "attachOnly",
        2 => "customProfile",
        _ => "managed",
    }
}

fn load_accounts(dirs: &concat_host::AppDirs) -> (PathBuf, Accounts) {
    let root = flow::root_dir(dirs);
    let mut accounts = Accounts::default();
    accounts.load(&root);
    (root, accounts)
}

fn pick_account(
    dirs: &concat_host::AppDirs,
    account_id: Option<&str>,
) -> Result<(PathBuf, Account), ApiError> {
    let (root, accounts) = load_accounts(dirs);
    if accounts.config.accounts.is_empty() && !accounts.message.is_empty() {
        return Err(ApiError::new(ErrorCode::Refused, accounts.message));
    }
    let explicit = account_id.unwrap_or("").trim();
    let candidates = accounts
        .config
        .candidates(explicit, None)
        .map_err(|msg| ApiError::new(ErrorCode::Refused, msg))?;
    let selected = candidates
        .into_iter()
        .next()
        .ok_or_else(|| ApiError::new(ErrorCode::Refused, "no enabled Google Flow account"))?;
    Ok((root, selected))
}

fn receipt_to_info(receipt: FlowReceipt, state_dir: &Path) -> FlowReceiptInfo {
    let files: Vec<String> = receipt
        .result
        .as_ref()
        .map(|body| {
            let mut paths = if receipt.tool == "flow_generate_image" {
                flow::generated_images(state_dir, body)
            } else {
                let mut combined = flow::strict_result_files(state_dir, body);
                combined.extend(flow::generated_images(state_dir, body));
                combined
            };
            paths.sort();
            paths.dedup();
            paths
                .into_iter()
                .map(|p| p.to_string_lossy().into_owned())
                .collect()
        })
        .unwrap_or_default();

    FlowReceiptInfo {
        id: receipt.id,
        phase: receipt.phase,
        tool: receipt.tool,
        updated: receipt.updated,
        model: receipt.model,
        error: receipt.error,
        files,
    }
}

fn resolve_project_url(requested: Option<&str>, account: &Account) -> Result<String, ApiError> {
    let url = requested
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| account.project_url.trim());
    if url.is_empty() {
        return Err(ApiError::invalid(
            "`projectUrl` is required (pass a `https://flow.google.com/project/<uuid>` URL or save a default URL in Settings > Google Flow)",
        ));
    }
    if !flow::valid_project(url) {
        return Err(ApiError::invalid(format!(
            "invalid Google Flow project URL: {url:?}"
        )));
    }
    Ok(url.to_owned())
}

fn validate_references(refs: &[String]) -> Result<Vec<String>, ApiError> {
    let mut validated = Vec::new();
    for raw in refs {
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            continue;
        }
        if !Path::new(trimmed).is_file() {
            return Err(ApiError::new(
                ErrorCode::NotFound,
                format!("reference image not found: {trimmed}"),
            ));
        }
        validated.push(trimmed.to_owned());
    }
    Ok(validated)
}

#[derive(Clone, Copy)]
enum FlowGenKind {
    Image,
    Video,
}

struct FlowGenSpec {
    kind: FlowGenKind,
    account_id: Option<String>,
    prompt: String,
    model: Option<String>,
    ratio: Option<String>,
    count: Option<u32>,
    duration: Option<u32>,
    reference_images: Vec<String>,
    project_url: Option<String>,
    auto_confirm: Option<bool>,
    resume: bool,
    job_id: Option<String>,
    edit_mode: bool,
    use_direct_api: Option<bool>,
    allow_credit: bool,
    acknowledge_uncertain: bool,
    project_path: Option<String>,
    auto_import: Option<bool>,
    place_on_timeline: bool,
    start: Option<f64>,
    track_id: Option<String>,
    dry_run: bool,
}

fn validate_generation_params(
    spec: &FlowGenSpec,
) -> Result<(String, String, Option<u32>, Option<u32>), ApiError> {
    let prompt_trimmed = spec.prompt.trim();
    if prompt_trimmed.is_empty() && !spec.resume {
        return Err(ApiError::invalid(
            "`prompt` must not be empty unless `resume: true`",
        ));
    }
    match spec.kind {
        FlowGenKind::Image => {
            let model = spec
                .model
                .as_deref()
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .unwrap_or("nano-banana-pro")
                .to_owned();
            if !matches!(
                model.as_str(),
                "nano-banana-pro" | "nano-banana-2" | "nano-banana-2-lite"
            ) {
                return Err(ApiError::invalid(format!(
                    "unknown Flow image model {model:?}: expected \"nano-banana-pro\", \"nano-banana-2\", or \"nano-banana-2-lite\""
                )));
            }
            let ratio = spec
                .ratio
                .as_deref()
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .unwrap_or("1:1")
                .to_owned();
            if !matches!(ratio.as_str(), "9:16" | "16:9" | "1:1" | "4:3" | "3:4") {
                return Err(ApiError::invalid(format!(
                    "invalid Flow image aspect ratio {ratio:?}: expected 9:16, 16:9, 1:1, 4:3, or 3:4"
                )));
            }
            let count = spec.count.unwrap_or(1);
            if !(1..=4).contains(&count) {
                return Err(ApiError::invalid(format!(
                    "image `count` must be 1..=4 (got {count})"
                )));
            }
            Ok((model, ratio, Some(count), None))
        }
        FlowGenKind::Video => {
            let model = spec
                .model
                .as_deref()
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .unwrap_or("veo-3.1-fast")
                .to_owned();
            if !matches!(
                model.as_str(),
                "veo-3.1-lite"
                    | "veo-3.1-fast"
                    | "veo-3.1-quality"
                    | "omni-flash"
                    | "omni-flash-1.2"
            ) {
                return Err(ApiError::invalid(format!(
                    "unknown Flow video model {model:?}: expected \"veo-3.1-lite\", \"veo-3.1-fast\", \"veo-3.1-quality\", \"omni-flash\", or \"omni-flash-1.2\""
                )));
            }
            let ratio = spec
                .ratio
                .as_deref()
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .unwrap_or("9:16")
                .to_owned();
            if !matches!(ratio.as_str(), "9:16" | "16:9") {
                return Err(ApiError::invalid(format!(
                    "invalid Flow video aspect ratio {ratio:?}: expected \"9:16\" or \"16:9\""
                )));
            }
            let duration = spec.duration.unwrap_or(8);
            let valid_duration = match model.as_str() {
                "omni-flash-1.2" => (3..=15).contains(&duration),
                "omni-flash" => matches!(duration, 4 | 6 | 8 | 10),
                _ => matches!(duration, 4 | 6 | 8),
            };
            if !valid_duration {
                return Err(ApiError::invalid(format!(
                    "duration {duration}s is unsupported for Flow model {model:?}"
                )));
            }
            Ok((model, ratio, None, Some(duration)))
        }
    }
}

impl Api {
    /// [`Request::FlowAccountsList`].
    pub fn flow_accounts_list(&self) -> FlowAccountsInfo {
        let (root, accounts) = load_accounts(&self.dirs);
        let rendering = self.flow_lock.rendering_accounts();
        let config_valid =
            accounts.config.validate().is_ok() && !accounts.config.accounts.is_empty();
        let config_error = if accounts.message.trim().is_empty() {
            None
        } else {
            Some(accounts.message.clone())
        };
        let list = if !accounts.config.accounts.is_empty() {
            &accounts.config.accounts
        } else {
            &accounts.draft.accounts
        };
        let infos = list
            .iter()
            .map(|a| {
                let state_dir = a.state_dir(&root);
                let busy = rendering.contains(&a.id) || self.flow_lock.in_use(&state_dir);
                let unresolved_receipts = flow::unresolved_receipts(&state_dir).len();
                FlowAccountInfo {
                    id: a.id.clone(),
                    name: a.name.clone(),
                    enabled: a.enabled,
                    mode: mode_name(a.mode).to_owned(),
                    port: a.port,
                    profile_directory: a.profile_directory.clone(),
                    project_url: a.project_url.clone(),
                    detected_email: a.detected_email(&root),
                    busy,
                    unresolved_receipts,
                }
            })
            .collect();

        FlowAccountsInfo {
            root_dir: root.to_string_lossy().into_owned(),
            config_valid,
            config_error,
            parallel_accounts: accounts.config.parallel_accounts,
            total_slots: accounts.config.total_slots,
            effective_limit: accounts.config.limit(),
            accounts: infos,
        }
    }

    /// [`Request::FlowConnect`].
    pub fn flow_connect(
        &self,
        account_id: Option<&str>,
        open_url: Option<&str>,
    ) -> Result<FlowConnectionInfo, ApiError> {
        let (root, account) = pick_account(&self.dirs, account_id)?;
        let state_dir = account.state_dir(&root);
        if self.flow_lock.in_use(&state_dir) {
            return Err(ApiError::new(
                ErrorCode::Busy,
                format!(
                    "Flow account `{}` is currently busy in another task",
                    account.id
                ),
            ));
        }

        let target_url = open_url
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .or_else(|| {
                let saved = account.project_url.trim();
                (!saved.is_empty()).then_some(saved)
            });
        let args = match target_url {
            Some(url) => json!({ "open_url": url }),
            None => json!({}),
        };

        let body = self
            .flow_lock
            .call_account(&root, &account, "flow_connect", args, false)
            .map_err(|err| {
                if account.mode == 1 {
                    ApiError::failed(format!(
                        "{err} (Account `{}` uses attachOnly mode on CDP port {}: start Chrome with `--remote-debugging-port={}` first.)",
                        account.id, account.port, account.port
                    ))
                } else {
                    ApiError::failed(err)
                }
            })?;

        let attached_existing = body["attached_existing"].as_bool().unwrap_or(false);
        let cdp_port = body["cdp_port"]
            .as_u64()
            .and_then(|n| u16::try_from(n).ok())
            .unwrap_or(account.port);
        let tab_url = body["tab_url"].as_str().unwrap_or("").to_owned();
        let logged_in = body["logged_in"].as_bool().unwrap_or(false);
        let hint = if logged_in {
            None
        } else {
            Some(format!(
                "Chrome is open on CDP port {cdp_port}, but Google Flow is not signed in yet. Sign in manually inside that Chrome window and run `flow_status` again."
            ))
        };

        Ok(FlowConnectionInfo {
            account_id: account.id,
            attached_existing,
            cdp_port,
            tab_url,
            logged_in,
            hint,
        })
    }

    /// [`Request::FlowStatus`].
    pub fn flow_status(&self, account_id: Option<&str>) -> Result<FlowStatusInfo, ApiError> {
        let (root, account) = pick_account(&self.dirs, account_id)?;
        let state_dir = account.state_dir(&root);
        let unresolved: Vec<FlowReceiptInfo> = flow::unresolved_receipts(&state_dir)
            .into_iter()
            .map(|r| receipt_to_info(r, &state_dir))
            .collect();
        let busy = self.flow_lock.in_use(&state_dir)
            || self.flow_lock.rendering_accounts().contains(&account.id);

        if busy {
            return Ok(FlowStatusInfo {
                account_id: account.id,
                cdp_port: account.port,
                busy: true,
                connected: true,
                logged_in: None,
                url: None,
                project_id: None,
                scene_id: None,
                running_tool: None,
                jobs_in_flight: Vec::new(),
                unresolved_receipts: unresolved,
                note: Some(
                    "Worker is currently executing another Google Flow request on this account."
                        .to_owned(),
                ),
            });
        }

        let body = self
            .flow_lock
            .call_account(&root, &account, "flow_status", json!({}), false)
            .map_err(ApiError::failed)?;

        let connected = body["connected"].as_bool().unwrap_or(false);
        let logged_in = body["logged_in"].as_bool();
        let note = if !connected && account.mode == 1 {
            Some(format!(
                "No Chrome CDP endpoint reachable on port {} (`attachOnly` mode does not launch Chrome automatically).",
                account.port
            ))
        } else if connected && logged_in == Some(false) {
            Some(format!(
                "Chrome on CDP port {} is not signed in to Google Flow yet; sign in manually in Chrome.",
                account.port
            ))
        } else {
            None
        };

        Ok(FlowStatusInfo {
            account_id: account.id,
            cdp_port: account.port,
            busy: false,
            connected,
            logged_in,
            url: body["url"].as_str().map(str::to_owned),
            project_id: body["project_id"].as_str().map(str::to_owned),
            scene_id: body["scene_id"].as_str().map(str::to_owned),
            running_tool: body["running_tool"].as_str().map(str::to_owned),
            jobs_in_flight: body["jobs_in_flight"]
                .as_array()
                .cloned()
                .unwrap_or_default(),
            unresolved_receipts: unresolved,
            note,
        })
    }

    /// [`Request::FlowRecentOutputs`].
    pub fn flow_recent_outputs(
        &self,
        account_id: Option<&str>,
    ) -> Result<FlowRecentInfo, ApiError> {
        let (root, accounts) = load_accounts(&self.dirs);
        if accounts.config.accounts.is_empty() && !accounts.message.is_empty() {
            return Err(ApiError::new(ErrorCode::Refused, accounts.message));
        }
        let selected: Vec<Account> = match account_id.map(str::trim).filter(|s| !s.is_empty()) {
            Some(id) => accounts
                .config
                .candidates(id, None)
                .map_err(|msg| ApiError::new(ErrorCode::Refused, msg))?,
            None => accounts.config.accounts,
        };

        let mut total_files = 0usize;
        let mut total_unresolved = 0usize;
        let mut rows = Vec::new();
        for account in selected {
            let state_dir = account.state_dir(&root);
            let summary = flow::recent_outputs(&state_dir);
            let files: Vec<String> = summary
                .files
                .into_iter()
                .map(|p| p.to_string_lossy().into_owned())
                .collect();
            let unresolved: Vec<FlowReceiptInfo> = summary
                .unresolved
                .into_iter()
                .map(|r| receipt_to_info(r, &state_dir))
                .collect();
            total_files += files.len();
            total_unresolved += unresolved.len();
            rows.push(FlowAccountRecentInfo {
                account_id: account.id,
                state_dir: state_dir.to_string_lossy().into_owned(),
                files,
                unresolved,
            });
        }

        Ok(FlowRecentInfo {
            root_dir: root.to_string_lossy().into_owned(),
            total_files,
            total_unresolved,
            accounts: rows,
        })
    }

    fn start_flow_generation(&mut self, spec: FlowGenSpec) -> Result<Reply, ApiError> {
        let (root, account) = pick_account(&self.dirs, spec.account_id.as_deref())?;
        let state_dir = account.state_dir(&root);
        let project_url = resolve_project_url(spec.project_url.as_deref(), &account)?;
        let (model, ratio, count, duration) = validate_generation_params(&spec)?;
        let refs = validate_references(&spec.reference_images)?;

        if let Some(proj) = spec.project_path.as_deref() {
            self.ensure_open(proj)?;
            self.writable(proj)?;
        }

        let auto_confirm = spec.auto_confirm.unwrap_or(true);
        let will_spend = !spec.resume && auto_confirm;
        let unresolved = flow::unresolved_receipts(&state_dir);

        if spec.dry_run {
            let ready = !will_spend
                || (spec.allow_credit && (unresolved.is_empty() || spec.acknowledge_uncertain));
            let op = match spec.kind {
                FlowGenKind::Image => "flow.generateImage",
                FlowGenKind::Video => "flow.generateVideo",
            };
            let note = if will_spend && !spec.allow_credit {
                "Dry run passed validation, but actual submission requires `allowCredit: true`."
                    .to_owned()
            } else if will_spend && !unresolved.is_empty() && !spec.acknowledge_uncertain {
                format!(
                    "{} unresolved/uncertain receipt(s) found on `{}` — inspect `flow_recent_outputs` before generating.",
                    unresolved.len(),
                    account.id
                )
            } else if will_spend {
                format!(
                    "Ready to generate on account `{}` ({}, {}) with credits enabled.",
                    account.id, model, ratio
                )
            } else {
                "Dry run for composer preparation / resume (0 new credits).".to_owned()
            };
            return Ok(Reply::DryRun(DryRunInfo {
                dry_run: true,
                operation: op.to_owned(),
                provider: "google-flow".to_owned(),
                model,
                estimated_characters: spec.prompt.trim().chars().count(),
                estimated_seconds: f64::from(duration.unwrap_or(0)),
                ready,
                note,
            }));
        }

        if will_spend && !spec.allow_credit {
            return Err(ApiError::new(
                ErrorCode::Refused,
                "Google Flow generation spends account credits. Confirm with the user and retry with `allowCredit: true`, or set `autoConfirm: false` to prepare in Flow without submitting.",
            ));
        }

        if will_spend {
            flow::check_ready_for_generation(&state_dir, spec.acknowledge_uncertain)
                .map_err(|msg| ApiError::new(ErrorCode::Refused, msg))?;
        }

        if self.flow_lock.in_use(&state_dir)
            || self.flow_lock.rendering_accounts().contains(&account.id)
        {
            return Err(ApiError::new(
                ErrorCode::Busy,
                format!(
                    "Flow account `{}` is currently busy in another task",
                    account.id
                ),
            ));
        }

        let (tool_name, job_kind) = match spec.kind {
            FlowGenKind::Image => ("flow_generate_image", "flow.generateImage"),
            FlowGenKind::Video => ("flow_generate_video", "flow.generateVideo"),
        };

        let mut args = match spec.kind {
            FlowGenKind::Image => json!({
                "prompt": spec.prompt.trim(),
                "model": model,
                "ratio": ratio,
                "count": count.unwrap_or(1),
                "reference_images": refs,
                "project_url": project_url,
                "auto_confirm": auto_confirm,
                "resume": spec.resume,
            }),
            FlowGenKind::Video => json!({
                "prompt": spec.prompt.trim(),
                "model": model,
                "ratio": ratio,
                "duration": duration.unwrap_or(8),
                "reference_images": refs,
                "project_url": project_url,
                "auto_confirm": auto_confirm,
                "resume": spec.resume,
                "edit_mode": spec.edit_mode,
                "use_direct_api": spec.use_direct_api.unwrap_or(true),
            }),
        };
        if let Some(ref jid) = spec.job_id {
            args["job_id"] = json!(jid);
        }

        let allow_credit = spec.allow_credit;
        let flow_lock = self.flow_lock.clone();
        let proj_key = spec
            .project_path
            .as_deref()
            .map(crate::key_of)
            .unwrap_or_default();
        let auto_import = spec.auto_import.unwrap_or(true);
        let place_on_timeline = spec.place_on_timeline;
        let start = spec.start.unwrap_or(0.0).max(0.0);
        let track_id = spec.track_id;
        let live = self.live_window.clone();
        let pending_edits = Arc::clone(&self.pending_edits);
        let is_image = matches!(spec.kind, FlowGenKind::Image);
        let path_arg = spec.project_path.as_deref().unwrap_or("");

        let started = self.spawn_job(job_kind, path_arg, "", move |job_id, _cancel, events| {
            (events)(Event::JobProgress {
                job: job_id.clone(),
                path: proj_key.clone(),
                stage: "running".to_owned(),
                fraction: 0.1,
            });

            match flow_lock.call_account(&root, &account, tool_name, args, allow_credit) {
                Ok(body) => {
                    let verified = if is_image {
                        flow::generated_images(&state_dir, &body)
                    } else {
                        flow::strict_result_files(&state_dir, &body)
                    };
                    let file_strs: Vec<String> = verified
                        .into_iter()
                        .map(|p| p.to_string_lossy().into_owned())
                        .collect();

                    let mut media_ids = Vec::new();
                    let mut clip_ids = Vec::new();
                    let mut import_errors = Vec::new();

                    if !proj_key.is_empty() && auto_import && !file_strs.is_empty() {
                        let mut steps = Vec::new();
                        for (idx, f) in file_strs.iter().enumerate() {
                            match media::probe(f) {
                                Ok(summary) => {
                                    let item = summary.to_new_media();
                                    if place_on_timeline {
                                        steps.push(PendingStep::ImportAndPlace {
                                            item,
                                            track_id: track_id.clone(),
                                            start: if idx == 0 { start } else { start + (idx as f64) * 5.0 },
                                        });
                                    } else {
                                        steps.push(PendingStep::Apply(Command::AddMedia { item }));
                                    }
                                }
                                Err(e) => import_errors.push(format!("{f}: {e}")),
                            }
                        }
                        if !steps.is_empty() {
                            let (mids, cids, apply_err) = Api::finish_worker_steps(
                                &job_id,
                                &proj_key,
                                steps,
                                &live,
                                &pending_edits,
                            );
                            media_ids = mids;
                            clip_ids = cids;
                            if let Some(err) = apply_err {
                                import_errors.push(err);
                            }
                        }
                    }

                    let post_unresolved: Vec<FlowReceiptInfo> = flow::unresolved_receipts(&state_dir)
                        .into_iter()
                        .map(|r| receipt_to_info(r, &state_dir))
                        .collect();
                    let main_output = file_strs.first().cloned().unwrap_or_default();

                    (events)(Event::JobDone {
                        job: job_id,
                        path: proj_key,
                        output: main_output,
                        result: Some(json!({
                            "accountId": account.id,
                            "status": body["status"],
                            "jobId": body["job_id"],
                            "model": model,
                            "ratio": ratio,
                            "duration": duration,
                            "approvalText": body["approval_text"],
                            "files": file_strs,
                            "mediaIds": media_ids,
                            "clipIds": clip_ids,
                            "importError": (!import_errors.is_empty()).then(|| import_errors.join("; ")),
                            "unresolvedReceipts": post_unresolved,
                        })),
                    });
                }
                Err(err) => {
                    // Never auto-retry. Report any `uncertain` receipt explicitly so caller checks Flow before re-submitting.
                    let post_unresolved = flow::unresolved_receipts(&state_dir);
                    let msg = if post_unresolved.is_empty() {
                        err
                    } else {
                        let ids: Vec<&str> = post_unresolved.iter().map(|r| r.id.as_str()).collect();
                        format!(
                            "{err} (unresolved receipt(s) saved: {}; do NOT retry automatically — inspect Flow or call `flow_recent_outputs` / `resume: true`)",
                            ids.join(", ")
                        )
                    };
                    (events)(Event::JobFailed {
                        job: job_id,
                        path: proj_key,
                        error: ApiError::failed(msg),
                    });
                }
            }
        })?;

        Ok(Reply::Started(started))
    }

    pub(crate) fn run_flow_request(&mut self, request: Request) -> Result<Reply, ApiError> {
        match request {
            Request::FlowAccountsList => Ok(Reply::FlowAccounts(self.flow_accounts_list())),
            Request::FlowConnect {
                account_id,
                open_url,
            } => Ok(Reply::FlowConnection(
                self.flow_connect(account_id.as_deref(), open_url.as_deref())?,
            )),
            Request::FlowStatus { account_id } => {
                Ok(Reply::FlowStatus(self.flow_status(account_id.as_deref())?))
            }
            Request::FlowRecentOutputs { account_id } => Ok(Reply::FlowRecent(
                self.flow_recent_outputs(account_id.as_deref())?,
            )),
            Request::FlowGenerateImage {
                account_id,
                prompt,
                model,
                ratio,
                count,
                reference_images,
                project_url,
                auto_confirm,
                resume,
                job_id,
                allow_credit,
                acknowledge_uncertain,
                project_path,
                auto_import,
                place_on_timeline,
                start,
                track_id,
                dry_run,
            } => self.start_flow_generation(FlowGenSpec {
                kind: FlowGenKind::Image,
                account_id,
                prompt,
                model,
                ratio,
                count,
                duration: None,
                reference_images,
                project_url,
                auto_confirm,
                resume,
                job_id,
                edit_mode: false,
                use_direct_api: None,
                allow_credit,
                acknowledge_uncertain,
                project_path,
                auto_import,
                place_on_timeline,
                start,
                track_id,
                dry_run,
            }),
            Request::FlowGenerateVideo {
                account_id,
                prompt,
                model,
                ratio,
                duration,
                reference_images,
                project_url,
                auto_confirm,
                resume,
                job_id,
                edit_mode,
                use_direct_api,
                allow_credit,
                acknowledge_uncertain,
                project_path,
                auto_import,
                place_on_timeline,
                start,
                track_id,
                dry_run,
            } => self.start_flow_generation(FlowGenSpec {
                kind: FlowGenKind::Video,
                account_id,
                prompt,
                model,
                ratio,
                count: None,
                duration,
                reference_images,
                project_url,
                auto_confirm,
                resume,
                job_id,
                edit_mode,
                use_direct_api,
                allow_credit,
                acknowledge_uncertain,
                project_path,
                auto_import,
                place_on_timeline,
                start,
                track_id,
                dry_run,
            }),
            _ => unreachable!("non-flow request routed to run_flow_request"),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::sync::{Arc, Mutex};
    use std::time::{Duration, Instant};

    use concat_host::flow::FlowAccountLock;
    use serde_json::json;

    use crate::message::{ErrorCode, JobState, Reply, Request};
    use crate::{Api, AppDirs, write_png};

    const VALID_PROJECT_URL: &str =
        "https://flow.google.com/project/01234567-89ab-cdef-0123-456789abcdef";

    fn test_api() -> (Api, tempfile::TempDir) {
        let scratch = tempfile::tempdir().expect("scratch");
        let dirs = AppDirs {
            config: scratch.path().join("config"),
            data: scratch.path().join("data"),
        };
        fs::create_dir_all(&dirs.config).expect("config");
        fs::create_dir_all(&dirs.data).expect("data");
        let sink = Arc::new(Mutex::new(Vec::new()));
        let api = Api::with_dirs(dirs, Arc::new(move |ev| sink.lock().expect("ev").push(ev)));
        (api, scratch)
    }

    #[test]
    fn flow_generation_requires_explicit_allow_credit_and_honors_uncertain_receipts() {
        let (mut api, scratch) = test_api();

        let accounts_reply = match api.dispatch(Request::FlowAccountsList) {
            crate::Response::Result(Reply::FlowAccounts(info)) => info,
            other => panic!(
                "expected FlowAccounts: {}",
                serde_json::to_string(&other).unwrap()
            ),
        };
        assert_eq!(accounts_reply.accounts.len(), 1);
        assert_eq!(accounts_reply.accounts[0].id, "primary");

        // Credit spend refused without allowCredit: true.
        let refused = match api.dispatch(Request::FlowGenerateVideo {
            account_id: None,
            prompt: "cinematic drone shot".to_owned(),
            model: None,
            ratio: None,
            duration: Some(8),
            reference_images: Vec::new(),
            project_url: Some(VALID_PROJECT_URL.to_owned()),
            auto_confirm: Some(true),
            resume: false,
            job_id: None,
            edit_mode: false,
            use_direct_api: None,
            allow_credit: false,
            acknowledge_uncertain: false,
            project_path: None,
            auto_import: None,
            place_on_timeline: false,
            start: None,
            track_id: None,
            dry_run: false,
        }) {
            crate::Response::Error(err) => err,
            other => panic!(
                "expected refusal: {}",
                serde_json::to_string(&other).unwrap()
            ),
        };
        assert_eq!(refused.code, ErrorCode::Refused);
        assert!(refused.message.contains("allowCredit"));

        // Write an uncertain receipt + a valid finished image receipt in primary state dir.
        let state_dir = api.dirs().data.join("google-flow");
        fs::create_dir_all(state_dir.join("receipts")).unwrap();
        fs::create_dir_all(state_dir.join("output/old")).unwrap();
        let saved_png = state_dir.join("output/old/frame.png");
        write_png(&saved_png, 1, 1, &[0, 255, 0, 255]).unwrap();

        fs::write(
            state_dir.join("receipts/finished.json"),
            json!({
                "id": "finished",
                "phase": "finished",
                "tool": "flow_generate_image",
                "updated": "2026-11-16T00:00:00Z",
                "result": { "files": [saved_png] }
            })
            .to_string(),
        )
        .unwrap();
        fs::write(
            state_dir.join("receipts/stuck-123.json"),
            json!({
                "id": "stuck-123",
                "phase": "uncertain",
                "tool": "flow_generate_video",
                "updated": "2026-11-16T01:00:00Z",
                "error": "bridge EOF"
            })
            .to_string(),
        )
        .unwrap();

        let recent = match api.dispatch(Request::FlowRecentOutputs { account_id: None }) {
            crate::Response::Result(Reply::FlowRecent(info)) => info,
            other => panic!(
                "expected FlowRecent: {}",
                serde_json::to_string(&other).unwrap()
            ),
        };
        assert_eq!(recent.total_files, 1);
        assert_eq!(recent.total_unresolved, 1);
        assert_eq!(recent.accounts[0].unresolved[0].id, "stuck-123");

        // Even with allowCredit: true, uncertain receipt blocks new submissions unless acknowledged.
        let blocked = match api.dispatch(Request::FlowGenerateImage {
            account_id: None,
            prompt: "portrait".to_owned(),
            model: None,
            ratio: None,
            count: Some(1),
            reference_images: Vec::new(),
            project_url: Some(VALID_PROJECT_URL.to_owned()),
            auto_confirm: Some(true),
            resume: false,
            job_id: None,
            allow_credit: true,
            acknowledge_uncertain: false,
            project_path: None,
            auto_import: None,
            place_on_timeline: false,
            start: None,
            track_id: None,
            dry_run: false,
        }) {
            crate::Response::Error(err) => err,
            other => panic!(
                "expected uncertain receipt refusal: {}",
                serde_json::to_string(&other).unwrap()
            ),
        };
        assert_eq!(blocked.code, ErrorCode::Refused);
        assert!(blocked.message.contains("stuck-123"));
        drop(scratch);
    }

    #[cfg(not(any(target_os = "android", target_os = "ios")))]
    #[test]
    fn mock_flow_worker_generates_image_and_auto_imports_into_project() {
        let (mut api, scratch) = test_api();

        // Create mock worker mimicking bridge.mjs stdio protocol.
        let worker_dir = scratch.path().join("mock-worker");
        fs::create_dir_all(worker_dir.join("dist")).unwrap();
        fs::write(worker_dir.join("dist/cli.js"), "").unwrap();
        fs::write(
            worker_dir.join("bridge.mjs"),
            r#"
import fs from 'node:fs';
import path from 'node:path';
import readline from 'node:readline';

const stateDir = process.argv[2];
fs.mkdirSync(path.join(stateDir, 'receipts'), { recursive: true });
for await (const line of readline.createInterface({ input: process.stdin, crlfDelay: Infinity })) {
  if (!line.trim()) continue;
  const req = JSON.parse(line);
  if (req.tool === 'flow_generate_image') {
    if (req.args.auto_confirm !== false && req.allowCredit !== true) {
      process.stdout.write(JSON.stringify({ id: req.id, result: { ok: false, message: 'Credit authorization required' } }) + '\n');
      continue;
    }
    const outDir = path.join(stateDir, 'output', req.id);
    fs.mkdirSync(outDir, { recursive: true });
    const outFile = path.join(outDir, 'gen.png');
    fs.copyFileSync(req.args.reference_images[0], outFile);
    const body = { ok: true, status: 'completed', job_id: req.id, files: [outFile], media_ids: ['abcd1234'], approval_text: '' };
    fs.writeFileSync(path.join(stateDir, 'receipts', req.id + '.json'), JSON.stringify({ id: req.id, phase: 'finished', tool: req.tool, updated: new Date().toISOString(), result: body }));
    process.stdout.write(JSON.stringify({ id: req.id, result: body }) + '\n');
  } else {
    process.stdout.write(JSON.stringify({ id: req.id, result: { ok: true } }) + '\n');
  }
}
"#,
        )
        .unwrap();

        let lock = FlowAccountLock::with_worker_root(worker_dir);
        api.share_flow_lock(lock.clone());

        let loc = scratch.path().to_string_lossy().into_owned();
        api.create(&loc, "FlowProj", Default::default())
            .expect("create");
        let proj = scratch
            .path()
            .join("FlowProj")
            .to_string_lossy()
            .into_owned();

        let ref_png = scratch.path().join("ref.png");
        write_png(&ref_png, 1, 1, &[255, 0, 0, 255]).unwrap();

        let started = match api.dispatch(Request::FlowGenerateImage {
            account_id: None,
            prompt: "test character portrait".to_owned(),
            model: Some("nano-banana-pro".to_owned()),
            ratio: Some("1:1".to_owned()),
            count: Some(1),
            reference_images: vec![ref_png.to_string_lossy().into_owned()],
            project_url: Some(VALID_PROJECT_URL.to_owned()),
            auto_confirm: Some(true),
            resume: false,
            job_id: None,
            allow_credit: true,
            acknowledge_uncertain: false,
            project_path: Some(proj.clone()),
            auto_import: Some(true),
            place_on_timeline: false,
            start: None,
            track_id: None,
            dry_run: false,
        }) {
            crate::Response::Result(Reply::Started(started)) => started,
            other => panic!(
                "expected Started: {}",
                serde_json::to_string(&other).unwrap()
            ),
        };

        let deadline = Instant::now() + Duration::from_secs(15);
        let status = loop {
            match api.dispatch(Request::JobStatus {
                job: started.job.clone(),
            }) {
                crate::Response::Result(Reply::Job(status))
                    if status.state != JobState::Running =>
                {
                    break status;
                }
                crate::Response::Result(Reply::Job(_)) if Instant::now() < deadline => {
                    std::thread::sleep(Duration::from_millis(25));
                }
                other => panic!(
                    "job did not complete in time: {}",
                    serde_json::to_string(&other).unwrap()
                ),
            }
        };

        assert_eq!(status.state, JobState::Done, "error: {:?}", status.error);
        let res = status.result.expect("result json");
        let media_ids = res["mediaIds"].as_array().expect("mediaIds");
        assert_eq!(media_ids.len(), 1);
        assert!(res["importError"].is_null());

        let view = match api.dispatch(Request::ProjectGet { path: proj }) {
            crate::Response::Result(Reply::View(view)) => *view,
            other => panic!("expected View: {}", serde_json::to_string(&other).unwrap()),
        };
        assert_eq!(view.project.media.len(), 1);

        lock.shutdown_all();
    }
}
