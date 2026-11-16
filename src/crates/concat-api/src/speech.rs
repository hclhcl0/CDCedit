// SPDX-License-Identifier: AGPL-3.0-or-later
//! AI Speech & Caption Studio endpoints behind [`crate::Api`].

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::Ordering;

use concat_host::media;
use concat_host::secrets::ServiceKeys;
use concat_project::Command;
use concat_project::commands::ClipPatch;
use concat_project::model::{ClipKind, TextStyle};
use concat_speech::dubbing::{self, Cue};
use concat_speech::transcribe::TranscribeRequest;
use concat_speech::tts::{Family, SpeakRequest, VoiceInfo};
use concat_speech::{
    JobStage, Speech, Transcriber, api_keys, cloud_tts, gemini, gemini_studio, translation,
};
use serde_json::json;

use crate::message::{
    ApiError, CaptionLineSpec, DialogueTurnSpec, DryRunInfo, DubCueSpec, ErrorCode, Event, Reply,
    Request, SpeechStatusInfo, SpeechTranscriberInfo, SpeechTtsModelInfo, SpeechVoiceInfo, Started,
};
use crate::{Api, PendingStep};

fn family_name(family: Family) -> String {
    match family {
        Family::Kokoro => "kokoro".to_owned(),
        Family::Pocket => "pocket".to_owned(),
        Family::Chatterbox => "chatterbox".to_owned(),
    }
}

struct AudioSource {
    path: String,
    audio_stream: Option<u32>,
    source_start: f64,
    window: f64,
    clip_start: f64,
    clip_speed: f64,
}

fn resolve_source_clip(
    api: &Api,
    path: &str,
    clip_id: Option<&str>,
    media_path: Option<&str>,
) -> Result<AudioSource, ApiError> {
    match (clip_id, media_path) {
        (Some(cid), None) => {
            let live_snapshot = match api.live_handler_for(path) {
                Some(handler) => Some(handler.snapshot_project(path)?),
                None => None,
            };
            let session = match live_snapshot.as_ref() {
                Some(snapshot) => snapshot,
                None => api.session(path)?,
            };
            let project = session.project();
            let clip = project
                .active()
                .clips
                .iter()
                .find(|c| c.id == cid)
                .ok_or_else(|| {
                    ApiError::new(
                        ErrorCode::NotFound,
                        format!("clip {cid} not found on active timeline"),
                    )
                })?;
            let media_item = project.media_by_id(&clip.media_id).ok_or_else(|| {
                ApiError::new(
                    ErrorCode::NotFound,
                    format!("media {} for clip {cid} is missing", clip.media_id),
                )
            })?;
            Ok(AudioSource {
                path: media_item.path.clone(),
                audio_stream: clip.audio_stream,
                source_start: clip.source_start,
                window: (clip.duration * clip.speed).max(0.05),
                clip_start: clip.start,
                clip_speed: clip.speed.max(0.05),
            })
        }
        (None, Some(file)) => {
            let summary = media::probe(file).map_err(ApiError::failed)?;
            let duration = summary.duration.unwrap_or(0.0);
            if duration <= 0.0 {
                return Err(ApiError::invalid("media file reports no duration"));
            }
            Ok(AudioSource {
                path: file.to_owned(),
                audio_stream: None,
                source_start: 0.0,
                window: duration,
                clip_start: 0.0,
                clip_speed: 1.0,
            })
        }
        (Some(_), Some(_)) => Err(ApiError::invalid(
            "pass either `clipId` or `mediaPath`, not both",
        )),
        (None, None) => Err(ApiError::invalid("pass either `clipId` or `mediaPath`")),
    }
}

fn find_local_voice(voices: &[VoiceInfo], raw: &str) -> Result<VoiceInfo, ApiError> {
    let trimmed = raw.trim();
    if let Ok(id) = trimmed.parse::<i32>()
        && let Some(found) = voices.iter().find(|v| v.id == id)
    {
        return Ok(found.clone());
    }
    if let Some(found) = voices.iter().find(|v| v.name.eq_ignore_ascii_case(trimmed)) {
        return Ok(found.clone());
    }
    Err(ApiError::invalid(format!(
        "unknown local voice {trimmed:?}; call `models_speech_status` for valid voice ids and names"
    )))
}

fn default_local_model_for(
    status: &concat_speech::tts::TtsStatus,
    family: Family,
    explicit: Option<&str>,
) -> Result<String, ApiError> {
    if let Some(model) = explicit.map(str::trim).filter(|m| !m.is_empty()) {
        return Ok(model.to_owned());
    }
    status
        .models
        .iter()
        .find(|m| m.family == family && m.downloaded)
        .or_else(|| status.models.iter().find(|m| m.family == family))
        .map(|m| m.id.clone())
        .ok_or_else(|| ApiError::failed("no matching local TTS model available"))
}

fn finish_audio_output(
    api_live: &Option<Arc<dyn crate::LiveWindowHandler>>,
    pending_edits: &Arc<std::sync::Mutex<Vec<crate::PendingSessionEdit>>>,
    job_id: &str,
    project_path: &str,
    out_path: &Path,
    place_on_timeline: bool,
    start: f64,
    track_id: Option<String>,
    extra_result: serde_json::Value,
) -> (Vec<String>, Vec<String>, Option<String>, serde_json::Value) {
    let out_str = out_path.to_string_lossy().into_owned();
    let probed = match media::probe(&out_str) {
        Ok(summary) => summary.to_new_media(),
        Err(e) => {
            return (Vec::new(), Vec::new(), Some(e), extra_result);
        }
    };
    let steps = if place_on_timeline {
        vec![PendingStep::ImportAndPlace {
            item: probed,
            track_id,
            start: start.max(0.0),
        }]
    } else {
        vec![PendingStep::Apply(Command::AddMedia { item: probed })]
    };
    let (media_ids, clip_ids, import_err) =
        Api::finish_worker_steps(job_id, project_path, steps, api_live, pending_edits);
    (media_ids, clip_ids, import_err, extra_result)
}

impl Api {
    /// [`Request::ModelsSpeechStatus`].
    pub fn models_speech_status(&self) -> Result<SpeechStatusInfo, ApiError> {
        let keys = ServiceKeys::load(&self.dirs);
        let gem_count = api_keys::parse(keys.gemini_api_key.expose()).len();
        let elev_count = api_keys::parse(keys.eleven_api_key.expose()).len();

        let tx_status = Transcriber::status(&self.dirs).map_err(ApiError::failed)?;
        let tts_status = Speech::status(&self.dirs).map_err(ApiError::failed)?;

        Ok(SpeechStatusInfo {
            transcribers_dir: tx_status.models_dir,
            transcribers: tx_status
                .models
                .into_iter()
                .map(|m| SpeechTranscriberInfo {
                    id: m.id,
                    label: m.label,
                    blurb: m.blurb,
                    english_only: m.english_only,
                    size_bytes: m.size_bytes,
                    downloaded: m.downloaded,
                })
                .collect(),
            tts_models_dir: tts_status.models_dir,
            tts_models: tts_status
                .models
                .into_iter()
                .map(|m| SpeechTtsModelInfo {
                    id: m.id,
                    family: family_name(m.family),
                    label: m.label,
                    blurb: m.blurb,
                    size_bytes: m.size_bytes,
                    downloaded: m.downloaded,
                })
                .collect(),
            voices: tts_status
                .voices
                .into_iter()
                .map(|v| SpeechVoiceInfo {
                    id: v.id,
                    name: v.name,
                    family: family_name(v.family),
                })
                .collect(),
            gemini_configured: gem_count > 0,
            gemini_key_count: gem_count,
            eleven_configured: elev_count > 0,
            eleven_key_count: elev_count,
        })
    }

    /// [`Request::CaptionsTranscribe`].
    #[allow(clippy::too_many_arguments)]
    pub fn captions_transcribe(
        &mut self,
        path: &str,
        clip_id: Option<&str>,
        media_path: Option<&str>,
        engine: Option<&str>,
        model: Option<&str>,
        language: Option<&str>,
        two_speakers: bool,
        smart_split: bool,
        glossary: Option<&str>,
        auto_apply: bool,
        offset_y: f64,
        dry_run: bool,
    ) -> Result<Reply, ApiError> {
        self.ensure_open(path)?;
        self.writable(path)?;
        let source = resolve_source_clip(self, path, clip_id, media_path)?;
        let engine_kind = engine.unwrap_or("whisper").trim().to_ascii_lowercase();
        if !matches!(engine_kind.as_str(), "whisper" | "gemini") {
            return Err(ApiError::invalid(format!(
                "unknown engine {engine_kind:?}; expected \"whisper\" or \"gemini\""
            )));
        }

        let keys = ServiceKeys::load(&self.dirs);
        let gemini_keys = keys.gemini_api_key.expose().to_owned();
        let chosen_model = if engine_kind == "gemini" {
            model.unwrap_or("gemini-3.5-transcribe").to_owned()
        } else {
            let status = Transcriber::status(&self.dirs).map_err(ApiError::failed)?;
            if let Some(m) = model.map(str::trim).filter(|s| !s.is_empty()) {
                let found = status.models.iter().find(|entry| entry.id == m);
                if !found.is_some_and(|entry| entry.downloaded) {
                    return Err(ApiError::new(
                        ErrorCode::NotFound,
                        format!(
                            "Whisper model `{m}` is not downloaded; install it in Settings > Transcriber"
                        ),
                    ));
                }
                m.to_owned()
            } else {
                status
                    .models
                    .into_iter()
                    .find(|m| m.downloaded)
                    .map(|m| m.id)
                    .ok_or_else(|| {
                        ApiError::new(
                            ErrorCode::NotFound,
                            "No Whisper model is downloaded; install one in Settings > Transcriber or use engine \"gemini\"",
                        )
                    })?
            }
        };

        let key_ready = engine_kind == "whisper" || !api_keys::parse(&gemini_keys).is_empty();
        if !key_ready && !dry_run {
            return Err(ApiError::new(
                ErrorCode::Refused,
                "Gemini API key is not configured in Settings > API keys",
            ));
        }

        if dry_run {
            return Ok(Reply::DryRun(DryRunInfo {
                dry_run: true,
                operation: "captions.transcribe".to_owned(),
                provider: engine_kind,
                model: chosen_model,
                estimated_characters: 0,
                estimated_seconds: source.window,
                ready: key_ready,
                note: if key_ready {
                    "Ready to transcribe".to_owned()
                } else {
                    "Missing Gemini API key in Settings".to_owned()
                },
            }));
        }

        let lang = language.unwrap_or("vi").to_owned();
        let vietnamese = lang == "vi";
        let glossary = glossary.unwrap_or_default().to_owned();
        let dirs = self.dirs.clone();
        let transcriber = Arc::clone(&self.transcriber);
        let live = self.live_window.clone();
        let pending_edits = Arc::clone(&self.pending_edits);
        let project_path = crate::key_of(path);

        let started = self.spawn_job(
            "captions.transcribe",
            path,
            &source.path,
            move |job_id, cancel, events| {
                let req = TranscribeRequest {
                    path: source.path,
                    audio_stream: source.audio_stream,
                    source_start: source.source_start,
                    window: source.window,
                    model_id: chosen_model.clone(),
                };
                let events_p = Arc::clone(&events);
                let job_p = job_id.clone();
                let path_p = project_path.clone();
                let mut report = move |stage: JobStage| {
                    let (label, frac) = match stage {
                        JobStage::Reading => ("reading audio", 0.05),
                        JobStage::Loading => ("loading model", 0.15),
                        JobStage::Working { done, .. } => {
                            ("transcribing", 0.2 + 0.75 * f64::from(done))
                        }
                    };
                    (events_p)(Event::JobProgress {
                        job: job_p.clone(),
                        path: path_p.clone(),
                        stage: label.to_owned(),
                        fraction: frac,
                    });
                };

                let result: Result<Vec<(f64, f64, String, usize)>, String> = if engine_kind
                    == "gemini"
                {
                    let raw = gemini::transcribe_with_options(
                        &req,
                        &gemini_keys,
                        vietnamese,
                        two_speakers,
                        &glossary,
                        &cancel,
                        &mut report,
                    );
                    let final_segs = if smart_split {
                        match raw {
                            Ok(segs) if !segs.is_empty() => {
                                gemini::resegment_diarized_for_dubbing(&segs, &gemini_keys, &cancel)
                                    .unwrap_or(segs)
                            }
                            other => other?,
                        }
                    } else {
                        raw?
                    };
                    Ok(final_segs
                        .into_iter()
                        .map(|s| (s.start, s.end, s.text, s.speaker))
                        .collect())
                } else {
                    let segs =
                        transcriber.transcribe_with_language(&dirs, &req, Some(&lang), report);
                    segs.map(|list| {
                        list.into_iter()
                            .map(|s| (s.start, s.end, s.text, 0usize))
                            .collect()
                    })
                };

                match result {
                    Ok(raw_segments) => {
                        if cancel.load(Ordering::Relaxed) {
                            (events)(Event::JobFailed {
                                job: job_id,
                                path: project_path,
                                error: ApiError::new(
                                    ErrorCode::Cancelled,
                                    "Transcription was cancelled",
                                ),
                            });
                            return;
                        }
                        let timeline_segments: Vec<(f64, f64, String, usize)> = raw_segments
                            .into_iter()
                            .filter(|(_, _, text, _)| !text.trim().is_empty())
                            .map(|(s, e, text, speaker)| {
                                let ts = source.clip_start + s / source.clip_speed;
                                let te = (source.clip_start + e / source.clip_speed).max(ts + 0.1);
                                (ts, te, text, speaker)
                            })
                            .collect();

                        let mut clip_ids = Vec::new();
                        let mut apply_err = None;
                        if auto_apply && !timeline_segments.is_empty() {
                            let steps: Vec<PendingStep> = timeline_segments
                                .iter()
                                .map(|(ts, te, text, speaker)| {
                                    PendingStep::Apply(Command::AddTextClip {
                                        above: true,
                                        track_id: None,
                                        start: *ts,
                                        style: Some(TextStyle {
                                            content: text.clone(),
                                            font_family: "Hanken Grotesk".to_owned(),
                                            font_size: 0.04,
                                            font_weight: 600.0,
                                            color: if *speaker == 1 {
                                                "#ffd400".to_owned()
                                            } else {
                                                "#ffffff".to_owned()
                                            },
                                            ..TextStyle::default()
                                        }),
                                        duration: Some(*te - *ts),
                                        offset_y: Some(offset_y),
                                    })
                                })
                                .collect();
                            let (_, ids, err) = Api::finish_worker_steps(
                                &job_id,
                                &project_path,
                                steps,
                                &live,
                                &pending_edits,
                            );
                            clip_ids = ids;
                            apply_err = err;
                        }

                        let seg_json: Vec<_> = timeline_segments
                            .iter()
                            .map(|(s, e, t, sp)| {
                                json!({ "start": s, "end": e, "text": t, "speaker": sp })
                            })
                            .collect();
                        (events)(Event::JobDone {
                            job: job_id,
                            path: project_path,
                            output: String::new(),
                            result: Some(json!({
                                "engine": engine_kind,
                                "model": chosen_model,
                                "segments": seg_json,
                                "clipIds": clip_ids,
                                "applyError": apply_err,
                            })),
                        });
                    }
                    Err(e) => {
                        let clean = keys.redact(&e);
                        (events)(Event::JobFailed {
                            job: job_id,
                            path: project_path,
                            error: if cancel.load(Ordering::Relaxed) {
                                ApiError::new(ErrorCode::Cancelled, "Transcription was cancelled")
                            } else {
                                ApiError::failed(clean)
                            },
                        });
                    }
                }
            },
        )?;
        Ok(Reply::Started(started))
    }

    /// [`Request::CaptionsTranslate`].
    #[allow(clippy::too_many_arguments)]
    pub fn captions_translate(
        &mut self,
        path: Option<&str>,
        track_id: Option<&str>,
        lines: Option<Vec<CaptionLineSpec>>,
        target_language: &str,
        model: Option<&str>,
        glossary: Option<&str>,
        natural: bool,
        condense_to_fit: bool,
        apply_to_track: bool,
        dry_run: bool,
    ) -> Result<Reply, ApiError> {
        let target_language = target_language.trim();
        if target_language.is_empty() {
            return Err(ApiError::invalid("`targetLanguage` is required"));
        }
        let mut clip_refs: Vec<(String, TextStyle)> = Vec::new();
        let source_lines: Vec<translation::Line> = match lines {
            Some(list) if !list.is_empty() => list
                .into_iter()
                .map(|l| translation::Line {
                    start: l.start,
                    end: l.end,
                    source: l.text,
                    translated: String::new(),
                    speaker: l.speaker,
                })
                .collect(),
            Some(_) => {
                return Err(ApiError::invalid(
                    "`lines` must contain at least one subtitle",
                ));
            }
            None => {
                let project_path = path.ok_or_else(|| {
                    ApiError::invalid("provide `lines`, or an open project `path` with text clips")
                })?;
                self.ensure_open(project_path)?;
                let live_snapshot = match self.live_handler_for(project_path) {
                    Some(handler) => Some(handler.snapshot_project(project_path)?),
                    None => None,
                };
                let session = match live_snapshot.as_ref() {
                    Some(snapshot) => snapshot,
                    None => self.session(project_path)?,
                };
                let tl = session.project().active();
                let mut rows: Vec<_> = tl
                    .clips
                    .iter()
                    .filter(|c| c.kind == ClipKind::Text)
                    .filter(|c| track_id.is_none_or(|tid| c.track_id == tid))
                    .filter_map(|c| {
                        let style = c.text.clone()?;
                        if style.content.trim().is_empty() {
                            return None;
                        }
                        Some((c.id.clone(), c.start, c.start + c.duration, style))
                    })
                    .collect();
                rows.sort_by(|a, b| a.1.total_cmp(&b.1));
                if rows.is_empty() {
                    return Err(ApiError::new(
                        ErrorCode::NotFound,
                        "No subtitle/text clips found on the selected track",
                    ));
                }
                let mut collected = Vec::with_capacity(rows.len());
                for (cid, start, end, style) in rows {
                    collected.push(translation::Line {
                        start,
                        end,
                        source: style.content.clone(),
                        translated: String::new(),
                        speaker: 0,
                    });
                    clip_refs.push((cid, style));
                }
                collected
            }
        };

        let chars: usize = source_lines.iter().map(|l| l.source.chars().count()).sum();
        let chosen_model = model.unwrap_or("gemini-3.8-flash").to_owned();
        let keys = ServiceKeys::load(&self.dirs);
        let gemini_keys = keys.gemini_api_key.expose().to_owned();
        let key_ready = !api_keys::parse(&gemini_keys).is_empty();
        if !key_ready && !dry_run {
            return Err(ApiError::new(
                ErrorCode::Refused,
                "Gemini API key is not configured in Settings > API keys",
            ));
        }
        if dry_run {
            return Ok(Reply::DryRun(DryRunInfo {
                dry_run: true,
                operation: "captions.translate".to_owned(),
                provider: "gemini".to_owned(),
                model: chosen_model,
                estimated_characters: chars,
                estimated_seconds: 0.0,
                ready: key_ready,
                note: format!(
                    "{} subtitle line(s), {} characters",
                    source_lines.len(),
                    chars
                ),
            }));
        }

        let target_lang = target_language.to_owned();
        let glossary_str = glossary.unwrap_or_default().to_owned();
        let proj_path = path.map(crate::key_of).unwrap_or_default();
        let live = self.live_window.clone();
        let pending_edits = Arc::clone(&self.pending_edits);

        let started = self.spawn_job(
            "captions.translate",
            path.unwrap_or(""),
            "",
            move |job_id, cancel, events| {
                let events_p = Arc::clone(&events);
                let job_p = job_id.clone();
                let path_p = proj_path.clone();
                let res = translation::translate(
                    &source_lines,
                    None,
                    &gemini_keys,
                    &chosen_model,
                    &target_lang,
                    natural,
                    &glossary_str,
                    condense_to_fit,
                    &cancel,
                    |done, total| {
                        let frac = if total > 0 {
                            done as f64 / total as f64
                        } else {
                            0.0
                        };
                        (events_p)(Event::JobProgress {
                            job: job_p.clone(),
                            path: path_p.clone(),
                            stage: format!("translating {done}/{total}"),
                            fraction: frac,
                        });
                    },
                );

                match res {
                    Ok(pairs) => {
                        let mut translated_lines = source_lines.clone();
                        for (idx, text) in &pairs {
                            if let Some(row) = translated_lines.get_mut(*idx) {
                                row.translated = text.clone();
                            }
                        }
                        let mut clip_ids = Vec::new();
                        let mut apply_err = None;
                        if apply_to_track && !clip_refs.is_empty() && !proj_path.is_empty() {
                            let steps: Vec<PendingStep> = pairs
                                .iter()
                                .filter_map(|(idx, text)| {
                                    let (cid, style) = clip_refs.get(*idx)?;
                                    let mut next_style = style.clone();
                                    next_style.content = text.clone();
                                    clip_ids.push(cid.clone());
                                    Some(PendingStep::Apply(Command::UpdateClip {
                                        clip_id: cid.clone(),
                                        patch: ClipPatch {
                                            text: Some(Some(next_style)),
                                            ..Default::default()
                                        },
                                    }))
                                })
                                .collect();
                            let (_, _, err) = Api::finish_worker_steps(
                                &job_id,
                                &proj_path,
                                steps,
                                &live,
                                &pending_edits,
                            );
                            apply_err = err;
                        }
                        let out_rows: Vec<_> = translated_lines
                            .iter()
                            .enumerate()
                            .map(|(i, row)| {
                                json!({
                                    "index": i,
                                    "start": row.start,
                                    "end": row.end,
                                    "source": row.source,
                                    "translated": row.translated,
                                    "warning": translation::warning(row),
                                })
                            })
                            .collect();
                        (events)(Event::JobDone {
                            job: job_id,
                            path: proj_path,
                            output: String::new(),
                            result: Some(json!({
                                "targetLanguage": target_lang,
                                "translations": out_rows,
                                "updatedClipIds": clip_ids,
                                "applyError": apply_err,
                            })),
                        });
                    }
                    Err(e) => {
                        let clean = keys.redact(&e);
                        (events)(Event::JobFailed {
                            job: job_id,
                            path: proj_path,
                            error: if cancel.load(Ordering::Relaxed) {
                                ApiError::new(ErrorCode::Cancelled, "Translation was cancelled")
                            } else {
                                ApiError::failed(clean)
                            },
                        });
                    }
                }
            },
        )?;
        Ok(Reply::Started(started))
    }

    /// [`Request::SpeechTts`].
    #[allow(clippy::too_many_arguments)]
    pub fn speech_tts(
        &mut self,
        path: &str,
        text: Option<String>,
        turns: Option<Vec<DialogueTurnSpec>>,
        provider: Option<&str>,
        model: Option<&str>,
        voice_id: &str,
        voice_b_id: Option<&str>,
        style: Option<&str>,
        speed: Option<f32>,
        pauses: Option<f32>,
        temperature: Option<f32>,
        place_on_timeline: bool,
        start: f64,
        track_id: Option<String>,
        dry_run: bool,
    ) -> Result<Reply, ApiError> {
        self.ensure_open(path)?;
        self.writable(path)?;
        let provider_name = provider.unwrap_or("local").trim().to_ascii_lowercase();
        if !matches!(provider_name.as_str(), "local" | "gemini" | "elevenlabs") {
            return Err(ApiError::invalid(format!(
                "unknown provider {provider_name:?}; expected \"local\", \"gemini\", or \"elevenlabs\""
            )));
        }

        let char_count = if let Some(ref t) = text {
            if t.trim().is_empty() {
                return Err(ApiError::invalid("`text` must not be empty"));
            }
            t.chars().count()
        } else if let Some(ref list) = turns {
            if list.is_empty() {
                return Err(ApiError::invalid("`turns` must not be empty"));
            }
            list.iter().map(|t| t.text.chars().count()).sum()
        } else {
            return Err(ApiError::invalid("provide either `text` or `turns`"));
        };

        let keys = ServiceKeys::load(&self.dirs);
        let gemini_keys = keys.gemini_api_key.expose().to_owned();
        let eleven_keys = keys.eleven_api_key.expose().to_owned();

        let (chosen_model, local_voice_num, ready, note) = match provider_name.as_str() {
            "local" => {
                if turns.is_some() && text.is_none() {
                    return Err(ApiError::invalid(
                        "`turns` dialogue mode requires `provider: \"gemini\"`",
                    ));
                }
                let status = Speech::status(&self.dirs).map_err(ApiError::failed)?;
                let voice = find_local_voice(&status.voices, voice_id)?;
                let m = default_local_model_for(&status, voice.family, model)?;
                let installed = status
                    .models
                    .iter()
                    .any(|entry| entry.id == m && entry.downloaded);
                let msg = if installed {
                    "Offline TTS ready".to_owned()
                } else {
                    format!(
                        "Local TTS model `{m}` is not downloaded; install it in Settings > Speech"
                    )
                };
                (m, Some(voice.id), installed, msg)
            }
            "gemini" => {
                let m = model.unwrap_or(cloud_tts::GEMINI_MODELS[1]).to_owned();
                let ok = !api_keys::parse(&gemini_keys).is_empty();
                (
                    m,
                    None,
                    ok,
                    if ok {
                        "Gemini TTS ready".to_owned()
                    } else {
                        "Missing Gemini API key in Settings".to_owned()
                    },
                )
            }
            _ => {
                let m = model.unwrap_or(cloud_tts::ELEVEN_MODELS[0]).to_owned();
                let ok = !api_keys::parse(&eleven_keys).is_empty();
                (
                    m,
                    None,
                    ok,
                    if ok {
                        "ElevenLabs TTS ready".to_owned()
                    } else {
                        "Missing ElevenLabs API key in Settings".to_owned()
                    },
                )
            }
        };

        if dry_run {
            return Ok(Reply::DryRun(DryRunInfo {
                dry_run: true,
                operation: "speech.tts".to_owned(),
                provider: provider_name,
                model: chosen_model,
                estimated_characters: char_count,
                estimated_seconds: (char_count as f64 / 14.0).max(0.5),
                ready,
                note,
            }));
        }
        if !ready {
            return Err(ApiError::new(
                if provider_name == "local" {
                    ErrorCode::NotFound
                } else {
                    ErrorCode::Refused
                },
                note,
            ));
        }

        let dirs = self.dirs.clone();
        let speech_engine = Arc::clone(&self.speech);
        let live = self.live_window.clone();
        let pending_edits = Arc::clone(&self.pending_edits);
        let project_path = crate::key_of(path);
        let voice_a = voice_id.to_owned();
        let voice_b = voice_b_id.unwrap_or(voice_id).to_owned();
        let style_str = style.unwrap_or_default().to_owned();
        let speed_val = speed.unwrap_or(1.0);
        let temp_val = temperature.unwrap_or(1.0);

        let started = self.spawn_job("speech.tts", path, "", move |job_id, cancel, events| {
            let events_p = Arc::clone(&events);
            let job_p = job_id.clone();
            let path_p = project_path.clone();

            let out_res: Result<PathBuf, String> = if provider_name == "local" {
                let req = SpeakRequest {
                    model_id: chosen_model.clone(),
                    voice: local_voice_num.unwrap_or(0),
                    reference: None,
                    text: text.unwrap_or_default(),
                    speed: speed_val,
                    pauses,
                    steps: None,
                    project: project_path.clone(),
                };
                speech_engine
                    .speak(&dirs, &req, move |stage| {
                        let (label, frac) = match stage {
                            JobStage::Reading => ("preparing", 0.05),
                            JobStage::Loading => ("loading voice", 0.2),
                            JobStage::Working { done, .. } => {
                                ("synthesizing", 0.25 + 0.7 * f64::from(done))
                            }
                        };
                        (events_p)(Event::JobProgress {
                            job: job_p.clone(),
                            path: path_p.clone(),
                            stage: label.to_owned(),
                            fraction: frac,
                        });
                    })
                    .map(|res| PathBuf::from(res.path))
            } else if provider_name == "gemini" {
                (events_p)(Event::JobProgress {
                    job: job_p,
                    path: path_p,
                    stage: "synthesizing with Gemini".to_owned(),
                    fraction: 0.25,
                });
                if let Some(turns_list) = turns {
                    let va = gemini_studio::builtin(&voice_a);
                    let vb = gemini_studio::builtin(&voice_b);
                    let mapped: Vec<gemini_studio::Turn> = turns_list
                        .into_iter()
                        .map(|t| gemini_studio::Turn {
                            speaker: t.speaker.min(1),
                            text: t.text,
                            style: t.style,
                        })
                        .collect();
                    gemini_studio::dialogue(
                        &gemini_keys,
                        &chosen_model,
                        [&va, &vb],
                        &mapped,
                        temp_val,
                        &project_path,
                        &cancel,
                    )
                } else {
                    let opts = cloud_tts::VoiceOptions {
                        delivery: style_str,
                        temperature: temp_val,
                        speed: speed_val,
                        ..Default::default()
                    };
                    cloud_tts::synthesize(
                        1,
                        &gemini_keys,
                        &chosen_model,
                        &voice_a,
                        text.as_deref().unwrap_or_default(),
                        &project_path,
                        &cancel,
                        &opts,
                    )
                }
            } else {
                (events_p)(Event::JobProgress {
                    job: job_p,
                    path: path_p,
                    stage: "synthesizing with ElevenLabs".to_owned(),
                    fraction: 0.25,
                });
                let opts = cloud_tts::VoiceOptions {
                    speed: speed_val,
                    ..Default::default()
                };
                cloud_tts::synthesize(
                    0,
                    &eleven_keys,
                    &chosen_model,
                    &voice_a,
                    text.as_deref().unwrap_or_default(),
                    &project_path,
                    &cancel,
                    &opts,
                )
            };

            match out_res {
                Ok(out_path) => {
                    let out_str = out_path.to_string_lossy().into_owned();
                    let duration = media::probe(&out_str)
                        .ok()
                        .and_then(|s| s.duration)
                        .unwrap_or(0.0);
                    let (media_ids, clip_ids, import_err, _) = finish_audio_output(
                        &live,
                        &pending_edits,
                        &job_id,
                        &project_path,
                        &out_path,
                        place_on_timeline,
                        start,
                        track_id,
                        json!({}),
                    );
                    (events)(Event::JobDone {
                        job: job_id,
                        path: project_path,
                        output: out_str.clone(),
                        result: Some(json!({
                            "path": out_str,
                            "duration": duration,
                            "provider": provider_name,
                            "model": chosen_model,
                            "mediaIds": media_ids,
                            "clipIds": clip_ids,
                            "importError": import_err,
                        })),
                    });
                }
                Err(e) => {
                    let clean = keys.redact(&e);
                    (events)(Event::JobFailed {
                        job: job_id,
                        path: project_path,
                        error: if cancel.load(Ordering::Relaxed) {
                            ApiError::new(ErrorCode::Cancelled, "TTS job cancelled")
                        } else {
                            ApiError::failed(clean)
                        },
                    });
                }
            }
        })?;
        Ok(Reply::Started(started))
    }

    /// [`Request::SpeechDub`].
    #[allow(clippy::too_many_arguments)]
    pub fn speech_dub(
        &mut self,
        path: &str,
        cues: Option<Vec<DubCueSpec>>,
        source_track_id: Option<&str>,
        target_track_id: Option<String>,
        provider: Option<&str>,
        model: Option<&str>,
        voice_a: &str,
        voice_b: Option<&str>,
        temperature: Option<f32>,
        place_on_timeline: bool,
        dry_run: bool,
    ) -> Result<Reply, ApiError> {
        self.ensure_open(path)?;
        self.writable(path)?;
        let provider_name = provider.unwrap_or("gemini").trim().to_ascii_lowercase();
        if !matches!(provider_name.as_str(), "gemini" | "local" | "elevenlabs") {
            return Err(ApiError::invalid(format!(
                "unknown provider {provider_name:?}; expected \"gemini\", \"local\", or \"elevenlabs\""
            )));
        }

        let cue_specs: Vec<DubCueSpec> = match cues {
            Some(list) if !list.is_empty() => list,
            Some(_) => return Err(ApiError::invalid("`cues` must contain at least one cue")),
            None => {
                let live_snapshot = match self.live_handler_for(path) {
                    Some(handler) => Some(handler.snapshot_project(path)?),
                    None => None,
                };
                let session = match live_snapshot.as_ref() {
                    Some(snapshot) => snapshot,
                    None => self.session(path)?,
                };
                let mut rows: Vec<DubCueSpec> = session
                    .project()
                    .active()
                    .clips
                    .iter()
                    .filter(|c| c.kind == ClipKind::Text)
                    .filter(|c| source_track_id.is_none_or(|tid| c.track_id == tid))
                    .filter_map(|c| {
                        let text = c.text.as_ref()?.content.trim().to_owned();
                        if text.is_empty() {
                            return None;
                        }
                        Some(DubCueSpec {
                            text,
                            start: c.start,
                            end: c.start + c.duration,
                            speaker: 0,
                            style: String::new(),
                        })
                    })
                    .collect();
                rows.sort_by(|a, b| a.start.total_cmp(&b.start));
                if rows.is_empty() {
                    return Err(ApiError::new(
                        ErrorCode::NotFound,
                        "No text clips found on the timeline to dub",
                    ));
                }
                rows
            }
        };

        let chars: usize = cue_specs.iter().map(|c| c.text.chars().count()).sum();
        let keys = ServiceKeys::load(&self.dirs);
        let gemini_keys = keys.gemini_api_key.expose().to_owned();
        let eleven_keys = keys.eleven_api_key.expose().to_owned();

        let (chosen_model, local_voice_a, local_voice_b, ready, note) = match provider_name.as_str()
        {
            "gemini" => {
                let m = model.unwrap_or(cloud_tts::GEMINI_MODELS[1]).to_owned();
                let ok = !api_keys::parse(&gemini_keys).is_empty();
                (
                    m,
                    0,
                    0,
                    ok,
                    if ok {
                        "Gemini timed dubbing ready".to_owned()
                    } else {
                        "Missing Gemini API key in Settings".to_owned()
                    },
                )
            }
            "elevenlabs" => {
                let m = model.unwrap_or(cloud_tts::ELEVEN_MODELS[0]).to_owned();
                let ok = !api_keys::parse(&eleven_keys).is_empty();
                (
                    m,
                    0,
                    0,
                    ok,
                    if ok {
                        "ElevenLabs dubbing ready".to_owned()
                    } else {
                        "Missing ElevenLabs API key in Settings".to_owned()
                    },
                )
            }
            _ => {
                let status = Speech::status(&self.dirs).map_err(ApiError::failed)?;
                let va = find_local_voice(&status.voices, voice_a)?;
                let vb = find_local_voice(&status.voices, voice_b.unwrap_or(voice_a))?;
                let m = default_local_model_for(&status, va.family, model)?;
                let installed = status
                    .models
                    .iter()
                    .any(|entry| entry.id == m && entry.downloaded);
                (
                    m,
                    va.id,
                    vb.id,
                    installed,
                    if installed {
                        "Offline dubbing ready".to_owned()
                    } else {
                        "Local TTS model is not downloaded in Settings > Speech".to_owned()
                    },
                )
            }
        };

        if dry_run {
            return Ok(Reply::DryRun(DryRunInfo {
                dry_run: true,
                operation: "speech.dub".to_owned(),
                provider: provider_name,
                model: chosen_model,
                estimated_characters: chars,
                estimated_seconds: cue_specs.last().map(|c| c.end).unwrap_or(0.0),
                ready,
                note: format!("{} cue(s), {} characters", cue_specs.len(), chars),
            }));
        }
        if !ready {
            return Err(ApiError::new(
                if provider_name == "local" {
                    ErrorCode::NotFound
                } else {
                    ErrorCode::Refused
                },
                note,
            ));
        }

        let dirs = self.dirs.clone();
        let speech_engine = Arc::clone(&self.speech);
        let live = self.live_window.clone();
        let pending_edits = Arc::clone(&self.pending_edits);
        let project_path = crate::key_of(path);
        let va_str = voice_a.to_owned();
        let vb_str = voice_b.unwrap_or(voice_a).to_owned();
        let temp_val = temperature.unwrap_or(1.0);

        let started = self.spawn_job("speech.dub", path, "", move |job_id, cancel, events| {
            let min_start = cue_specs
                .iter()
                .map(|c| c.start)
                .fold(f64::INFINITY, f64::min)
                .max(0.0);
            let res: Result<(PathBuf, f64), String> = if provider_name == "gemini" {
                (events)(Event::JobProgress {
                    job: job_id.clone(),
                    path: project_path.clone(),
                    stage: "synthesizing timed dialogue".to_owned(),
                    fraction: 0.25,
                });
                let va = gemini_studio::builtin(&va_str);
                let vb = gemini_studio::builtin(&vb_str);
                let turns: Vec<_> = cue_specs
                    .iter()
                    .map(|c| gemini_studio::Turn {
                        speaker: c.speaker.min(1),
                        text: c.text.clone(),
                        style: c.style.clone(),
                    })
                    .collect();
                let spans: Vec<(f64, f64)> = cue_specs.iter().map(|c| (c.start, c.end)).collect();
                let timed = gemini_studio::timed_dialogue(
                    &gemini_keys,
                    &chosen_model,
                    [&va, &vb],
                    &turns,
                    &spans,
                    temp_val,
                    &project_path,
                    &cancel,
                    &[],
                );
                match (timed.preview, timed.notice) {
                    (Some(wav), None) => Ok((wav, 0.0)),
                    (_, Some(err)) => Err(err),
                    (None, None) => Err("Timed dubbing produced no output".to_owned()),
                }
            } else {
                let mut rendered_cues = Vec::with_capacity(cue_specs.len());
                let total = cue_specs.len().max(1);
                for (idx, spec) in cue_specs.iter().enumerate() {
                    if let Err(e) = cloud_tts::check_cancel(&cancel) {
                        (events)(Event::JobFailed {
                            job: job_id,
                            path: project_path,
                            error: ApiError::new(ErrorCode::Cancelled, e),
                        });
                        return;
                    }
                    (events)(Event::JobProgress {
                        job: job_id.clone(),
                        path: project_path.clone(),
                        stage: format!("synthesizing cue {}/{total}", idx + 1),
                        fraction: idx as f64 / total as f64,
                    });
                    let wav_res = if provider_name == "local" {
                        let vid = if spec.speaker == 1 {
                            local_voice_b
                        } else {
                            local_voice_a
                        };
                        let req = SpeakRequest {
                            model_id: chosen_model.clone(),
                            voice: vid,
                            reference: None,
                            text: spec.text.clone(),
                            speed: 1.0,
                            pauses: None,
                            steps: None,
                            project: project_path.clone(),
                        };
                        speech_engine
                            .speak(&dirs, &req, |_| {})
                            .map(|r| PathBuf::from(r.path))
                    } else {
                        let vid = if spec.speaker == 1 { &vb_str } else { &va_str };
                        let opts = cloud_tts::VoiceOptions::default();
                        cloud_tts::synthesize(
                            0,
                            &eleven_keys,
                            &chosen_model,
                            vid,
                            &spec.text,
                            &project_path,
                            &cancel,
                            &opts,
                        )
                    };
                    let wav = match wav_res {
                        Ok(w) => w,
                        Err(e) => {
                            let clean = keys.redact(&e);
                            (events)(Event::JobFailed {
                                job: job_id,
                                path: project_path,
                                error: ApiError::failed(clean),
                            });
                            return;
                        }
                    };
                    let audio_end = media::probe(&wav.to_string_lossy())
                        .ok()
                        .and_then(|s| s.duration)
                        .unwrap_or((spec.end - spec.start).max(0.1));
                    rendered_cues.push(Cue {
                        text: spec.text.clone(),
                        start: spec.start,
                        end: spec.end,
                        audio: Some(wav),
                        audio_start: 0.0,
                        audio_end,
                    });
                }
                dubbing::render(&rendered_cues, &project_path, &cancel).map(|p| (p, min_start))
            };

            match res {
                Ok((wav_path, place_at)) => {
                    let out_str = wav_path.to_string_lossy().into_owned();
                    let duration = media::probe(&out_str)
                        .ok()
                        .and_then(|s| s.duration)
                        .unwrap_or(0.0);
                    let (media_ids, clip_ids, import_err, _) = finish_audio_output(
                        &live,
                        &pending_edits,
                        &job_id,
                        &project_path,
                        &wav_path,
                        place_on_timeline,
                        place_at,
                        target_track_id,
                        json!({}),
                    );
                    (events)(Event::JobDone {
                        job: job_id,
                        path: project_path,
                        output: out_str.clone(),
                        result: Some(json!({
                            "path": out_str,
                            "duration": duration,
                            "placedAt": place_at,
                            "mediaIds": media_ids,
                            "clipIds": clip_ids,
                            "importError": import_err,
                        })),
                    });
                }
                Err(e) => {
                    let clean = keys.redact(&e);
                    (events)(Event::JobFailed {
                        job: job_id,
                        path: project_path,
                        error: if cancel.load(Ordering::Relaxed) {
                            ApiError::new(ErrorCode::Cancelled, "Dubbing was cancelled")
                        } else {
                            ApiError::failed(clean)
                        },
                    });
                }
            }
        })?;
        Ok(Reply::Started(started))
    }

    /// [`Request::SpeechVoiceChange`].
    #[allow(clippy::too_many_arguments)]
    pub fn speech_voice_change(
        &mut self,
        path: &str,
        clip_id: Option<&str>,
        source_path: Option<&str>,
        method: Option<&str>,
        preset: Option<i32>,
        pitch_semitones: Option<f32>,
        speed: Option<f32>,
        denoise: Option<bool>,
        model: Option<&str>,
        voice_id: Option<&str>,
        place_on_timeline: bool,
        start: Option<f64>,
        track_id: Option<String>,
        dry_run: bool,
    ) -> Result<Reply, ApiError> {
        self.ensure_open(path)?;
        self.writable(path)?;
        let src = resolve_source_clip(self, path, clip_id, source_path)?;
        let mode = method.unwrap_or("dsp").trim().to_ascii_lowercase();
        if !matches!(mode.as_str(), "dsp" | "elevenlabs" | "gemini") {
            return Err(ApiError::invalid(format!(
                "unknown voice-change method {mode:?}; expected \"dsp\", \"elevenlabs\", or \"gemini\""
            )));
        }

        let keys = ServiceKeys::load(&self.dirs);
        let gemini_keys = keys.gemini_api_key.expose().to_owned();
        let eleven_keys = keys.eleven_api_key.expose().to_owned();

        let (chosen_model, ready, note) = match mode.as_str() {
            "dsp" => (
                "dsp-avfilter".to_owned(),
                true,
                "Offline DSP ready".to_owned(),
            ),
            "elevenlabs" => {
                let v = voice_id.unwrap_or("").trim();
                if v.is_empty() && !dry_run {
                    return Err(ApiError::invalid(
                        "`voiceId` is required for ElevenLabs S2S",
                    ));
                }
                let ok = !api_keys::parse(&eleven_keys).is_empty();
                (
                    model.unwrap_or(cloud_tts::ELEVEN_STS_MODELS[0]).to_owned(),
                    ok,
                    if ok {
                        "ElevenLabs S2S ready".to_owned()
                    } else {
                        "Missing ElevenLabs API key in Settings".to_owned()
                    },
                )
            }
            _ => {
                let v = voice_id.unwrap_or("Kore").trim();
                let _ = v;
                let ok = !api_keys::parse(&gemini_keys).is_empty();
                (
                    model.unwrap_or(cloud_tts::GEMINI_STS_MODELS[0]).to_owned(),
                    ok,
                    if ok {
                        "Gemini S2S ready".to_owned()
                    } else {
                        "Missing Gemini API key in Settings".to_owned()
                    },
                )
            }
        };

        if dry_run {
            return Ok(Reply::DryRun(DryRunInfo {
                dry_run: true,
                operation: "speech.voiceChange".to_owned(),
                provider: mode,
                model: chosen_model,
                estimated_characters: 0,
                estimated_seconds: src.window,
                ready,
                note,
            }));
        }
        if !ready {
            return Err(ApiError::new(ErrorCode::Refused, note));
        }

        let preset_idx = preset.unwrap_or(0);
        let (default_pitch, default_denoise) = cloud_tts::dsp_preset_defaults(preset_idx);
        let pitch = pitch_semitones.unwrap_or(default_pitch);
        let spd = speed.unwrap_or(1.0);
        let clean_denoise = denoise.unwrap_or(default_denoise);
        let voice_target = voice_id.unwrap_or("Kore").to_owned();
        let place_start = start.unwrap_or(src.clip_start);
        let project_path = crate::key_of(path);
        let live = self.live_window.clone();
        let pending_edits = Arc::clone(&self.pending_edits);
        let window_opt = clip_id.map(|_| src.window);

        let started = self.spawn_job(
            "speech.voiceChange",
            path,
            "",
            move |job_id, cancel, events| {
                (events)(Event::JobProgress {
                    job: job_id.clone(),
                    path: project_path.clone(),
                    stage: format!("transforming voice ({mode})"),
                    fraction: 0.25,
                });
                let out_res = if mode == "dsp" {
                    cloud_tts::transform_voice_dsp(
                        Path::new(&src.path),
                        src.audio_stream.map(|s| s as usize),
                        src.source_start,
                        window_opt,
                        preset_idx,
                        pitch,
                        spd,
                        clean_denoise,
                        &project_path,
                        &cancel,
                    )
                } else {
                    let (provider_id, key_str) = if mode == "gemini" {
                        (1, gemini_keys.as_str())
                    } else {
                        (0, eleven_keys.as_str())
                    };
                    let opts = cloud_tts::VoiceOptions::default();
                    cloud_tts::synthesize_speech_to_speech(
                        provider_id,
                        key_str,
                        &chosen_model,
                        &voice_target,
                        Path::new(&src.path),
                        src.audio_stream.map(|s| s as usize),
                        src.source_start,
                        window_opt,
                        clean_denoise,
                        &project_path,
                        &cancel,
                        &opts,
                    )
                };

                match out_res {
                    Ok(wav_path) => {
                        let out_str = wav_path.to_string_lossy().into_owned();
                        let duration = media::probe(&out_str)
                            .ok()
                            .and_then(|s| s.duration)
                            .unwrap_or(0.0);
                        let (media_ids, clip_ids, import_err, _) = finish_audio_output(
                            &live,
                            &pending_edits,
                            &job_id,
                            &project_path,
                            &wav_path,
                            place_on_timeline,
                            place_start,
                            track_id,
                            json!({}),
                        );
                        (events)(Event::JobDone {
                            job: job_id,
                            path: project_path,
                            output: out_str.clone(),
                            result: Some(json!({
                                "path": out_str,
                                "duration": duration,
                                "method": mode,
                                "mediaIds": media_ids,
                                "clipIds": clip_ids,
                                "importError": import_err,
                            })),
                        });
                    }
                    Err(e) => {
                        let clean = keys.redact(&e);
                        (events)(Event::JobFailed {
                            job: job_id,
                            path: project_path,
                            error: if cancel.load(Ordering::Relaxed) {
                                ApiError::new(ErrorCode::Cancelled, "Voice change was cancelled")
                            } else {
                                ApiError::failed(clean)
                            },
                        });
                    }
                }
            },
        )?;
        Ok(Reply::Started(started))
    }

    pub(crate) fn run_speech_request(&mut self, request: Request) -> Result<Reply, ApiError> {
        match request {
            Request::ModelsSpeechStatus => Ok(Reply::SpeechStatus(self.models_speech_status()?)),
            Request::CaptionsTranscribe {
                path,
                clip_id,
                media_path,
                engine,
                model,
                language,
                two_speakers,
                smart_split,
                glossary,
                auto_apply,
                offset_y,
                dry_run,
            } => self.captions_transcribe(
                &path,
                clip_id.as_deref(),
                media_path.as_deref(),
                engine.as_deref(),
                model.as_deref(),
                language.as_deref(),
                two_speakers,
                smart_split,
                glossary.as_deref(),
                auto_apply.unwrap_or(true),
                offset_y.unwrap_or(0.35),
                dry_run,
            ),
            Request::CaptionsTranslate {
                path,
                track_id,
                lines,
                target_language,
                model,
                glossary,
                natural,
                condense_to_fit,
                apply_to_track,
                dry_run,
            } => {
                let apply = apply_to_track.unwrap_or_else(|| lines.is_none() && path.is_some());
                self.captions_translate(
                    path.as_deref(),
                    track_id.as_deref(),
                    lines,
                    &target_language,
                    model.as_deref(),
                    glossary.as_deref(),
                    natural,
                    condense_to_fit,
                    apply,
                    dry_run,
                )
            }
            Request::SpeechTts {
                path,
                text,
                turns,
                provider,
                model,
                voice_id,
                voice_b_id,
                style,
                speed,
                pauses,
                temperature,
                place_on_timeline,
                start,
                track_id,
                dry_run,
            } => self.speech_tts(
                &path,
                text,
                turns,
                provider.as_deref(),
                model.as_deref(),
                &voice_id,
                voice_b_id.as_deref(),
                style.as_deref(),
                speed,
                pauses,
                temperature,
                place_on_timeline.unwrap_or(true),
                start.unwrap_or(0.0),
                track_id,
                dry_run,
            ),
            Request::SpeechDub {
                path,
                cues,
                source_track_id,
                provider,
                model,
                voice_a,
                voice_b,
                temperature,
                place_on_timeline,
                target_track_id,
                dry_run,
            } => self.speech_dub(
                &path,
                cues,
                source_track_id.as_deref(),
                target_track_id,
                provider.as_deref(),
                model.as_deref(),
                &voice_a,
                voice_b.as_deref(),
                temperature,
                place_on_timeline.unwrap_or(true),
                dry_run,
            ),
            Request::SpeechVoiceChange {
                path,
                clip_id,
                source_path,
                method,
                preset,
                pitch_semitones,
                speed,
                denoise,
                model,
                voice_id,
                place_on_timeline,
                start,
                track_id,
                dry_run,
            } => self.speech_voice_change(
                &path,
                clip_id.as_deref(),
                source_path.as_deref(),
                method.as_deref(),
                preset,
                pitch_semitones,
                speed,
                denoise,
                model.as_deref(),
                voice_id.as_deref(),
                place_on_timeline.unwrap_or(true),
                start,
                track_id,
                dry_run,
            ),
            _ => unreachable!("non-speech request routed to run_speech_request"),
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::message::{CaptionLineSpec, ErrorCode, Reply, Request};
    use crate::{Api, AppDirs};
    use concat_host::secrets::ApiKey;
    use std::sync::{Arc, Mutex};

    fn test_api() -> (Api, tempfile::TempDir) {
        let scratch = tempfile::tempdir().expect("scratch");
        let dirs = AppDirs {
            config: scratch.path().join("config"),
            data: scratch.path().join("data"),
        };
        std::fs::create_dir_all(&dirs.config).expect("config");
        std::fs::create_dir_all(&dirs.data).expect("data");
        let sink = Arc::new(Mutex::new(Vec::new()));
        let api = Api::with_dirs(dirs, Arc::new(move |ev| sink.lock().expect("ev").push(ev)));
        (api, scratch)
    }

    #[test]
    fn speech_status_redacts_keys_and_reports_counts() {
        let (mut api, _scratch) = test_api();
        let gem = ApiKey::new("secret-one;secret-two".to_owned());
        let doc = serde_json::json!({
            "geminiApiKey": serde_json::to_value(&gem).expect("ser"),
            "elevenApiKey": "",
        });
        std::fs::write(api.dirs().config.join("settings.json"), doc.to_string()).expect("write");

        let reply = match api.dispatch(Request::ModelsSpeechStatus) {
            crate::Response::Result(Reply::SpeechStatus(status)) => status,
            other => panic!(
                "unexpected response: {}",
                serde_json::to_string(&other).unwrap()
            ),
        };
        assert!(reply.gemini_configured);
        assert_eq!(reply.gemini_key_count, 2);
        assert!(!reply.eleven_configured);
        assert_eq!(reply.eleven_key_count, 0);
        let serialized = serde_json::to_string(&reply).expect("json");
        assert!(!serialized.contains("secret-one"));
        assert!(!serialized.contains("secret-two"));
    }

    #[test]
    fn translate_and_tts_dry_run_validate_without_network() {
        let (mut api, scratch) = test_api();
        let loc = scratch.path().to_string_lossy().into_owned();
        api.create(&loc, "SpeechProj", Default::default())
            .expect("create");
        let proj = scratch
            .path()
            .join("SpeechProj")
            .to_string_lossy()
            .into_owned();

        let translate_dry = match api.dispatch(Request::CaptionsTranslate {
            path: None,
            track_id: None,
            lines: Some(vec![CaptionLineSpec {
                start: 0.0,
                end: 2.0,
                text: "Hello world".to_owned(),
                speaker: 0,
            }]),
            target_language: "Vietnamese".to_owned(),
            model: None,
            glossary: None,
            natural: true,
            condense_to_fit: false,
            apply_to_track: None,
            dry_run: true,
        }) {
            crate::Response::Result(Reply::DryRun(info)) => info,
            other => panic!(
                "expected dry run: {}",
                serde_json::to_string(&other).unwrap()
            ),
        };
        assert!(!translate_dry.ready, "no key configured");
        assert_eq!(translate_dry.estimated_characters, 11);

        let tts_refused = match api.dispatch(Request::SpeechTts {
            path: proj,
            text: Some("Test line".to_owned()),
            turns: None,
            provider: Some("gemini".to_owned()),
            model: None,
            voice_id: "Kore".to_owned(),
            voice_b_id: None,
            style: None,
            speed: None,
            pauses: None,
            temperature: None,
            place_on_timeline: None,
            start: None,
            track_id: None,
            dry_run: false,
        }) {
            crate::Response::Error(err) => err,
            _ => panic!("expected refusal without key"),
        };
        assert_eq!(tts_refused.code, ErrorCode::Refused);
    }
}
