// SPDX-License-Identifier: AGPL-3.0-or-later
//! The desktop Flow worker: one serialized JSON-lines process, with no service port.
use serde_json::{Value, json};
use std::{
    fs,
    io::{BufRead, BufReader, Write},
    path::{Path, PathBuf},
    process::{Child, ChildStdin, ChildStdout, Command, Stdio},
    sync::{Arc, Mutex, OnceLock},
};

struct Worker {
    child: Child,
    input: ChildStdin,
    output: BufReader<ChildStdout>,
    state_dir: PathBuf,
    connection: String,
}
impl Drop for Worker {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}
type Slot = Arc<Mutex<Option<Worker>>>;
static WORKERS: OnceLock<Mutex<std::collections::HashMap<PathBuf, Slot>>> = OnceLock::new();
static RENDERING: OnceLock<Mutex<std::collections::HashSet<String>>> = OnceLock::new();
static LIMIT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(1);
pub(crate) fn rendering_accounts() -> std::collections::HashSet<String> {
    RENDERING
        .get_or_init(Default::default)
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .clone()
}
pub(crate) fn set_limit(limit: usize) {
    let _held = RENDERING
        .get_or_init(Default::default)
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    LIMIT.store(limit.clamp(1, 4), std::sync::atomic::Ordering::Relaxed);
}
struct RenderReservation(String);
impl RenderReservation {
    fn take(account: &str) -> Result<Self, String> {
        let mut active = RENDERING
            .get_or_init(Default::default)
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        if active.contains(account)
            || active.len() >= LIMIT.load(std::sync::atomic::Ordering::Relaxed)
        {
            return Err(
                "Đã đủ luồng Flow đang chạy hoặc tài khoản đang bận. Chưa gửi yêu cầu mới.".into(),
            );
        }
        active.insert(account.into());
        Ok(Self(account.into()))
    }
}
impl Drop for RenderReservation {
    fn drop(&mut self) {
        RENDERING
            .get_or_init(Default::default)
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .remove(&self.0);
    }
}
fn slot(dir: &Path) -> Result<Slot, String> {
    let mut map = WORKERS
        .get_or_init(Default::default)
        .lock()
        .map_err(|_| "Flow worker registry bị lỗi")?;
    Ok(map
        .entry(dir.to_owned())
        .or_insert_with(|| Arc::new(Mutex::new(None)))
        .clone())
}
pub(crate) fn in_use(dir: &Path) -> bool {
    slot(dir).map_or(true, |slot| slot.try_lock().is_err())
}

fn location() -> Result<PathBuf, String> {
    let mut candidates = Vec::new();
    if let Some(root) = std::env::var_os("CONCAT_FLOW_WORKER_DIR") {
        candidates.push(PathBuf::from(root));
    }
    if let Ok(exe) = std::env::current_exe() {
        if let Some(parent) = exe.parent() {
            candidates.push(parent.join("google-flow-worker"));
        }
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
    fs::create_dir_all(state_dir).map_err(|e| e.to_string())?;
    let packaged = root.join(if cfg!(windows) { "node.exe" } else { "node" });
    let node = if packaged.is_file() {
        packaged
    } else {
        PathBuf::from("node")
    };
    let log = fs::OpenOptions::new()
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
/// Sends exactly once. An unknown outcome is returned as an error, never retried.
pub fn call(
    state_dir: &Path,
    tool: &str,
    args: Value,
    allow_credit: bool,
) -> Result<Value, String> {
    call_inner(state_dir, tool, args, allow_credit, None, None)
}
pub(crate) fn call_account(
    root: &Path,
    account: &crate::flow_accounts::Account,
    tool: &str,
    args: Value,
    allow_credit: bool,
) -> Result<Value, String> {
    let config = crate::flow_accounts::Config {
        accounts: vec![account.clone()],
        ..Default::default()
    };
    config.validate()?;
    if !account.enabled {
        return Err("Tài khoản Flow đang tắt.".into());
    }
    let generating = matches!(tool, "flow_generate_video" | "flow_generate_image" | "flow_upscale_video");
    if generating && args["auto_confirm"] == true && !allow_credit {
        return Err("Credit authorization required".into());
    }
    let _reservation = if generating && args["auto_confirm"] == true {
        Some(RenderReservation::take(&account.id)?)
    } else {
        None
    };
    let dir = account.state_dir(root);
    let mut result = call_inner(
        &dir,
        tool,
        args,
        allow_credit,
        Some(account.browser_config(root)),
        None,
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
fn call_inner(
    state_dir: &Path,
    tool: &str,
    args: Value,
    allow_credit: bool,
    connection: Option<Value>,
    worker_root: Option<&Path>,
) -> Result<Value, String> {
    let selected = slot(state_dir)?;
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
        let request = json!({"id":request_id,"tool":tool,"args":args,"allowCredit":allow_credit});
        writeln!(w.input, "{request}")
            .and_then(|_| w.input.flush())
            .map_err(|e| e.to_string())?;
        let mut line = String::new();
        if w.output.read_line(&mut line).map_err(|e| e.to_string())? == 0 {
            return Err("Worker đã ngắt. Kiểm tra kết quả trong Flow trước khi tạo lại.".into());
        }
        let reply: Value = serde_json::from_str(&line)
            .map_err(|e| format!("Worker trả dữ liệu không hợp lệ: {e}"))?;
        if reply["id"].as_str() != Some(request_id.as_str()) {
            return Err("Worker trả sai mã tác vụ; không tự gửi lại.".into());
        }
        let body = reply["result"].clone();
        if !body.is_object() {
            return Err("Worker trả kết quả không đúng cấu trúc. Không tự gửi lại tác vụ.".into());
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

/// Trust only generated files inside the managed output tree, never arbitrary worker paths.
pub fn result_files(state_dir: &Path, value: &Value) -> Vec<PathBuf> {
    let Ok(root) = state_dir.canonicalize() else {
        return Vec::new();
    };
    value["files"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|v| v.as_str())
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
fn managed_output(root: &Path, path: &Path) -> bool {
    let Ok(relative) = path.strip_prefix(root) else {
        return false;
    };
    let parts = relative
        .iter()
        .filter_map(|p| p.to_str())
        .collect::<Vec<_>>();
    (parts.first() == Some(&"output") && parts.len() > 1)
        || (parts.len() > 3
            && parts[0] == "accounts"
            && crate::flow_accounts::valid_id(parts[1])
            && parts[2] == "output")
}
fn strict_result_files(dir: &Path, value: &Value) -> Vec<PathBuf> {
    let Ok(output) = dir.join("output").canonicalize() else {
        return Vec::new();
    };
    result_files(dir, value)
        .into_iter()
        .filter(|path| {
            let canon = Path::new(path).canonicalize().ok();
            canon.as_ref().is_some_and(|c| c.starts_with(&output))
        })
        .map(clean_path)
        .collect()
}

/// Generated reference images must belong to this account's output directory.
pub(crate) fn generated_images(dir: &Path, value: &Value) -> Vec<PathBuf> {
    use std::io::Read;
    let Ok(output) = dir.join("output").canonicalize() else {
        return Vec::new();
    };
    value["files"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .filter_map(|raw| Path::new(raw).canonicalize().ok())
        .filter(|path| {
            if !path.starts_with(&output) {
                return false;
            }
            let Ok(metadata) = fs::metadata(path) else {
                return false;
            };
            if !metadata.is_file() || metadata.len() == 0 || metadata.len() > 32 * 1024 * 1024 {
                return false;
            }
            let mut header = [0u8; 12];
            let Ok(mut file) = fs::File::open(path) else {
                return false;
            };
            let Ok(n) = file.read(&mut header) else {
                return false;
            };
            let ext = path
                .extension()
                .and_then(|s| s.to_str())
                .unwrap_or("")
                .to_ascii_lowercase();
            match ext.as_str() {
                "png" => n >= 8 && header[..8] == *b"\x89PNG\r\n\x1a\n",
                "jpg" | "jpeg" => n >= 3 && header[..3] == [0xff, 0xd8, 0xff],
                "webp" => n >= 12 && header[..4] == *b"RIFF" && header[8..12] == *b"WEBP",
                _ => false,
            }
        })
        .map(clean_path)
        .collect()
}

/// Accept only a bounded local image inside the worker's managed asset tree.
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
    let ext = path
        .extension()
        .and_then(|s| s.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    let metadata = std::fs::metadata(&path).map_err(|e| e.to_string())?;
    if !path.starts_with(root)
        || !metadata.is_file()
        || metadata.len() == 0
        || metadata.len() > 32 * 1024 * 1024
        || !["png", "jpg", "jpeg", "webp"].contains(&ext.as_str())
    {
        return Err("Ảnh Flow nằm ngoài thư viện quản lý hoặc sai định dạng/dung lượng.".into());
    }
    Ok(Some(clean_path(path)))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn generated_images_are_bounded_and_scoped_to_the_selected_account() {
        let root =
            std::env::temp_dir().join(format!("concat-flow-image-paths-{}", std::process::id()));
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
            vec![image.canonicalize().unwrap()]
        );
        assert!(result_files(&root, &value).is_empty());
        let account = crate::flow_accounts::Account::default();
        let denied = call_account(
            &root,
            &account,
            "flow_generate_image",
            json!({"auto_confirm":true}),
            false,
        )
        .unwrap_err();
        assert!(denied.contains("authorization"));
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn render_capacity_and_account_reservations_release_on_drop() {
        set_limit(1);
        let first = RenderReservation::take("capacity-test-one").unwrap();
        assert!(RenderReservation::take("capacity-test-two").is_err());
        assert!(RenderReservation::take("capacity-test-one").is_err());
        drop(first);
        assert!(rendering_accounts().is_empty());
        let next = RenderReservation::take("capacity-test-two").unwrap();
        drop(next);
    }
    #[test]
    fn independent_workers_progress_in_parallel_without_a_browser() {
        let root = std::env::temp_dir().join(format!(
            "concat-account-workers-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let worker = root.join("mock");
        fs::create_dir_all(worker.join("dist")).unwrap();
        fs::write(worker.join("dist/cli.js"), "").unwrap();
        fs::write(worker.join("bridge.mjs"),r#"import fs from 'node:fs';import readline from 'node:readline';import path from 'node:path';for await(const line of readline.createInterface({input:process.stdin})){const r=JSON.parse(line);fs.writeFileSync(path.join(r.args.barrier,r.args.name),'started');const deadline=Date.now()+3000;while(!['one','two'].every(name=>fs.existsSync(path.join(r.args.barrier,name)))&&Date.now()<deadline)await new Promise(resolve=>setTimeout(resolve,20));const ready=['one','two'].every(name=>fs.existsSync(path.join(r.args.barrier,name)));process.stdout.write(JSON.stringify({id:r.id,result:{ok:ready,message:'workers were serialized',name:r.args.name}})+'\n');}"#).unwrap();
        let barrier = root.join("barrier");
        fs::create_dir_all(&barrier).unwrap();
        let handles = ["one", "two"]
            .into_iter()
            .map(|name| {
                let state = root.join(name);
                let worker = worker.clone();
                let barrier = barrier.clone();
                std::thread::spawn(move || {
                    call_inner(
                        &state,
                        "health",
                        json!({"name":name,"barrier":barrier}),
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
        for name in ["one", "two"] {
            *slot(&root.join(name)).unwrap().lock().unwrap() = None;
        }
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn only_existing_video_files_in_the_managed_tree_are_importable() {
        let dir = std::env::temp_dir().join(format!(
            "concat-flow-paths-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(dir.join("output/job")).unwrap();
        let video = dir.join("output/job/clip.mp4");
        let outside = dir.join("outside.mp4");
        let text = dir.join("output/job/config.json");
        for p in [&video, &outside, &text] {
            fs::write(p, b"fixture").unwrap();
        }
        let files = result_files(
            &dir,
            &json!({"files":[video,outside,text,dir.join("output/missing.mp4")]}),
        );
        assert_eq!(files, vec![video.canonicalize().unwrap()]);
        fs::remove_dir_all(dir).unwrap();
    }
}
