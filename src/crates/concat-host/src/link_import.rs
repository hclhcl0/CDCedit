//! Desktop link import. No shell interpolation, remote writes, or credentials in project data.
use crate::dirs::AppDirs;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    fs,
    io::{BufRead, BufReader, Read, Write},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    time::{Duration, Instant},
};

/// A source ready for user selection.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Item {
    /// Display title.
    pub title: String,
    /// Original source address.
    pub url: String,
    /// Drive file ID for authenticated copy, empty for public video extraction.
    pub drive_id: String,
    /// Anonymous Drive download, without OAuth or video transcoding.
    pub public_drive: bool,
    /// Duration in seconds when known.
    pub duration: f64,
    /// Estimated source size in bytes, zero when unknown.
    pub size: u64,
    /// Optional local cached thumbnail path.
    pub thumbnail: String,
    /// Available source video heights.
    pub heights: Vec<u32>,
}
/// Validated supported source.
#[derive(Clone, Debug, PartialEq)]
pub enum Source {
    /// A supported video platform link.
    Video(String),
    /// A Drive file or folder.
    Drive {
        /// Original source address including optional resource key.
        url: String,
        /// Validated Drive ID.
        id: String,
        /// Whether the source denotes a folder.
        folder: bool,
    },
}
/// Validate URL scheme, credentials, hostname and Drive IDs before invoking helpers.
pub fn source(input: &str) -> Result<Source, String> {
    let u = url::Url::parse(input.trim()).map_err(|_| "Invalid link")?;
    if !matches!(u.scheme(), "https" | "http")
        || !u.username().is_empty()
        || u.password().is_some()
        || u.port().is_some()
    {
        return Err("Only normal HTTP/HTTPS links are supported.".into());
    }
    let host = u.host_str().unwrap_or("").to_ascii_lowercase();
    let domain = |d: &str| host == d || host.ends_with(&format!(".{d}"));
    if domain("drive.google.com") {
        let parts: Vec<_> = u.path_segments().into_iter().flatten().collect();
        let folder = parts.contains(&"folders");
        let id = if folder {
            parts
                .iter()
                .position(|p| *p == "folders")
                .and_then(|i| parts.get(i + 1))
                .map(|s| s.to_string())
        } else {
            parts
                .iter()
                .position(|p| *p == "d")
                .and_then(|i| parts.get(i + 1))
                .map(|s| s.to_string())
                .or_else(|| {
                    u.query_pairs()
                        .find(|(k, _)| k == "id")
                        .map(|(_, v)| v.into_owned())
                })
        }
        .ok_or("Google Drive link is missing a file/folder ID")?;
        if id.starts_with('-')
            || id.len() < 5
            || id.len() > 200
            || !id
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-'))
        {
            return Err("Invalid Drive ID".into());
        }
        return Ok(Source::Drive {
            url: u.to_string(),
            id,
            folder,
        });
    }
    if [
        "youtube.com",
        "youtu.be",
        "facebook.com",
        "fb.watch",
        "fb.com",
        "tiktok.com",
    ]
    .iter()
    .any(|d| domain(d))
    {
        Ok(Source::Video(u.to_string()))
    } else {
        Err("Supported links: YouTube, Facebook, TikTok and Google Drive.".into())
    }
}
/// Tool locations, separate from project files.
#[derive(Clone)]
pub struct Tools {
    /// Installed helper executables.
    pub bin: PathBuf,
    /// Account configuration outside project directories.
    pub config: PathBuf,
}
impl Tools {
    /// Resolve per-user tool and account storage.
    pub fn new(dirs: &AppDirs) -> Self {
        Self {
            bin: dirs.data.join("link-tools"),
            config: dirs.config.join("link-import").join("drive.conf"),
        }
    }
    /// Executable path for the active platform.
    pub fn exe(&self, name: &str) -> PathBuf {
        self.bin.join(if cfg!(windows) {
            format!("{name}.exe")
        } else {
            name.into()
        })
    }
    /// All required tools are installed.
    pub fn ready(&self) -> bool {
        ["yt-dlp", "rclone", "ffmpeg", "ffprobe", "deno"]
            .iter()
            .all(|n| self.exe(n).is_file())
    }
    /// A completed local Drive authorization is present.
    pub fn connected(&self) -> bool {
        fs::read_to_string(&self.config).is_ok_and(|s| s.contains("refresh_token"))
    }
    fn command(&self, name: &str) -> Command {
        let mut c = Command::new(self.exe(name));
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            c.creation_flags(0x08000000);
        }
        #[cfg(unix)]
        {
            use std::os::unix::process::CommandExt;
            c.process_group(0);
        }
        let mut paths = vec![self.bin.clone()];
        if let Ok(exe) = std::env::current_exe() {
            if let Some(dir) = exe.parent() {
                paths.push(dir.to_path_buf());
            }
        }
        if let Some(old) = std::env::var_os("PATH") {
            paths.extend(std::env::split_paths(&old));
        }
        if let Ok(path) = std::env::join_paths(paths) {
            c.env("PATH", path);
        }
        c.stdin(Stdio::null());
        c
    }
    fn drive(&self) -> Command {
        let mut c = self.command("rclone");
        c.arg("--config").arg(&self.config);
        c
    }
}
/// Invoke a bounded worker process and reap it, including children on cancellation.
fn run(
    mut command: Command,
    cancel: &AtomicBool,
    limit: Duration,
    progress: &mut dyn FnMut(f32, String),
) -> Result<Vec<u8>, String> {
    if cancel.load(Ordering::Relaxed) {
        return Err("Cancelled".into());
    }
    let mut child = command
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|_| "Could not start download tools. Install or update them first.")?;
    let (tx, rx) = mpsc::sync_channel(32);
    for (is_out, pipe) in [
        (
            true,
            Box::new(child.stdout.take().unwrap()) as Box<dyn Read + Send>,
        ),
        (
            false,
            Box::new(child.stderr.take().unwrap()) as Box<dyn Read + Send>,
        ),
    ] {
        let tx = tx.clone();
        std::thread::spawn(move || {
            let mut r = BufReader::new(pipe);
            loop {
                let mut line = Vec::new();
                match r.by_ref().take(16_777_217).read_until(b'\n', &mut line) {
                    Ok(0) | Err(_) => break,
                    _ => {
                        if line.len() > 16_777_216 {
                            break;
                        }
                        if tx.send((is_out, line)).is_err() {
                            break;
                        }
                    }
                }
            }
        });
    }
    drop(tx);
    let start = Instant::now();
    let mut output = Vec::new();
    loop {
        if cancel.load(Ordering::Relaxed) || start.elapsed() > limit {
            #[cfg(windows)]
            {
                let mut kill = Command::new("taskkill");
                use std::os::windows::process::CommandExt;
                let _ = kill
                    .creation_flags(0x08000000)
                    .args(["/PID", &child.id().to_string(), "/T", "/F"])
                    .stdout(Stdio::null())
                    .stderr(Stdio::null())
                    .status();
            }
            #[cfg(unix)]
            {
                unsafe {
                    libc::kill(-(child.id() as i32), libc::SIGKILL);
                }
            }
            let _ = child.kill();
            let _ = child.wait();
            return Err(if cancel.load(Ordering::Relaxed) {
                "Cancelled"
            } else {
                "Download timed out. Retry the item."
            }
            .into());
        }
        match rx.recv_timeout(Duration::from_millis(100)) {
            Ok((out, line)) => {
                let text = String::from_utf8_lossy(&line);
                if let Some(rest) = text.trim().strip_prefix("CDC_PROGRESS:") {
                    let mut parts = rest.split('|');
                    let percent = parts
                        .next()
                        .unwrap_or("")
                        .trim()
                        .trim_end_matches('%')
                        .parse::<f32>()
                        .unwrap_or(0.0);
                    let speed = parts
                        .next()
                        .unwrap_or("")
                        .chars()
                        .filter(|c| c.is_ascii_alphanumeric() || matches!(c, '/' | '.' | ' '))
                        .take(40)
                        .collect::<String>();
                    progress((percent / 100.0).clamp(0.0, 1.0), speed);
                } else if !out {
                    if let Ok(v) = serde_json::from_slice::<Value>(&line) {
                        if let Some(stats) = v.get("stats") {
                            let done = stats["bytes"].as_f64().unwrap_or(0.0);
                            let total = stats["totalBytes"].as_f64().unwrap_or(0.0);
                            progress(
                                if total > 0.0 {
                                    (done / total) as f32
                                } else {
                                    0.0
                                },
                                format!(
                                    "{:.1} MiB/s",
                                    stats["speed"].as_f64().unwrap_or(0.0) / 1048576.0
                                ),
                            );
                        }
                    }
                } else {
                    if output.len() + line.len() > 16 * 1024 * 1024 {
                        let _ = child.kill();
                        let _ = child.wait();
                        return Err("Source metadata is too large. Use fewer links.".into());
                    }
                    output.extend(line);
                }
            }
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                if child
                    .try_wait()
                    .map_err(|_| "Could not poll download process")?
                    .is_some()
                {
                    break;
                }
                std::thread::sleep(Duration::from_millis(100));
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {}
        }
    }
    let status = child
        .wait()
        .map_err(|_| "Could not finish download process")?;
    if !status.success() {
        return Err("Source request failed. Check link access, sign in when required, or update download tools.".into());
    }
    Ok(output)
}
fn video_command(tools: &Tools, cookies: Option<&Path>) -> Command {
    let mut c = tools.command("yt-dlp");
    c.args([
        "--ignore-config",
        "--no-playlist",
        "--no-warnings",
        "--no-color",
        "--socket-timeout",
        "25",
        "--retries",
        "2",
    ]);
    c.arg("--ffmpeg-location")
        .arg(&tools.bin)
        .arg("--js-runtimes")
        .arg(format!("deno:{}", tools.exe("deno").display()));
    if let Some(p) = cookies {
        c.arg("--cookies").arg(p);
    }
    c
}
/// Explicit browser OAuth flow, read-only scope, account state outside projects.
pub fn connect(tools: &Tools, cancel: &AtomicBool) -> Result<(), String> {
    let dir = tools.config.parent().ok_or("Missing settings directory")?;
    fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    let mut c = tools.drive();
    c.args([
        "config",
        "create",
        "cdcedit",
        "drive",
        "scope",
        "drive.readonly",
        "config_is_local",
        "true",
    ]);
    run(c, cancel, Duration::from_secs(300), &mut |_, _| {})?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&tools.config, fs::Permissions::from_mode(0o600))
            .map_err(|e| e.to_string())?;
    }
    if !tools.connected() {
        return Err("Google sign-in did not complete.".into());
    }
    Ok(())
}
fn media_name(name: &str) -> bool {
    Path::new(name)
        .extension()
        .and_then(|s| s.to_str())
        .is_some_and(|e| {
            matches!(
                e.to_ascii_lowercase().as_str(),
                "mp4"
                    | "mov"
                    | "mkv"
                    | "webm"
                    | "avi"
                    | "m4v"
                    | "mp3"
                    | "wav"
                    | "aac"
                    | "m4a"
                    | "flac"
                    | "ogg"
                    | "opus"
                    | "png"
                    | "jpg"
                    | "jpeg"
                    | "webp"
                    | "gif"
                    | "bmp"
                    | "tif"
                    | "tiff"
            )
        })
}
fn resource_key(command: &mut Command, address: &str) {
    if let Ok(url) = url::Url::parse(address) {
        if let Some((_, key)) = url.query_pairs().find(|(k, _)| k == "resourcekey") {
            command.arg("--drive-resource-key").arg(key.as_ref());
        }
    }
}
fn public_drive(address: &str, probe: bool) -> Result<ureq::Response, String> {
    let Source::Drive { id, .. } = source(address)? else {
        return Err("Expected Drive link".into());
    };
    let mut request = ureq::AgentBuilder::new()
        .timeout_connect(Duration::from_secs(15))
        .timeout_read(Duration::from_secs(15))
        .build()
        .get("https://drive.usercontent.google.com/download")
        .query("id", &id)
        .query("export", "download")
        .query("confirm", "t");
    if probe {
        request = request.set("Range", "bytes=0-0");
    }
    if let Ok(url) = url::Url::parse(address) {
        if let Some((_, key)) = url.query_pairs().find(|(k, _)| k == "resourcekey") {
            request = request.query("resourcekey", &key);
        }
    }
    let response = request.call().map_err(
        |_| "Drive file is unavailable publicly. Connect Google Drive and enable account access.",
    )?;
    let mime = response.header("Content-Type").unwrap_or("");
    if mime.starts_with("text/") || mime.contains("html") {
        return Err("Drive requires sign-in, confirmation or download permission. Connect Google Drive and enable account access.".into());
    }
    Ok(response)
}
fn drive_filename(disposition: &str) -> String {
    let mut name = String::new();
    for part in disposition.split(';') {
        let Some((key, value)) = part.trim().split_once('=') else {
            continue;
        };
        if key.eq_ignore_ascii_case("filename") && name.is_empty() {
            name = value.trim_matches('"').into();
        }
        if key.eq_ignore_ascii_case("filename*") {
            let encoded = value
                .trim_matches('"')
                .split_once("''")
                .map(|(_, s)| s)
                .unwrap_or(value);
            let bytes = encoded.as_bytes();
            let mut decoded = Vec::new();
            let mut i = 0;
            while i < bytes.len() {
                if bytes[i] == b'%' && i + 2 < bytes.len() {
                    if let (Some(a), Some(b)) = (
                        (bytes[i + 1] as char).to_digit(16),
                        (bytes[i + 2] as char).to_digit(16),
                    ) {
                        decoded.push((a * 16 + b) as u8);
                        i += 3;
                        continue;
                    }
                }
                decoded.push(bytes[i]);
                i += 1;
            }
            name = String::from_utf8_lossy(&decoded).into_owned();
            break;
        }
    }
    let safe = name
        .chars()
        .filter(|c| {
            !c.is_control() && !matches!(c, '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*')
        })
        .take(180)
        .collect::<String>();
    format!("Drive-{}", safe.trim_matches([' ', '.']))
}
fn download_public(
    item: &Item,
    dir: &Path,
    cancel: &AtomicBool,
    progress: &mut dyn FnMut(f32, String),
) -> Result<PathBuf, String> {
    let response = public_drive(&item.url, false)?;
    let title = drive_filename(response.header("Content-Disposition").unwrap_or(""));
    if !media_name(&title) {
        return Err(
            "Drive did not return a supported media filename. Try authenticated Drive access."
                .into(),
        );
    }
    let total = response
        .header("Content-Length")
        .and_then(|s| s.parse::<u64>().ok())
        .unwrap_or(0);
    let target = dir.join(&title);
    let partial = dir.join(format!("{title}.part"));
    let mut output = fs::File::create(&partial).map_err(|e| e.to_string())?;
    let mut reader = response.into_reader();
    let mut bytes = 0u64;
    let start = Instant::now();
    let mut tick = Instant::now();
    let mut buffer = [0u8; 65536];
    loop {
        if cancel.load(Ordering::Relaxed) {
            return Err("Cancelled".into());
        }
        let n = reader
            .read(&mut buffer)
            .map_err(|_| "Drive download interrupted. Retry the file.")?;
        if n == 0 {
            break;
        }
        output.write_all(&buffer[..n]).map_err(|e| e.to_string())?;
        bytes += n as u64;
        if tick.elapsed() > Duration::from_millis(250) {
            progress(
                if total > 0 {
                    bytes as f32 / total as f32
                } else {
                    0.0
                },
                format!(
                    "{:.1} MiB/s",
                    bytes as f64 / start.elapsed().as_secs_f64().max(0.001) / 1048576.0
                ),
            );
            tick = Instant::now();
        }
    }
    if bytes == 0 || (total > 0 && bytes != total) {
        return Err("Incomplete Drive download. Retry the file.".into());
    }
    output.sync_all().map_err(|e| e.to_string())?;
    drop(output);
    fs::rename(partial, &target).map_err(|e| e.to_string())?;
    Ok(target)
}
fn thumbnail(tools: &Tools, address: &str) -> String {
    let Ok(u) = url::Url::parse(address) else {
        return String::new();
    };
    let host = u.host_str().unwrap_or("");
    if u.scheme() != "https"
        || ![
            "ytimg.com",
            "googleusercontent.com",
            "fbcdn.net",
            "tiktokcdn.com",
            "tiktokcdn-us.com",
            "tiktokcdn-eu.com",
            "byteimg.com",
        ]
        .iter()
        .any(|d| host == *d || host.ends_with(&format!(".{d}")))
    {
        return String::new();
    }
    use sha2::{Digest, Sha256};
    let dir = tools.bin.join("thumbnails");
    let path = dir.join(format!("{:x}.jpg", Sha256::digest(address.as_bytes())));
    if path.is_file() {
        return path.to_string_lossy().into_owned();
    }
    let result = (|| -> Result<(), String> {
        let response = ureq::AgentBuilder::new()
            .timeout(Duration::from_secs(8))
            .build()
            .get(address)
            .call()
            .map_err(|_| "Thumbnail unavailable")?;
        let mut bytes = Vec::new();
        response
            .into_reader()
            .take(2_000_001)
            .read_to_end(&mut bytes)
            .map_err(|_| "Thumbnail read failed")?;
        if bytes.len() > 2_000_000 {
            return Err("Thumbnail too large".into());
        }
        fs::create_dir_all(dir).map_err(|e| e.to_string())?;
        save(&path, &bytes)
    })();
    if result.is_ok() {
        path.to_string_lossy().into_owned()
    } else {
        String::new()
    }
}
/// Inspect source links without downloading their media.
pub fn inspect(
    tools: &Tools,
    links: &str,
    drive_auth: bool,
    cookies: Option<&Path>,
    cancel: &AtomicBool,
) -> Result<Vec<Item>, String> {
    let links: Vec<_> = links
        .lines()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .collect();
    if links.is_empty() || links.len() > 50 {
        return Err("Paste between 1 and 50 links, one per line.".into());
    }
    let mut result = Vec::new();
    for link in links {
        match source(link)? {
            Source::Drive { url, id, folder } if folder || drive_auth => {
                if !tools.connected() {
                    return Err(
                        "Connect Google Drive before browsing folders or private files.".into(),
                    );
                }
                if folder {
                    let mut c = tools.drive();
                    resource_key(&mut c, &url);
                    c.args([
                        "lsjson",
                        "cdcedit:",
                        "--files-only",
                        "--drive-root-folder-id",
                        &id,
                    ]);
                    let bytes = run(c, cancel, Duration::from_secs(90), &mut |_, _| {})?;
                    let entries: Vec<Value> =
                        serde_json::from_slice(&bytes).map_err(|_| "Invalid Drive file list")?;
                    for e in entries
                        .into_iter()
                        .filter(|e| !e["IsDir"].as_bool().unwrap_or(false))
                    {
                        let name = e["Name"].as_str().unwrap_or("");
                        let fid = e["ID"].as_str().unwrap_or("");
                        if media_name(name) && !fid.is_empty() {
                            result.push(Item {
                                title: name.into(),
                                url: url.clone(),
                                drive_id: fid.into(),
                                public_drive: false,
                                duration: 0.0,
                                size: e["Size"].as_u64().unwrap_or(0),
                                thumbnail: String::new(),
                                heights: vec![],
                            });
                        }
                    }
                } else {
                    result.push(Item {
                        title: format!("Google Drive · {id}"),
                        url,
                        drive_id: id,
                        public_drive: false,
                        duration: 0.0,
                        size: 0,
                        thumbnail: String::new(),
                        heights: vec![],
                    });
                }
            }
            Source::Drive { url, id, .. } => {
                let response = public_drive(&url, true)?;
                let title = drive_filename(response.header("Content-Disposition").unwrap_or(""));
                if !media_name(&title) {
                    return Err("This public Drive link did not return supported media. Connect Google Drive and enable account access.".into());
                }
                let size = response
                    .header("Content-Range")
                    .and_then(|s| s.rsplit('/').next())
                    .or_else(|| response.header("Content-Length"))
                    .and_then(|s| s.parse().ok())
                    .unwrap_or(0);
                result.push(Item {
                    title,
                    url,
                    drive_id: id,
                    public_drive: true,
                    duration: 0.0,
                    size,
                    thumbnail: String::new(),
                    heights: vec![],
                });
            }
            other => {
                let url = match other {
                    Source::Video(u) | Source::Drive { url: u, .. } => u,
                };
                let mut c = video_command(tools, cookies);
                c.args(["--skip-download", "--dump-single-json", "--", &url]);
                let bytes = run(c, cancel, Duration::from_secs(90), &mut |_, _| {})?;
                let v: Value =
                    serde_json::from_slice(&bytes).map_err(|_| "Invalid source metadata")?;
                if v["is_live"].as_bool() == Some(true) {
                    return Err("Live streams are not supported. Use a recorded video.".into());
                }
                let mut heights: Vec<_> = v["formats"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(|f| f["height"].as_u64().map(|h| h as u32))
                    .collect();
                heights.sort_unstable();
                heights.dedup();
                result.push(Item {
                    title: v["title"].as_str().unwrap_or("Media").into(),
                    url,
                    drive_id: String::new(),
                    public_drive: false,
                    duration: v["duration"].as_f64().unwrap_or(0.0),
                    size: v["filesize"]
                        .as_u64()
                        .or_else(|| v["filesize_approx"].as_u64())
                        .unwrap_or(0),
                    thumbnail: thumbnail(tools, v["thumbnail"].as_str().unwrap_or("")),
                    heights,
                });
            }
        }
        if result.len() > 1000 {
            return Err("Too many files. Open a smaller folder.".into());
        }
    }
    if result.is_empty() {
        Err("No supported media files found.".into())
    } else {
        Ok(result)
    }
}
/// Download selected items, preserving successful files when another item fails.
pub fn download(
    tools: &Tools,
    items: &[Item],
    project: &Path,
    height: u32,
    audio: bool,
    cookies: Option<&Path>,
    cancel: &AtomicBool,
    progress: &mut dyn FnMut(f32, String),
) -> Result<(Vec<PathBuf>, Vec<String>, Vec<usize>), String> {
    let root = project.join("media").join("downloads");
    fs::create_dir_all(&root).map_err(|e| e.to_string())?;
    let mut files = Vec::new();
    let mut errors = Vec::new();
    let mut failed = Vec::new();
    for (i, item) in items.iter().enumerate() {
        if cancel.load(Ordering::Relaxed) {
            failed.extend(i..items.len());
            break;
        }
        let serial = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|e| e.to_string())?
            .as_nanos();
        let dir = root.join(format!("{serial}-{i}"));
        fs::create_dir(&dir).map_err(|e| e.to_string())?;
        if item.public_drive {
            match download_public(item, &dir, cancel, &mut |p, s| {
                progress((i as f32 + p) / items.len().max(1) as f32, s)
            }) {
                Ok(file) => files.push(file),
                Err(e) => {
                    errors.push(format!("{}: {e}", item.title));
                    failed.push(i);
                }
            }
            progress((i + 1) as f32 / items.len().max(1) as f32, String::new());
            continue;
        }
        let mut c = if item.drive_id.is_empty() {
            let mut c = video_command(tools, cookies);
            c.args([
                "--newline",
                "--progress",
                "--no-simulate",
                "--progress-template",
                "download:CDC_PROGRESS:%(progress._percent_str)s|%(progress._speed_str)s",
                "--print",
                "after_move:filepath",
            ]);
            c.arg("-o").arg(dir.join("%(title).120B [%(id)s].%(ext)s"));
            if audio {
                c.args([
                    "-f",
                    "bestaudio/best",
                    "--extract-audio",
                    "--audio-format",
                    "m4a",
                ]);
            } else {
                let format = if height == 0 {
                    "bv*+ba/b".to_owned()
                } else {
                    format!("bv*[height<={height}]+ba/b[height<={height}]")
                };
                c.args(["-f", &format, "--merge-output-format", "mkv"]);
            }
            c.args(["--", &item.url]);
            c
        } else {
            let mut c = tools.drive();
            resource_key(&mut c, &item.url);
            c.args(["backend", "copyid", "cdcedit:", &item.drive_id]);
            c.arg(format!("{}/", dir.to_string_lossy().replace('\\', "/")));
            c.args([
                "--use-json-log",
                "--stats",
                "1s",
                "--stats-log-level",
                "NOTICE",
            ]);
            c
        };
        let _ = &mut c;
        let outcome = run(c, cancel, Duration::from_secs(7200), &mut |p, s| {
            progress((i as f32 + p) / items.len().max(1) as f32, s)
        });
        match outcome {
            Ok(_) => {
                let before = files.len();
                for entry in fs::read_dir(&dir).map_err(|e| e.to_string())?.flatten() {
                    let path = entry.path();
                    if path.is_file() && media_name(&entry.file_name().to_string_lossy()) {
                        files.push(path);
                    }
                }
                if files.len() == before {
                    errors.push(format!("{}: No completed media file found.", item.title));
                    failed.push(i);
                }
            }
            Err(e) => {
                errors.push(format!("{}: {e}", item.title));
                failed.push(i);
            }
        }
        progress((i + 1) as f32 / items.len().max(1) as f32, String::new());
    }
    Ok((files, errors, failed))
}
fn agent() -> ureq::Agent {
    ureq::AgentBuilder::new()
        .timeout_connect(Duration::from_secs(15))
        .timeout_read(Duration::from_secs(180))
        .build()
}
fn fetch(url: &str, max: u64) -> Result<Vec<u8>, String> {
    let response = agent()
        .get(url)
        .set("User-Agent", "CDCedit-LinkImport")
        .call()
        .map_err(|_| "Could not download official tools. Check network or retry.")?;
    let mut b = Vec::new();
    response
        .into_reader()
        .take(max + 1)
        .read_to_end(&mut b)
        .map_err(|_| "Tool download interrupted")?;
    if b.len() as u64 > max {
        return Err("Tool download exceeds size limit".into());
    }
    Ok(b)
}
fn checked(url: &str, sha: &str) -> Result<Vec<u8>, String> {
    use sha2::{Digest, Sha256};
    let b = fetch(url, 300 * 1024 * 1024)?;
    if format!("{:x}", Sha256::digest(&b)) != sha {
        return Err("Downloaded tool checksum mismatch. Nothing installed.".into());
    }
    Ok(b)
}
fn save(path: &Path, bytes: &[u8]) -> Result<(), String> {
    let tmp = path.with_extension("download");
    let mut f = fs::File::create(&tmp).map_err(|e| e.to_string())?;
    f.write_all(bytes).map_err(|e| e.to_string())?;
    f.sync_all().map_err(|e| e.to_string())?;
    drop(f);
    fs::rename(tmp, path).map_err(|e| e.to_string())
}
fn unzip(bytes: Vec<u8>, bin: &Path, accept: &dyn Fn(&str) -> bool) -> Result<(), String> {
    let mut z =
        zip::ZipArchive::new(std::io::Cursor::new(bytes)).map_err(|_| "Invalid tool archive")?;
    for i in 0..z.len() {
        let mut file = z.by_index(i).map_err(|_| "Invalid archive entry")?;
        let Some(name) = Path::new(file.name())
            .file_name()
            .and_then(|n| n.to_str())
            .map(str::to_owned)
        else {
            continue;
        };
        if file.is_file() && accept(&name) {
            if file.size() > 300 * 1024 * 1024 {
                return Err("Archive entry too large".into());
            }
            let mut b = Vec::new();
            file.read_to_end(&mut b).map_err(|e| e.to_string())?;
            save(&bin.join(name), &b)?;
        }
    }
    Ok(())
}
/// Install/update verified official Windows x64 tools after an explicit UI action.
pub fn install(
    tools: &Tools,
    cancel: &AtomicBool,
    progress: &mut dyn FnMut(f32, String),
) -> Result<(), String> {
    if !cfg!(all(target_os = "windows", target_arch = "x86_64")) {
        return Err("Automatic link tools installation currently supports Windows x64.".into());
    }
    fs::create_dir_all(&tools.bin).map_err(|e| e.to_string())?;
    for (i, (repo, asset, exe)) in [
        ("yt-dlp/yt-dlp", "yt-dlp.exe", "yt-dlp.exe"),
        (
            "denoland/deno",
            "deno-x86_64-pc-windows-msvc.zip",
            "deno.exe",
        ),
        ("rclone/rclone", "", "rclone.exe"),
    ]
    .iter()
    .enumerate()
    {
        if cancel.load(Ordering::Relaxed) {
            return Err("Cancelled".into());
        }
        progress(i as f32 / 4.0, format!("{exe}"));
        let release: Value = serde_json::from_slice(&fetch(
            &format!("https://api.github.com/repos/{repo}/releases/latest"),
            4 * 1024 * 1024,
        )?)
        .map_err(|_| "Invalid release metadata")?;
        let chosen = release["assets"]
            .as_array()
            .and_then(|a| {
                a.iter().find(|a| {
                    let name = a["name"].as_str().unwrap_or("");
                    if asset.is_empty() {
                        name.ends_with("-windows-amd64.zip")
                    } else {
                        name == *asset
                    }
                })
            })
            .ok_or("Official download asset unavailable")?;
        let url = chosen["browser_download_url"]
            .as_str()
            .ok_or("Missing asset URL")?;
        let sha = chosen["digest"]
            .as_str()
            .and_then(|s| s.strip_prefix("sha256:"))
            .ok_or("Official SHA256 digest unavailable")?;
        if !url.starts_with(&format!("https://github.com/{repo}/releases/download/")) {
            return Err("Unexpected tool download origin".into());
        }
        let b = checked(url, sha)?;
        if asset.ends_with(".exe") {
            save(&tools.exe("yt-dlp"), &b)?;
        } else {
            unzip(b, &tools.bin, &|n| n == *exe)?;
        }
    }
    if cancel.load(Ordering::Relaxed) {
        return Err("Cancelled".into());
    }
    progress(0.75, "FFmpeg".into());
    let line = include_str!("../../../../scripts/ci/deps.lock")
        .lines()
        .find(|l| l.split_whitespace().next() == Some("ffmpeg-win64"))
        .ok_or("Missing FFmpeg download pin")?;
    let fields: Vec<_> = line.split_whitespace().collect();
    if fields.len() != 3 {
        return Err("Invalid FFmpeg pin".into());
    }
    unzip(checked(fields[1], fields[2])?, &tools.bin, &|n| {
        n == "ffmpeg.exe" || n == "ffprobe.exe" || n.ends_with(".dll")
    })?;
    if !tools.ready() {
        return Err("Tool setup incomplete. Retry installation.".into());
    }
    save(
        &tools.bin.join("versions.json"),
        serde_json::to_string_pretty(&json!({"installed":true}))
            .unwrap()
            .as_bytes(),
    )?;
    progress(1.0, String::new());
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    #[ignore = "child process fixture, invoked by runner timeout test"]
    fn worker_process() {
        println!("\nCDC_PROGRESS:25%|1 MiB/s");
        std::thread::sleep(Duration::from_secs(20));
    }
    #[test]
    fn worker_timeout_reaps_process_and_reports_progress() {
        let mut command = Command::new(std::env::current_exe().unwrap());
        command.args([
            "--ignored",
            "--exact",
            "link_import::tests::worker_process",
            "--nocapture",
        ]);
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            command.creation_flags(0x08000000);
        }
        #[cfg(unix)]
        {
            use std::os::unix::process::CommandExt;
            command.process_group(0);
        }
        let start = Instant::now();
        let mut fraction = 0.0;
        let result = run(
            command,
            &AtomicBool::new(false),
            Duration::from_secs(2),
            &mut |p, _| fraction = p,
        );
        assert!(result.is_err());
        assert!(start.elapsed() < Duration::from_secs(8));
        assert_eq!(fraction, 0.25);
    }
    #[test]
    fn pre_cancelled_job_does_not_start_a_process() {
        let result = run(
            Command::new("nonexistent-download-helper"),
            &AtomicBool::new(true),
            Duration::from_secs(1),
            &mut |_, _| {},
        );
        assert_eq!(result.unwrap_err(), "Cancelled");
    }
    #[test]
    fn linked_drive_resource_key_is_not_lost() {
        let mut command = Command::new("rclone");
        resource_key(
            &mut command,
            "https://drive.google.com/drive/folders/abc123?resourcekey=key_123&usp=sharing",
        );
        let args = command
            .get_args()
            .map(|s| s.to_string_lossy().into_owned())
            .collect::<Vec<_>>();
        assert_eq!(args, vec!["--drive-resource-key", "key_123"]);
    }
    #[test]
    fn rejects_command_and_hostname_tricks() {
        for u in [
            "file:///etc/passwd",
            "https://youtube.com.evil.test/v",
            "https://youtube.com@evil.test/a",
            "https://youtube.com:444/a",
            "--exec=calc",
        ] {
            assert!(source(u).is_err(), "{u}");
        }
    }
    #[test]
    fn parses_drive_and_short_links() {
        assert!(matches!(
            source("https://drive.google.com/file/d/abc_123/view").unwrap(),
            Source::Drive { folder: false, .. }
        ));
        assert!(matches!(
            source("https://drive.google.com/drive/folders/abc-123?usp=sharing").unwrap(),
            Source::Drive { folder: true, .. }
        ));
        assert!(matches!(
            source("https://vm.tiktok.com/test").unwrap(),
            Source::Video(_)
        ));
    }
    #[test]
    fn recognizes_media_not_html_or_partial() {
        assert!(media_name("clip.MP4"));
        assert!(media_name("photo.jpg"));
        assert!(!media_name("consent.html"));
        assert!(!media_name("video.mp4.part"));
    }
    #[test]
    fn drive_filename_decodes_unicode_and_cannot_escape_destination() {
        assert_eq!(
            drive_filename("attachment; filename=\"../clip.mp4\""),
            "Drive-clip.mp4"
        );
        assert_eq!(
            drive_filename(
                "attachment; filename=\"fallback.mp4\"; filename*=UTF-8''gi%E1%BB%8Dng.mp4"
            ),
            "Drive-giọng.mp4"
        );
        assert!(!media_name(&drive_filename("inline")));
    }
}
