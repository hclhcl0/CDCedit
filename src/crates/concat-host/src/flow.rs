// SPDX-License-Identifier: AGPL-3.0-or-later

//! Google Flow Studio desktop browser profiles, account locks, receipts, and
//! serialized stdio worker client.
//!
//! Lives in `concat-host` so the desktop UI window (`concat`) and headless/MCP
//! API (`concat-api`) share the exact same [`FlowAccountLock`], Chrome profile
//! validation (`google-flow/accounts.json`), and `receipts/*.json` safety
//! invariants (credit authorization + never auto-retrying `uncertain` jobs).

use std::collections::{BTreeMap, HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, OnceLock};

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

/// Returns the root directory storing Google Flow profiles, receipts, and
/// outputs under CDCAi's application data folder.
pub fn root_dir(dirs: &crate::AppDirs) -> PathBuf {
    dirs.data.join("google-flow")
}

fn primary_id() -> String {
    "primary".into()
}

/// A Google Flow Chrome CDP account profile stored in `google-flow/accounts.json`.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct Account {
    /// Stable profile identifier (`"primary"`, `"account-..."`, etc.).
    pub id: String,
    /// Human-readable display label shown in Settings › Google Flow.
    pub name: String,
    /// Whether this account is eligible for scheduling.
    pub enabled: bool,
    /// Connection mode: `0` managed profile, `1` attach existing loopback CDP,
    /// `2` custom Chrome `user-data-dir`.
    pub mode: usize,
    /// Custom Chrome `User Data` directory when `mode == 2`.
    pub user_data_dir: String,
    /// Chrome profile subdirectory name (`"Default"`, `"Profile 1"`, ...).
    pub profile_directory: String,
    /// Optional explicit path to `chrome.exe` / Chrome binary.
    pub chrome_path: String,
    /// Local CDP debugging port (`1024..=65535`, default `9337`).
    pub port: u16,
    /// Optional default Google Flow project URL (`https://flow.google.com/project/...`).
    pub project_url: String,
}

impl Default for Account {
    fn default() -> Self {
        Self {
            id: primary_id(),
            name: "Flow 1".into(),
            enabled: true,
            mode: 0,
            user_data_dir: String::new(),
            profile_directory: "Default".into(),
            chrome_path: String::new(),
            port: 9337,
            project_url: String::new(),
        }
    }
}

impl Account {
    /// Per-account state directory holding `chrome/`, `worker/`, `receipts/`, and `output/`.
    pub fn state_dir(&self, root: &Path) -> PathBuf {
        if self.id == "primary" {
            root.into()
        } else {
            root.join("accounts").join(&self.id)
        }
    }

    /// Serializes connection settings passed on `bridge.mjs`'s argv.
    pub fn browser_config(&self, root: &Path) -> Value {
        json!({
            "id": self.id,
            "attachOnly": self.mode == 1,
            "userDataDir": if self.mode == 2 {
                PathBuf::from(self.user_data_dir.trim())
            } else {
                self.state_dir(root).join("chrome")
            },
            "profileDirectory": self.profile_directory.trim(),
            "chromePath": self.chrome_path.trim(),
            "cdpPort": self.port,
        })
    }

    /// Reads the Chrome `Local State` metadata to detect the signed-in Google account email/name.
    pub fn detected_email(&self, root: &Path) -> Option<String> {
        if self.mode == 1 {
            return None;
        }
        let base = if self.mode == 2 && !self.user_data_dir.trim().is_empty() {
            PathBuf::from(self.user_data_dir.trim())
        } else {
            self.state_dir(root).join("chrome")
        };
        let bytes = fs::read(base.join("Local State")).ok()?;
        let parsed: Value = serde_json::from_slice(&bytes).ok()?;
        let prof = if self.profile_directory.trim().is_empty() {
            "Default"
        } else {
            self.profile_directory.trim()
        };
        let info = parsed.get("profile")?.get("info_cache")?.get(prof)?;
        let email = info
            .get("user_name")
            .and_then(Value::as_str)
            .unwrap_or("")
            .trim();
        let name = info
            .get("name")
            .and_then(Value::as_str)
            .unwrap_or("")
            .trim();
        if !email.is_empty() {
            Some(email.to_owned())
        } else if !name.is_empty() {
            Some(name.to_owned())
        } else {
            None
        }
    }
}

/// Persisted multi-account configuration (`google-flow/accounts.json`).
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    /// Configured Google Flow profiles (up to 16).
    pub accounts: Vec<Account>,
    /// Maximum concurrent distinct Flow accounts allowed (`1..=4`).
    pub parallel_accounts: usize,
    /// Maximum total concurrent generation slots allowed (`1..=4`).
    pub total_slots: usize,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            accounts: vec![Account::default()],
            parallel_accounts: 1,
            total_slots: 1,
        }
    }
}

/// Validates an account id (`[A-Za-z0-9_-]{1,100}`).
pub fn valid_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 100
        && id
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || c == b'-' || c == b'_')
}

/// Validates a Google Flow project URL containing a 36-character UUID.
pub fn valid_project(url: &str) -> bool {
    let trimmed = url.trim();
    let rest = if let Some(r) = trimmed.strip_prefix("https://flow.google.com/project/") {
        Some(r)
    } else if let Some(r) = trimmed.strip_prefix("https://labs.google/fx/") {
        r.split_once("tools/flow/project/").map(|(_, after)| after)
    } else {
        None
    };
    let Some(id) = rest else {
        return false;
    };
    let id = id.split(['?', '#', '/']).next().unwrap_or("");
    id.len() == 36
        && id.bytes().enumerate().all(|(i, c)| {
            if [8, 13, 18, 23].contains(&i) {
                c == b'-'
            } else {
                c.is_ascii_hexdigit()
            }
        })
}

impl Config {
    /// Picks the next `(job_index, scene_index, account)` eligible for parallel execution.
    pub fn pick(
        &self,
        jobs: &[(usize, Vec<Account>)],
        active: &BTreeMap<usize, String>,
        blocked: &HashSet<String>,
        unavailable: &HashSet<String>,
        cursor: usize,
    ) -> Option<(usize, usize, Account)> {
        if active.len() >= self.limit() {
            return None;
        }
        for (position, (scene, accounts)) in jobs.iter().enumerate() {
            for offset in 0..accounts.len() {
                let account = &accounts[(cursor + offset) % accounts.len()];
                if account.enabled
                    && !active.values().any(|id| id == &account.id)
                    && !blocked.contains(&account.id)
                    && !unavailable.contains(&account.id)
                {
                    return Some((position, *scene, account.clone()));
                }
            }
        }
        None
    }

    /// Effective concurrency ceiling (`1..=4`).
    pub fn limit(&self) -> usize {
        self.parallel_accounts
            .clamp(1, 4)
            .min(self.total_slots.clamp(1, 4))
    }

    /// Finds an enabled account matching `id`.
    pub fn get(&self, id: &str) -> Result<&Account, String> {
        self.accounts
            .iter()
            .find(|a| a.id == id && a.enabled)
            .ok_or_else(|| {
                "Tài khoản Flow không tồn tại hoặc đang tắt. Kiểm tra Cài đặt → Google Flow.".into()
            })
    }

    /// Resolves candidate accounts for a scene considering explicit selection or pinned voice ownership.
    pub fn candidates(&self, explicit: &str, pinned: Option<&str>) -> Result<Vec<Account>, String> {
        if let Some(id) = pinned {
            if !explicit.is_empty() && explicit != id {
                return Err(
                    "Cảnh có nhân vật/giọng gốc của tài khoản khác. Giữ đúng tài khoản nguồn hoặc bỏ liên kết nhân vật khỏi cảnh."
                        .into(),
                );
            }
            return Ok(vec![self.get(id)?.clone()]);
        }
        if !explicit.is_empty() {
            return Ok(vec![self.get(explicit)?.clone()]);
        }
        let accounts = self
            .accounts
            .iter()
            .filter(|a| a.enabled)
            .cloned()
            .collect::<Vec<_>>();
        if accounts.is_empty() {
            Err("Chưa có tài khoản Flow được bật trong Cài đặt.".into())
        } else {
            Ok(accounts)
        }
    }

    /// Validates all configured accounts, CDP ports, Chrome executables, and distinct profile directories.
    pub fn validate(&self) -> Result<(), String> {
        if self.accounts.len() > 16
            || !(1..=4).contains(&self.parallel_accounts)
            || !(1..=4).contains(&self.total_slots)
        {
            return Err("Hỗ trợ tối đa 16 tài khoản và 1–4 tác vụ đồng thời.".into());
        }
        let mut ids = HashSet::new();
        let mut ports = HashSet::new();
        let mut profiles = HashSet::new();
        for a in &self.accounts {
            if !valid_id(&a.id)
                || !ids.insert(&a.id)
                || a.name.trim().is_empty()
                || a.name.len() > 300
                || a.mode > 2
                || a.port < 1024
                || !ports.insert(a.port)
            {
                return Err("Tên/ID tài khoản không hợp lệ hoặc cổng Chrome bị trùng.".into());
            }
            if a.profile_directory.trim().is_empty()
                || a.profile_directory.contains(['/', '\\'])
                || matches!(a.profile_directory.trim(), "." | "..")
            {
                return Err(
                    "Tên profile phải là Default hoặc Profile 1…; không nhập đường dẫn ở ô tên profile."
                        .into(),
                );
            }
            if !a.project_url.trim().is_empty() && !valid_project(&a.project_url) {
                return Err(format!("{}: URL dự án Flow mặc định không hợp lệ.", a.name));
            }
            if !a.chrome_path.trim().is_empty()
                && (!Path::new(a.chrome_path.trim()).is_absolute()
                    || !Path::new(a.chrome_path.trim()).is_file())
            {
                return Err(format!("{}: đường dẫn chrome.exe không hợp lệ.", a.name));
            }
            if a.mode == 2 && a.enabled {
                let path = Path::new(a.user_data_dir.trim());
                if !path.is_absolute() || !path.is_dir() {
                    return Err(format!(
                        "{}: chọn thư mục dữ liệu Chrome có sẵn và dùng đường dẫn tuyệt đối.",
                        a.name
                    ));
                }
                let standard = std::env::var_os("LOCALAPPDATA")
                    .map(PathBuf::from)
                    .map(|p| p.join("Google/Chrome/User Data"));
                let normalized = path.canonicalize().map_err(|e| e.to_string())?;
                if !path.join(a.profile_directory.trim()).is_dir() {
                    return Err(format!(
                        "{}: không tìm thấy thư mục profile đã chọn bên trong user-data-dir.",
                        a.name
                    ));
                }
                if standard
                    .and_then(|p| p.canonicalize().ok())
                    .is_some_and(|p| p == normalized)
                {
                    return Err(
                        "Chrome không mở CDP theo cách này trên thư mục User Data mặc định. Dùng Chrome đã bật CDP hoặc profile riêng."
                            .into(),
                    );
                }
                let key = normalized.to_string_lossy().to_lowercase();
                if !profiles.insert(key) {
                    return Err(
                        "Hai tài khoản chạy song song cần thư mục user-data-dir riêng. Các Profile 1/2 trong cùng thư mục không mở được hai cổng riêng."
                            .into(),
                    );
                }
            }
        }
        Ok(())
    }
}

/// Editable multi-account state used by Settings › Google Flow and read by headless/MCP callers.
#[derive(Default)]
pub struct Accounts {
    /// Active validated configuration used when dispatching jobs.
    pub config: Config,
    /// Draft configuration currently being edited in Settings.
    pub draft: Config,
    /// Status or validation message for the UI.
    pub message: String,
    /// Last known connection status per account id.
    pub states: HashMap<String, String>,
    /// Account ids currently running a connection check.
    pub pending: HashSet<String>,
    /// In-progress CDP port text edits keyed by account id.
    pub port_edits: HashMap<String, String>,
}

impl Accounts {
    /// Loads `accounts.json` from `root` (keeping safe defaults or reporting validation errors).
    pub fn load(&mut self, root: &Path) {
        self.config = Config::default();
        match fs::read(root.join("accounts.json")) {
            Ok(bytes) => match serde_json::from_slice::<Config>(&bytes) {
                Ok(config) => {
                    if let Err(e) = config.validate() {
                        self.message = e;
                        self.draft = config;
                        self.config = Config {
                            accounts: Vec::new(),
                            ..Default::default()
                        };
                        return;
                    }
                    self.config = config;
                }
                _ => {
                    self.message =
                        "Cấu hình tài khoản Flow không hợp lệ; chưa ghi đè file cũ.".into();
                    self.draft = Config::default();
                    self.config = Config {
                        accounts: Vec::new(),
                        ..Default::default()
                    };
                    return;
                }
            },
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                if root.join("chrome").is_dir() {
                    self.config.accounts[0].mode = 0;
                    self.message =
                        "Đã giữ profile đăng nhập cũ. CDCAi sẽ tự mở lại Chrome của profile này khi kết nối/tạo ảnh."
                            .into();
                }
            }
            Err(e) => {
                self.message = e.to_string();
                self.draft = Config::default();
                self.config = Config {
                    accounts: Vec::new(),
                    ..Default::default()
                };
                return;
            }
        }
        self.draft = self.config.clone();
    }

    /// Validates and atomically writes `draft` to `accounts.json`, promoting it to `config`.
    pub fn save(&mut self, root: &Path) -> Result<(), String> {
        if self.port_edits.iter().any(|(id, text)| {
            self.draft.accounts.iter().any(|a| &a.id == id)
                && text
                    .trim()
                    .parse::<u16>()
                    .ok()
                    .is_none_or(|port| port < 1024)
        }) {
            return Err("Cổng CDP phải là số từ 1024 đến 65535.".into());
        }
        self.draft.validate()?;
        fs::create_dir_all(root).map_err(|e| e.to_string())?;
        let path = root.join("accounts.json");
        let temp = root.join("accounts.json.tmp");
        fs::write(
            &temp,
            serde_json::to_vec_pretty(&self.draft).map_err(|e| e.to_string())?,
        )
        .map_err(|e| e.to_string())?;
        fs::rename(temp, path).map_err(|e| e.to_string())?;
        for a in &self.draft.accounts {
            if self.config.accounts.iter().find(|old| old.id == a.id) != Some(a) {
                self.states.remove(&a.id);
            }
        }
        self.config = self.draft.clone();
        self.port_edits.clear();
        self.message = "Đã lưu tài khoản và giới hạn chạy Flow.".into();
        Ok(())
    }

    /// Appends a new draft account with a unique id and free CDP port (`>= 9337`).
    pub fn add(&mut self) {
        if self.draft.accounts.len() >= 16 {
            self.message = "Tối đa 16 tài khoản Flow.".into();
            return;
        }
        let port = (9337..=u16::MAX)
            .find(|port| !self.draft.accounts.iter().any(|a| a.port == *port))
            .unwrap_or(9337);
        let id = format!(
            "account-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos()
        );
        self.draft.accounts.push(Account {
            id,
            name: format!("Flow {}", self.draft.accounts.len() + 1),
            port,
            ..Default::default()
        });
    }
}

/// Receipt recorded in `<state_dir>/receipts/<id>.json` by `bridge.mjs` around
/// credit-spending Google Flow generations.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FlowReceipt {
    /// Request / job identifier inside `receipts/`.
    pub id: String,
    /// Lifecycle phase: `"running"`, `"finished"`, `"failed"`, or `"uncertain"`.
    #[serde(default)]
    pub phase: String,
    /// Worker tool name (`"flow_generate_video"`, `"flow_generate_image"`, ...).
    #[serde(default)]
    pub tool: String,
    /// ISO-8601 timestamp when receipt was last updated.
    #[serde(default)]
    pub updated: String,
    /// Requested model identifier when recorded.
    #[serde(default)]
    pub model: Option<String>,
    /// Error message when `phase` is `"failed"` or `"uncertain"`.
    #[serde(default)]
    pub error: Option<String>,
    /// Raw JSON body returned by the worker when finished or failed cleanly.
    #[serde(default)]
    pub result: Option<Value>,
}

/// Recent downloaded outputs and unresolved (`running` / `uncertain`) receipts for one account state dir.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct FlowRecentSummary {
    /// Verified output media paths still present under `<state_dir>/output`.
    pub files: Vec<PathBuf>,
    /// Receipts in `"running"` or `"uncertain"` state that must be checked before re-submitting.
    pub unresolved: Vec<FlowReceipt>,
}

/// Reads and sorts `receipts/*.json` inside `state_dir` (newest first by `updated`).
pub fn read_receipts(state_dir: &Path) -> Vec<FlowReceipt> {
    let dir = state_dir.join("receipts");
    let Ok(entries) = fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut receipts = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("json") {
            continue;
        }
        let Ok(bytes) = fs::read(&path) else {
            continue;
        };
        if let Ok(parsed) = serde_json::from_slice::<FlowReceipt>(&bytes) {
            receipts.push(parsed);
        }
    }
    receipts.sort_by(|a, b| b.updated.cmp(&a.updated).then_with(|| b.id.cmp(&a.id)));
    receipts
}

/// Returns only unresolved (`"running"` or `"uncertain"`) receipts inside `state_dir`.
pub fn unresolved_receipts(state_dir: &Path) -> Vec<FlowReceipt> {
    read_receipts(state_dir)
        .into_iter()
        .filter(|r| matches!(r.phase.as_str(), "running" | "uncertain"))
        .collect()
}

/// Scans `receipts/*.json` in `state_dir` without launching Chrome/Node, returning
/// verified generated media files on disk alongside any unresolved receipts.
pub fn recent_outputs(state_dir: &Path) -> FlowRecentSummary {
    let receipts = read_receipts(state_dir);
    let mut seen = HashSet::new();
    let mut files = Vec::new();
    let mut unresolved = Vec::new();
    for r in receipts {
        if matches!(r.phase.as_str(), "running" | "uncertain") {
            unresolved.push(r);
            continue;
        }
        let Some(body) = r.result.as_ref() else {
            continue;
        };
        let candidates: Vec<PathBuf> = if r.tool == "flow_generate_image" {
            generated_images(state_dir, body)
        } else {
            let mut combined = strict_result_files(state_dir, body);
            combined.extend(generated_images(state_dir, body));
            combined
        };
        for file in candidates {
            if seen.insert(file.clone()) {
                files.push(file);
            }
        }
    }
    FlowRecentSummary { files, unresolved }
}

/// Refuses launching a new credit-spending generation when an account still has an
/// `"uncertain"` or `"running"` receipt unless `acknowledge_uncertain` is `true`.
pub fn check_ready_for_generation(
    state_dir: &Path,
    acknowledge_uncertain: bool,
) -> Result<(), String> {
    let unresolved = unresolved_receipts(state_dir);
    if unresolved.is_empty() || acknowledge_uncertain {
        return Ok(());
    }
    let summary: Vec<String> = unresolved
        .iter()
        .take(3)
        .map(|r| format!("{} ({}, {})", r.id, r.phase, r.updated))
        .collect();
    Err(format!(
        "This Flow account has {} unresolved/uncertain receipt(s): {}. Never auto-retry after an uncertain timeout/crash — inspect Flow or call `flow_recent_outputs` (or `resume: true` with `jobId`), then pass `acknowledgeUncertain: true` only after verifying credits/output.",
        unresolved.len(),
        summary.join("; ")
    ))
}

#[cfg(not(any(target_os = "android", target_os = "ios")))]
mod desktop {
    use std::io::{BufRead, BufReader, Write};
    use std::path::{Path, PathBuf};
    use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};
    use std::sync::{Arc, Mutex};

    use serde_json::{Value, json};

    #[derive(Debug)]
    pub(super) struct Worker {
        pub(super) child: Child,
        pub(super) input: ChildStdin,
        pub(super) output: BufReader<ChildStdout>,
        pub(super) state_dir: PathBuf,
        pub(super) connection: String,
    }

    impl Drop for Worker {
        fn drop(&mut self) {
            let _ = self.child.kill();
            let _ = self.child.wait();
        }
    }

    pub(super) type Slot = Arc<Mutex<Option<Worker>>>;

    fn location() -> Result<PathBuf, String> {
        let mut candidates = Vec::new();
        if let Some(root) = std::env::var_os("CONCAT_FLOW_WORKER_DIR") {
            candidates.push(PathBuf::from(root));
        }
        if let Ok(exe) = std::env::current_exe()
            && let Some(parent) = exe.parent()
        {
            candidates.push(parent.join("google-flow-worker"));
        }
        candidates
            .push(Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../tools/google-flow-worker"));
        candidates
            .into_iter()
            .find(|p| p.join("bridge.mjs").is_file() && p.join("dist/cli.js").is_file())
            .ok_or_else(|| {
                "Chưa có Google Flow worker. Chạy scripts/package-flow-worker.ps1 trước.".into()
            })
    }

    fn launch(
        state_dir: &Path,
        connection: Option<&Value>,
        worker_root: Option<&Path>,
    ) -> Result<Worker, String> {
        let root = worker_root
            .map(Path::to_owned)
            .map(Ok)
            .unwrap_or_else(location)?;
        std::fs::create_dir_all(state_dir).map_err(|e| e.to_string())?;
        let packaged = root.join(if cfg!(windows) { "node.exe" } else { "node" });
        let node = if packaged.is_file() {
            packaged
        } else {
            PathBuf::from("node")
        };
        let log = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(state_dir.join("bridge.log"))
            .map_err(|e| e.to_string())?;
        let mut cmd = Command::new(node);
        cmd.arg(root.join("bridge.mjs"))
            .arg(state_dir)
            .current_dir(root)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(log);
        if let Some(connection) = connection {
            cmd.arg(connection.to_string());
        }
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            cmd.creation_flags(0x08000000);
        }
        let mut child = cmd.spawn().map_err(|e| {
            format!("Không mở được worker (cần Node.js 20+ hoặc runtime đóng kèm): {e}")
        })?;
        let input = child.stdin.take().ok_or("Không mở được stdin worker")?;
        let output = BufReader::new(child.stdout.take().ok_or("Không mở được stdout worker")?);
        Ok(Worker {
            child,
            input,
            output,
            state_dir: state_dir.into(),
            connection: connection.map(Value::to_string).unwrap_or_default(),
        })
    }

    pub(super) fn call_slot(
        selected: &Slot,
        state_dir: &Path,
        tool: &str,
        args: Value,
        allow_credit: bool,
        connection: Option<Value>,
        worker_root: Option<&Path>,
    ) -> Result<Value, String> {
        let mut held = selected.lock().map_err(|_| "Flow worker lock bị lỗi")?;
        if held.as_ref().is_some_and(|w| {
            w.state_dir != state_dir
                || w.connection
                    != connection
                        .as_ref()
                        .map(Value::to_string)
                        .unwrap_or_default()
        }) {
            *held = None;
        }
        if held.is_none() {
            *held = Some(launch(state_dir, connection.as_ref(), worker_root)?);
        }
        let request_id = format!(
            "{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos()
        );
        let result = (|| {
            let w = held.as_mut().ok_or("Worker chưa sẵn sàng")?;
            let request = json!({
                "id": request_id,
                "tool": tool,
                "args": args,
                "allowCredit": allow_credit,
            });
            writeln!(w.input, "{request}")
                .and_then(|_| w.input.flush())
                .map_err(|e| e.to_string())?;
            let mut line = String::new();
            if w.output.read_line(&mut line).map_err(|e| e.to_string())? == 0 {
                return Err(
                    "Worker đã ngắt. Kiểm tra kết quả trong Flow trước khi tạo lại.".into(),
                );
            }
            let reply: Value = serde_json::from_str(&line)
                .map_err(|e| format!("Worker trả dữ liệu không hợp lệ: {e}"))?;
            if reply["id"].as_str() != Some(request_id.as_str()) {
                return Err("Worker trả sai mã tác vụ; không tự gửi lại.".into());
            }
            let body = reply["result"].clone();
            if !body.is_object() {
                return Err(
                    "Worker trả kết quả không đúng cấu trúc. Không tự gửi lại tác vụ.".into(),
                );
            }
            if body["ok"] == false {
                return Err(body["message"]
                    .as_str()
                    .unwrap_or("Google Flow báo lỗi")
                    .to_owned());
            }
            Ok(body)
        })();
        // Protocol/transport errors are not retried. A semantic Flow error leaves the session intact.
        if held
            .as_mut()
            .is_some_and(|w| w.child.try_wait().ok().flatten().is_some())
        {
            *held = None;
        }
        result
    }
}

/// RAII reservation holding one concurrent render slot for an account inside a [`FlowAccountLock`].
pub struct RenderReservation {
    rendering: Arc<Mutex<HashSet<String>>>,
    account: String,
}

impl Drop for RenderReservation {
    fn drop(&mut self) {
        self.rendering
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .remove(&self.account);
    }
}

#[derive(Debug)]
struct FlowRegistry {
    #[cfg(not(any(target_os = "android", target_os = "ios")))]
    workers: Mutex<HashMap<PathBuf, desktop::Slot>>,
    #[cfg(not(any(target_os = "android", target_os = "ios")))]
    worker_root: Option<PathBuf>,
    rendering: Arc<Mutex<HashSet<String>>>,
    limit: AtomicUsize,
}

impl FlowRegistry {
    fn new(worker_root: Option<PathBuf>) -> Self {
        #[cfg(any(target_os = "android", target_os = "ios"))]
        let _ = worker_root;
        Self {
            #[cfg(not(any(target_os = "android", target_os = "ios")))]
            workers: Mutex::new(HashMap::new()),
            #[cfg(not(any(target_os = "android", target_os = "ios")))]
            worker_root,
            rendering: Arc::new(Mutex::new(HashSet::new())),
            limit: AtomicUsize::new(1),
        }
    }
}

/// Shared coordinator enforcing single-account exclusivity, concurrency limits,
/// and serialized stdio `bridge.mjs` worker access across the desktop UI and [`concat_api`].
#[derive(Clone, Debug)]
pub struct FlowAccountLock {
    inner: Arc<FlowRegistry>,
}

impl Default for FlowAccountLock {
    fn default() -> Self {
        Self::new()
    }
}

impl FlowAccountLock {
    /// Creates an isolated coordinator (ideal for headless API instances or tests).
    pub fn new() -> Self {
        Self {
            inner: Arc::new(FlowRegistry::new(None)),
        }
    }

    /// Creates an isolated coordinator pointing to an explicit worker directory
    /// containing `bridge.mjs` and `dist/cli.js` (useful in tests or custom builds).
    pub fn with_worker_root(worker_root: PathBuf) -> Self {
        Self {
            inner: Arc::new(FlowRegistry::new(Some(worker_root))),
        }
    }

    /// Process-wide coordinator shared with the desktop Flow Studio and Settings panes.
    pub fn shared() -> &'static Self {
        static SHARED: OnceLock<FlowAccountLock> = OnceLock::new();
        SHARED.get_or_init(Self::new)
    }

    /// Snapshot of account ids currently holding an active render slot.
    pub fn rendering_accounts(&self) -> HashSet<String> {
        self.inner
            .rendering
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
    }

    /// Updates the maximum concurrent generation slots (`1..=4`).
    pub fn set_limit(&self, limit: usize) {
        let _held = self
            .inner
            .rendering
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        self.inner.limit.store(limit.clamp(1, 4), Ordering::Relaxed);
    }

    /// Reserves an execution slot for `account` if capacity permits and `account` is idle.
    pub fn reserve(&self, account: &str) -> Result<RenderReservation, String> {
        let mut active = self
            .inner
            .rendering
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        if active.contains(account) || active.len() >= self.inner.limit.load(Ordering::Relaxed) {
            return Err(
                "Đã đủ luồng Flow đang chạy hoặc tài khoản đang bận. Chưa gửi yêu cầu mới.".into(),
            );
        }
        active.insert(account.to_owned());
        Ok(RenderReservation {
            rendering: Arc::clone(&self.inner.rendering),
            account: account.to_owned(),
        })
    }

    /// Returns `true` while a `bridge.mjs` worker call is currently executing for `dir`.
    pub fn in_use(&self, dir: &Path) -> bool {
        #[cfg(not(any(target_os = "android", target_os = "ios")))]
        {
            self.slot(dir).map_or(true, |slot| slot.try_lock().is_err())
        }
        #[cfg(any(target_os = "android", target_os = "ios"))]
        {
            let _ = dir;
            false
        }
    }

    #[cfg(not(any(target_os = "android", target_os = "ios")))]
    fn slot(&self, dir: &Path) -> Result<desktop::Slot, String> {
        let mut map = self
            .inner
            .workers
            .lock()
            .map_err(|_| "Flow worker registry bị lỗi")?;
        Ok(map
            .entry(dir.to_owned())
            .or_insert_with(|| Arc::new(Mutex::new(None)))
            .clone())
    }

    /// Sends one tool request to `bridge.mjs` under `state_dir`. Never auto-retries.
    pub fn call(
        &self,
        state_dir: &Path,
        tool: &str,
        args: Value,
        allow_credit: bool,
    ) -> Result<Value, String> {
        self.call_with_worker_root(state_dir, tool, args, allow_credit, None, None)
    }

    /// Sends one tool request with optional Chrome connection config and explicit worker root.
    pub fn call_with_worker_root(
        &self,
        state_dir: &Path,
        tool: &str,
        args: Value,
        allow_credit: bool,
        connection: Option<Value>,
        worker_root: Option<&Path>,
    ) -> Result<Value, String> {
        #[cfg(not(any(target_os = "android", target_os = "ios")))]
        {
            let selected = self.slot(state_dir)?;
            desktop::call_slot(
                &selected,
                state_dir,
                tool,
                args,
                allow_credit,
                connection,
                worker_root,
            )
        }
        #[cfg(any(target_os = "android", target_os = "ios"))]
        {
            let _ = (state_dir, tool, args, allow_credit, connection, worker_root);
            Err("Google Flow desktop browser worker is not available on mobile platforms".into())
        }
    }

    /// Validates account profile + credit gate, reserves capacity if generating, executes
    /// the tool on `account`'s worker, and filters returned files to `state_dir/output`.
    pub fn call_account(
        &self,
        root: &Path,
        account: &Account,
        tool: &str,
        args: Value,
        allow_credit: bool,
    ) -> Result<Value, String> {
        let config = Config {
            accounts: vec![account.clone()],
            ..Default::default()
        };
        config.validate()?;
        if !account.enabled {
            return Err("Tài khoản Flow đang tắt.".into());
        }
        let generating = matches!(
            tool,
            "flow_generate_video"
                | "flow_generate_image"
                | "flow_upscale_video"
                | "vids_generate_video"
        );
        let spending_credit = generating && args["auto_confirm"] == true && args["resume"] != true;
        if spending_credit && !allow_credit {
            return Err("Credit authorization required".into());
        }
        let _reservation = if generating && (args["auto_confirm"] == true || args["resume"] == true)
        {
            Some(self.reserve(&account.id)?)
        } else {
            None
        };
        let dir = account.state_dir(root);
        #[cfg(not(any(target_os = "android", target_os = "ios")))]
        let worker_override = self.inner.worker_root.as_deref();
        #[cfg(any(target_os = "android", target_os = "ios"))]
        let worker_override: Option<&Path> = None;
        let mut result = self.call_with_worker_root(
            &dir,
            tool,
            args,
            allow_credit,
            Some(account.browser_config(root)),
            worker_override,
        )?;
        if result["files"].is_array() {
            result["files"] = if tool == "flow_generate_image" {
                json!(generated_images(&dir, &result))
            } else {
                json!(strict_result_files(&dir, &result))
            };
        }
        result["_account_id"] = json!(account.id);
        result["_state_dir"] = json!(dir);
        Ok(result)
    }

    /// Shuts down all cached child workers held by this registry.
    pub fn shutdown_all(&self) {
        #[cfg(not(any(target_os = "android", target_os = "ios")))]
        if let Ok(mut map) = self.inner.workers.lock() {
            for slot in map.values() {
                if let Ok(mut guard) = slot.lock() {
                    *guard = None;
                }
            }
            map.clear();
        }
    }
}

/// Strips Windows `\\?\` verbatim path prefixes for display and probing.
pub fn clean_path(path: PathBuf) -> PathBuf {
    #[cfg(windows)]
    {
        let s = path.to_string_lossy();
        if let Some(stripped) = s.strip_prefix(r"\\?\") {
            return PathBuf::from(stripped);
        }
    }
    path
}

fn managed_output(root: &Path, path: &Path) -> bool {
    let Ok(relative) = path.strip_prefix(root) else {
        return false;
    };
    let parts = relative
        .iter()
        .filter_map(|p| p.to_str())
        .collect::<Vec<_>>();
    (parts.first() == Some(&"output") && parts.len() > 1)
        || (parts.len() > 3 && parts[0] == "accounts" && valid_id(parts[1]) && parts[2] == "output")
}

/// Filters `value["files"]` to existing `.mp4`/`.webm` files inside `state_dir`'s managed output tree.
pub fn result_files(state_dir: &Path, value: &Value) -> Vec<PathBuf> {
    let Ok(root) = state_dir.canonicalize() else {
        return Vec::new();
    };
    value["files"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .filter_map(|s| Path::new(s).canonicalize().ok())
        .filter(|p| {
            managed_output(&root, p)
                && p.is_file()
                && p.extension().is_some_and(|ext| {
                    ext.eq_ignore_ascii_case("mp4") || ext.eq_ignore_ascii_case("webm")
                })
        })
        .map(clean_path)
        .collect()
}

/// Filters `value["files"]` to existing `.mp4`/`.webm` files strictly inside `<dir>/output`.
pub fn strict_result_files(dir: &Path, value: &Value) -> Vec<PathBuf> {
    let Ok(output) = dir.join("output").canonicalize() else {
        return Vec::new();
    };
    result_files(dir, value)
        .into_iter()
        .filter(|path| {
            Path::new(path)
                .canonicalize()
                .ok()
                .is_some_and(|c| c.starts_with(&output))
        })
        .map(clean_path)
        .collect()
}

fn valid_image_file(root: &Path, path: &Path, check_magic: bool) -> bool {
    use std::io::Read;
    if !path.starts_with(root) {
        return false;
    }
    let Ok(metadata) = fs::metadata(path) else {
        return false;
    };
    if !metadata.is_file() || metadata.len() == 0 || metadata.len() > 32 * 1024 * 1024 {
        return false;
    }
    let ext = path
        .extension()
        .and_then(|s| s.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    if !matches!(ext.as_str(), "png" | "jpg" | "jpeg" | "webp") {
        return false;
    }
    if !check_magic {
        return true;
    }
    let mut header = [0u8; 12];
    let Ok(mut file) = fs::File::open(path) else {
        return false;
    };
    let Ok(n) = file.read(&mut header) else {
        return false;
    };
    match ext.as_str() {
        "png" => n >= 8 && header[..8] == *b"\x89PNG\r\n\x1a\n",
        "jpg" | "jpeg" => n >= 3 && header[..3] == [0xff, 0xd8, 0xff],
        "webp" => n >= 12 && header[..4] == *b"RIFF" && header[8..12] == *b"WEBP",
        _ => false,
    }
}

/// Filters `value["files"]` to valid generated images inside `<dir>/output`.
pub fn generated_images(dir: &Path, value: &Value) -> Vec<PathBuf> {
    let Ok(output) = dir.join("output").canonicalize() else {
        return Vec::new();
    };
    value["files"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .filter_map(|raw| Path::new(raw).canonicalize().ok())
        .filter(|path| valid_image_file(&output, path, true))
        .map(clean_path)
        .collect()
}

/// Validates a downloaded Flow character/asset image inside `<state_dir>/assets`.
pub fn imported_image(state_dir: &Path, value: &Value) -> Result<Option<PathBuf>, String> {
    let raw = value["image_path"]
        .as_str()
        .ok_or("Worker không trả đường dẫn ảnh hợp lệ")?;
    if raw.is_empty() {
        return Ok(None);
    }
    let assets = state_dir.join("assets");
    let _ = fs::create_dir_all(&assets);
    let root = assets.canonicalize().map_err(|e| e.to_string())?;
    let path = Path::new(raw).canonicalize().map_err(|e| e.to_string())?;
    if !valid_image_file(&root, &path, false) {
        return Err("Ảnh Flow nằm ngoài thư viện quản lý hoặc sai định dạng/dung lượng.".into());
    }
    Ok(Some(clean_path(path)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn owned_legacy_profiles_can_relaunch_without_overriding_explicit_attach_preferences() {
        let scratch = tempfile::tempdir().unwrap();
        let root = scratch.path();
        fs::create_dir_all(root.join("chrome")).unwrap();
        fs::create_dir_all(root.join("worker")).unwrap();
        fs::write(root.join("worker/chrome-owner.json"), "{}").unwrap();
        let mut accounts = Accounts::default();
        accounts.load(root);
        assert_eq!(accounts.config.accounts[0].mode, 0);
        accounts.draft.accounts[0].mode = 1;
        accounts.save(root).unwrap();
        accounts.load(root);
        assert_eq!(accounts.config.accounts[0].mode, 1);
    }

    #[test]
    fn profiles_have_independent_ports_and_managed_directories() {
        let mut s = Accounts::default();
        s.add();
        assert!(s.draft.validate().is_ok());
        assert_ne!(s.draft.accounts[0].port, s.draft.accounts[1].port);
        assert_ne!(
            s.draft.accounts[0].state_dir(Path::new("C:/flow")),
            s.draft.accounts[1].state_dir(Path::new("C:/flow"))
        );
        s.draft.accounts[1].port = 9337;
        assert!(s.draft.validate().is_err());
    }

    #[test]
    fn limits_are_real_single_profile_slots() {
        let mut c = Config::default();
        c.parallel_accounts = 4;
        c.total_slots = 2;
        assert_eq!(c.limit(), 2);
        assert!(c.get("missing").is_err());
        c.accounts[0].enabled = false;
        assert!(c.get("primary").is_err());
    }

    #[test]
    fn routing_keeps_original_voice_account_and_respects_capacity() {
        let a = Account::default();
        let b = Account {
            id: "second".into(),
            port: 9338,
            ..Default::default()
        };
        let config = Config {
            accounts: vec![a.clone(), b.clone()],
            parallel_accounts: 2,
            total_slots: 2,
        };
        assert!(config.candidates("second", Some("primary")).is_err());
        assert_eq!(
            config.candidates("", Some("primary")).unwrap()[0].id,
            "primary"
        );
        let mut active = BTreeMap::from([(0, "primary".into())]);
        let blocked = HashSet::new();
        let unavailable = HashSet::new();
        let jobs = vec![(1, vec![a.clone()]), (2, vec![a, b])];
        let (position, scene, account) = config
            .pick(&jobs, &active, &blocked, &unavailable, 0)
            .unwrap();
        assert_eq!((position, scene, account.id.as_str()), (1, 2, "second"));
        active.insert(scene, account.id);
        assert!(
            config
                .pick(&jobs, &active, &blocked, &unavailable, 0)
                .is_none()
        );
        active.remove(&2);
        assert!(
            config
                .pick(
                    &jobs,
                    &active,
                    &HashSet::from(["second".into()]),
                    &unavailable,
                    0
                )
                .is_none()
        );
    }

    #[test]
    fn invalid_saved_profiles_never_become_executable_configuration() {
        let scratch = tempfile::tempdir().unwrap();
        let root = scratch.path();
        let mut accounts = Accounts::default();
        accounts.add();
        accounts.draft.parallel_accounts = 2;
        accounts.save(root).unwrap();
        let mut loaded = Accounts::default();
        loaded.load(root);
        assert_eq!(loaded.config.accounts.len(), 2);
        loaded.draft.accounts[1].port = 9337;
        let bytes = serde_json::to_vec(&loaded.draft).unwrap();
        fs::write(root.join("accounts.json"), &bytes).unwrap();
        let mut invalid = Accounts::default();
        invalid.load(root);
        assert!(invalid.config.accounts.is_empty());
        assert_eq!(invalid.draft.accounts.len(), 2);
        assert_eq!(fs::read(root.join("accounts.json")).unwrap(), bytes);
        invalid.draft.accounts[1].port = 9338;
        invalid.port_edits.insert("primary".into(), "".into());
        assert!(invalid.save(root).is_err());
    }

    #[test]
    fn generated_images_and_credit_authorization_are_enforced() {
        let scratch = tempfile::tempdir().unwrap();
        let root = scratch.path();
        let first = root.join("first");
        let second = root.join("second");
        fs::create_dir_all(first.join("output/job")).unwrap();
        fs::create_dir_all(second.join("output/job")).unwrap();
        let image = first.join("output/job/reference.png");
        let other = second.join("output/job/reference.png");
        let bad = first.join("output/job/not-an-image.png");
        fs::write(&image, b"\x89PNG\r\n\x1a\nfixture").unwrap();
        fs::write(&other, b"\x89PNG\r\n\x1a\nfixture").unwrap();
        fs::write(&bad, b"html response").unwrap();
        let value = json!({"files":[image, other, bad]});
        assert_eq!(
            generated_images(&first, &value),
            vec![clean_path(image.canonicalize().unwrap())]
        );
        assert!(result_files(root, &value).is_empty());

        let lock = FlowAccountLock::new();
        let account = Account::default();
        let denied = lock
            .call_account(
                root,
                &account,
                "flow_generate_image",
                json!({"auto_confirm":true}),
                false,
            )
            .unwrap_err();
        assert!(denied.contains("authorization"));
    }

    #[test]
    fn render_capacity_and_account_reservations_release_on_drop() {
        let lock = FlowAccountLock::new();
        lock.set_limit(1);
        let first = lock.reserve("capacity-test-one").unwrap();
        assert!(lock.reserve("capacity-test-two").is_err());
        assert!(lock.reserve("capacity-test-one").is_err());
        drop(first);
        assert!(lock.rendering_accounts().is_empty());
        let next = lock.reserve("capacity-test-two").unwrap();
        drop(next);
    }

    #[test]
    fn uncertain_receipt_blocks_resubmit_and_appears_in_recent_outputs() {
        let scratch = tempfile::tempdir().unwrap();
        let state_dir = scratch.path();
        fs::create_dir_all(state_dir.join("receipts")).unwrap();
        fs::create_dir_all(state_dir.join("output/j1")).unwrap();
        let video = state_dir.join("output/j1/clip.mp4");
        fs::write(&video, b"mp4fixture").unwrap();

        fs::write(
            state_dir.join("receipts/done-job.json"),
            json!({
                "id": "done-job",
                "phase": "finished",
                "tool": "flow_generate_video",
                "updated": "2026-11-16T01:00:00Z",
                "result": { "files": [video] }
            })
            .to_string(),
        )
        .unwrap();
        fs::write(
            state_dir.join("receipts/lost-job.json"),
            json!({
                "id": "lost-job",
                "phase": "uncertain",
                "tool": "flow_generate_video",
                "updated": "2026-11-16T02:00:00Z",
                "error": "bridge closed mid-flight"
            })
            .to_string(),
        )
        .unwrap();

        let recent = recent_outputs(state_dir);
        assert_eq!(
            recent.files,
            vec![clean_path(video.canonicalize().unwrap())]
        );
        assert_eq!(recent.unresolved.len(), 1);
        assert_eq!(recent.unresolved[0].id, "lost-job");
        assert_eq!(recent.unresolved[0].phase, "uncertain");

        let blocked = check_ready_for_generation(state_dir, false).unwrap_err();
        assert!(blocked.contains("lost-job"));
        assert!(check_ready_for_generation(state_dir, true).is_ok());
    }

    #[cfg(not(any(target_os = "android", target_os = "ios")))]
    #[test]
    fn independent_workers_progress_in_parallel_without_a_browser() {
        let scratch = tempfile::tempdir().unwrap();
        let root = scratch.path();
        let worker = root.join("mock");
        fs::create_dir_all(worker.join("dist")).unwrap();
        fs::write(worker.join("dist/cli.js"), "").unwrap();
        fs::write(
            worker.join("bridge.mjs"),
            r#"import fs from 'node:fs';import readline from 'node:readline';import path from 'node:path';for await(const line of readline.createInterface({input:process.stdin})){const r=JSON.parse(line);fs.writeFileSync(path.join(r.args.barrier,r.args.name),'started');const deadline=Date.now()+3000;while(!['one','two'].every(name=>fs.existsSync(path.join(r.args.barrier,name)))&&Date.now()<deadline)await new Promise(resolve=>setTimeout(resolve,20));const ready=['one','two'].every(name=>fs.existsSync(path.join(r.args.barrier,name)));process.stdout.write(JSON.stringify({id:r.id,result:{ok:ready,message:'workers were serialized',name:r.args.name}})+'\n');}"#,
        )
        .unwrap();
        let barrier = root.join("barrier");
        fs::create_dir_all(&barrier).unwrap();
        let lock = FlowAccountLock::new();
        let handles = ["one", "two"]
            .into_iter()
            .map(|name| {
                let state = root.join(name);
                let worker = worker.clone();
                let barrier = barrier.clone();
                let lock = lock.clone();
                std::thread::spawn(move || {
                    lock.call_with_worker_root(
                        &state,
                        "health",
                        json!({"name": name, "barrier": barrier}),
                        false,
                        None,
                        Some(&worker),
                    )
                })
            })
            .collect::<Vec<_>>();
        for handle in handles {
            assert!(handle.join().unwrap().is_ok());
        }
        lock.shutdown_all();
    }

    #[test]
    fn only_existing_video_files_in_the_managed_tree_are_importable() {
        let scratch = tempfile::tempdir().unwrap();
        let dir = scratch.path();
        fs::create_dir_all(dir.join("output/job")).unwrap();
        let video = dir.join("output/job/clip.mp4");
        let outside = dir.join("outside.mp4");
        let text = dir.join("output/job/config.json");
        for p in [&video, &outside, &text] {
            fs::write(p, b"fixture").unwrap();
        }
        let files = result_files(
            dir,
            &json!({"files":[video, outside, text, dir.join("output/missing.mp4")]}),
        );
        assert_eq!(files, vec![clean_path(video.canonicalize().unwrap())]);
    }
}
