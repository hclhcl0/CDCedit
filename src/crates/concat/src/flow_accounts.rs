// SPDX-License-Identifier: AGPL-3.0-or-later
//! Application-wide Flow profiles. Connection snapshots are immutable while jobs run.
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
};

fn primary_id() -> String {
    "primary".into()
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub(crate) struct Account {
    pub id: String,
    pub name: String,
    pub enabled: bool,
    /// 0 managed, 1 attach to an existing loopback CDP endpoint, 2 custom profile directory.
    pub mode: usize,
    pub user_data_dir: String,
    pub profile_directory: String,
    pub chrome_path: String,
    pub port: u16,
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
    pub fn state_dir(&self, root: &Path) -> PathBuf {
        if self.id == "primary" {
            root.into()
        } else {
            root.join("accounts").join(&self.id)
        }
    }
    pub fn browser_config(&self, root: &Path) -> serde_json::Value {
        serde_json::json!({"id":self.id,"attachOnly":self.mode==1,"userDataDir":if self.mode==2 {PathBuf::from(self.user_data_dir.trim())}else{self.state_dir(root).join("chrome")},"profileDirectory":self.profile_directory.trim(),"chromePath":self.chrome_path.trim(),"cdpPort":self.port})
    }
    pub fn detected_email(&self, root: &Path) -> Option<String> {
        if self.mode == 1 {
            return None;
        }
        let base = if self.mode == 2 && !self.user_data_dir.trim().is_empty() {
            PathBuf::from(self.user_data_dir.trim())
        } else {
            self.state_dir(root).join("chrome")
        };
        let bytes = std::fs::read(base.join("Local State")).ok()?;
        let json: serde_json::Value = serde_json::from_slice(&bytes).ok()?;
        let prof = if self.profile_directory.trim().is_empty() {
            "Default"
        } else {
            self.profile_directory.trim()
        };
        let info = json.get("profile")?.get("info_cache")?.get(prof)?;
        let email = info
            .get("user_name")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .trim();
        let name = info
            .get("name")
            .and_then(|v| v.as_str())
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
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct Config {
    pub accounts: Vec<Account>,
    pub parallel_accounts: usize,
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
pub(crate) fn valid_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 100
        && id
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || c == b'-' || c == b'_')
}
pub(crate) fn valid_project(url: &str) -> bool {
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
    pub fn pick(
        &self,
        jobs: &[(usize, Vec<Account>)],
        active: &std::collections::BTreeMap<usize, String>,
        blocked: &std::collections::HashSet<String>,
        unavailable: &std::collections::HashSet<String>,
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
    pub fn limit(&self) -> usize {
        self.parallel_accounts
            .clamp(1, 4)
            .min(self.total_slots.clamp(1, 4))
    }
    pub fn get(&self, id: &str) -> Result<&Account, String> {
        self.accounts
            .iter()
            .find(|a| a.id == id && a.enabled)
            .ok_or_else(|| {
                "Tài khoản Flow không tồn tại hoặc đang tắt. Kiểm tra Cài đặt → Google Flow.".into()
            })
    }
    pub fn candidates(&self, explicit: &str, pinned: Option<&str>) -> Result<Vec<Account>, String> {
        if let Some(id) = pinned {
            if !explicit.is_empty() && explicit != id {
                return Err("Cảnh có nhân vật/giọng gốc của tài khoản khác. Giữ đúng tài khoản nguồn hoặc bỏ liên kết nhân vật khỏi cảnh.".into());
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
    pub fn validate(&self) -> Result<(), String> {
        if self.accounts.len() > 16
            || !(1..=4).contains(&self.parallel_accounts)
            || !(1..=4).contains(&self.total_slots)
        {
            return Err("Hỗ trợ tối đa 16 tài khoản và 1–4 tác vụ đồng thời.".into());
        }
        let mut ids = std::collections::HashSet::new();
        let mut ports = std::collections::HashSet::new();
        let mut profiles = std::collections::HashSet::new();
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
                return Err("Tên profile phải là Default hoặc Profile 1…; không nhập đường dẫn ở ô tên profile.".into());
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
                    return Err("Chrome không mở CDP theo cách này trên thư mục User Data mặc định. Dùng Chrome đã bật CDP hoặc profile riêng.".into());
                }
                let key = normalized.to_string_lossy().to_lowercase();
                if !profiles.insert(key) {
                    return Err("Hai tài khoản chạy song song cần thư mục user-data-dir riêng. Các Profile 1/2 trong cùng thư mục không mở được hai cổng riêng.".into());
                }
            }
        }
        Ok(())
    }
}
#[derive(Default)]
pub(crate) struct Accounts {
    pub config: Config,
    pub draft: Config,
    pub message: String,
    pub states: HashMap<String, String>,
    pub pending: std::collections::HashSet<String>,
    pub port_edits: HashMap<String, String>,
}
impl Accounts {
    pub fn load(&mut self, root: &Path) {
        self.config = Config::default();
        match std::fs::read(root.join("accounts.json")) {
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
                    self.message = "Đã giữ profile đăng nhập cũ. Concat sẽ tự mở lại Chrome của profile này khi kết nối/tạo ảnh.".into();
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
        std::fs::create_dir_all(root).map_err(|e| e.to_string())?;
        let path = root.join("accounts.json");
        let temp = root.join("accounts.json.tmp");
        std::fs::write(
            &temp,
            serde_json::to_vec_pretty(&self.draft).map_err(|e| e.to_string())?,
        )
        .map_err(|e| e.to_string())?;
        std::fs::rename(temp, path).map_err(|e| e.to_string())?;
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

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn owned_legacy_profiles_can_relaunch_without_overriding_explicit_attach_preferences() {
        let root =
            std::env::temp_dir().join(format!("concat-legacy-profile-{}", std::process::id()));
        std::fs::create_dir_all(root.join("chrome")).unwrap();
        std::fs::create_dir_all(root.join("worker")).unwrap();
        std::fs::write(root.join("worker/chrome-owner.json"), "{}").unwrap();
        let mut accounts = Accounts::default();
        accounts.load(&root);
        assert_eq!(accounts.config.accounts[0].mode, 0);
        accounts.draft.accounts[0].mode = 1;
        accounts.save(&root).unwrap();
        accounts.load(&root);
        assert_eq!(accounts.config.accounts[0].mode, 1);
        std::fs::remove_dir_all(root).unwrap();
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
        let mut active = std::collections::BTreeMap::from([(0, "primary".into())]);
        let blocked = std::collections::HashSet::new();
        let unavailable = std::collections::HashSet::new();
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
                    &std::collections::HashSet::from(["second".into()]),
                    &unavailable,
                    0
                )
                .is_none()
        );
    }
    #[test]
    fn invalid_saved_profiles_never_become_executable_configuration() {
        let root = std::env::temp_dir().join(format!(
            "concat-accounts-config-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let mut accounts = Accounts::default();
        accounts.add();
        accounts.draft.parallel_accounts = 2;
        accounts.save(&root).unwrap();
        let mut loaded = Accounts::default();
        loaded.load(&root);
        assert_eq!(loaded.config.accounts.len(), 2);
        loaded.draft.accounts[1].port = 9337;
        let bytes = serde_json::to_vec(&loaded.draft).unwrap();
        std::fs::write(root.join("accounts.json"), &bytes).unwrap();
        let mut invalid = Accounts::default();
        invalid.load(&root);
        assert!(invalid.config.accounts.is_empty());
        assert_eq!(invalid.draft.accounts.len(), 2);
        assert_eq!(std::fs::read(root.join("accounts.json")).unwrap(), bytes);
        invalid.draft.accounts[1].port = 9338;
        invalid.port_edits.insert("primary".into(), "".into());
        assert!(invalid.save(&root).is_err());
        std::fs::remove_dir_all(root).unwrap();
    }
}
