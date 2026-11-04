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
        #[cfg(target_os = "android")]
        {
            if let Ok(native_libs) = std::env::var("CONCAT_NATIVE_LIBS") {
                let lib_dir = PathBuf::from(&native_libs);
                if name == "yt-dlp" || name == "python" {
                    let py = lib_dir.join("libpython.so");
                    if py.is_file() {
                        return py;
                    }
                }
                if name == "ytdlp_script" || name == "ytdlp" {
                    let ytdlp = lib_dir.join("libytdlp.so");
                    if ytdlp.is_file() {
                        return ytdlp;
                    }
                }
                if name == "ffmpeg" {
                    let ff = lib_dir.join("libffmpeg.so");
                    if ff.is_file() {
                        return ff;
                    }
                }
                if name == "deno" || name == "quickjs" || name == "qjs" {
                    let qjs = lib_dir.join("libqjs.so");
                    if qjs.is_file() {
                        return qjs;
                    }
                }
            }
            let path = self.bin.join(name);
            if path.is_file() {
                return path;
            }
            let mut cur = self.bin.parent();
            while let Some(p) = cur {
                let pkg_bin = p.join("no_backup").join("youtubedl-android").join("packages");
                if pkg_bin.is_dir() {
                    if name == "yt-dlp" || name == "python" {
                        let py = pkg_bin.join("python").join("bin").join("python3");
                        if py.is_file() {
                            return py;
                        }
                    }
                    if name == "ffmpeg" {
                        let ff = pkg_bin.join("ffmpeg").join("bin").join("ffmpeg");
                        if ff.is_file() {
                            return ff;
                        }
                    }
                    break;
                }
                cur = p.parent();
            }
            path
        }
        #[cfg(not(target_os = "android"))]
        {
            let local = self.bin.join(if cfg!(windows) {
                format!("{name}.exe")
            } else {
                name.into()
            });
            if local.is_file() {
                return local;
            }
            if let Some(paths) = std::env::var_os("PATH") {
                let filename = if cfg!(windows) {
                    format!("{name}.exe")
                } else {
                    name.to_string()
                };
                for dir in std::env::split_paths(&paths) {
                    let candidate = dir.join(&filename);
                    if candidate.is_file() {
                        return candidate;
                    }
                }
            }
            local
        }
    }
    /// All required tools are installed.
    pub fn ready(&self) -> bool {
        #[cfg(target_os = "android")]
        {
            // Embedded via youtubedl-android or native binary on Android
            true
        }
        #[cfg(not(target_os = "android"))]
        {
            self.exe("yt-dlp").is_file() && self.exe("ffmpeg").is_file()
        }
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
    let mut err_lines: Vec<String> = Vec::new();
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
                    let trimmed = text.trim().to_string();
                    if !trimmed.is_empty() {
                        if err_lines.len() >= 10 {
                            err_lines.remove(0);
                        }
                        err_lines.push(trimmed);
                    }
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
        let last_err = err_lines
            .iter()
            .rev()
            .find(|l| l.contains("ERROR:") || l.contains("Error") || l.contains("Failed") || l.contains("Unable"))
            .cloned()
            .unwrap_or_else(|| err_lines.last().cloned().unwrap_or_default());
        log::error!("Download process failed with status {status:?}: {last_err}");
        if !last_err.is_empty() {
            let clean = last_err.trim_start_matches("ERROR:").trim();
            if clean.contains("Cannot parse data") && clean.contains("facebook") {
                return Err("Facebook chặn tải không đăng nhập (Reels/video riêng tư). Vui lòng thử video công khai khác hoặc chọn file cookies.txt ở mục Phiên đăng nhập.".into());
            }
            if clean.contains("Sign in to confirm you’re not a bot") || clean.contains("Sign in to confirm you're not a bot") {
                return Err("YouTube yêu cầu xác minh tài khoản. Vui lòng thêm cookies.txt hoặc thử lại sau.".into());
            }
            return Err(format!("Source request failed: {clean}"));
        }
        return Err("Source request failed. Check link access, sign in when required, or update download tools.".into());
    }
    Ok(output)
}
#[cfg(target_os = "android")]
fn ensure_android_python(tools: &Tools, native_libs: &Path) {
    let py_home = tools.bin.parent().unwrap_or(&tools.bin).join("python");
    let marker = py_home.join(".extracted_v6");
    if marker.is_file() {
        return;
    }
    let _ = fs::remove_dir_all(&py_home);
    let extract_zip = |archive_path: &Path, target_dir: &Path| {
        if let Ok(file) = fs::File::open(archive_path) {
            if let Ok(mut archive) = zip::ZipArchive::new(file) {
                let _ = fs::create_dir_all(target_dir);
                let mut symlinks = Vec::new();
                for i in 0..archive.len() {
                    if let Ok(mut f) = archive.by_index(i) {
                        if let Some(enclosed) = f.enclosed_name() {
                            let outpath = target_dir.join(enclosed);
                            if f.is_dir() {
                                let _ = fs::create_dir_all(&outpath);
                                continue;
                            }
                            if let Some(p) = outpath.parent() {
                                let _ = fs::create_dir_all(p);
                            }
                            let is_symlink = f
                                .unix_mode()
                                .map_or(false, |m| (m & 0o170000) == 0o120000);
                            if is_symlink {
                                let mut target_bytes = Vec::new();
                                use std::io::Read;
                                if f.read_to_end(&mut target_bytes).is_ok() {
                                    if let Ok(target_str) = String::from_utf8(target_bytes) {
                                        symlinks.push((outpath, target_str.trim().to_string()));
                                    }
                                }
                            } else {
                                let _ = fs::remove_file(&outpath);
                                if let Ok(mut outfile) = fs::File::create(&outpath) {
                                    let _ = std::io::copy(&mut f, &mut outfile);
                                }
                                #[cfg(unix)]
                                {
                                    use std::os::unix::fs::PermissionsExt;
                                    let _ = fs::set_permissions(&outpath, fs::Permissions::from_mode(0o755));
                                }
                            }
                        }
                    }
                }
                // Resolve symlinks to actual copied files so the Android dynamic linker
                // never encounters relative or broken symlinks during DT_NEEDED dlopen.
                for (outpath, target_str) in symlinks {
                    let _ = fs::remove_file(&outpath);
                    let parent = outpath.parent().unwrap_or(target_dir);
                    let mut target_path = parent.join(&target_str);
                    let mut depth = 0;
                    while depth < 10 {
                        if let Ok(link_target) = fs::read_link(&target_path) {
                            target_path = if link_target.is_relative() {
                                target_path.parent().unwrap_or(parent).join(link_target)
                            } else {
                                link_target
                            };
                            depth += 1;
                        } else {
                            break;
                        }
                    }
                    if target_path.is_file() {
                        let _ = fs::copy(&target_path, &outpath);
                    } else {
                        #[cfg(unix)]
                        let _ = std::os::unix::fs::symlink(&target_str, &outpath);
                    }
                    #[cfg(unix)]
                    {
                        use std::os::unix::fs::PermissionsExt;
                        let _ = fs::set_permissions(&outpath, fs::Permissions::from_mode(0o755));
                    }
                }
            }
        }
    };

    let py_candidates = [
        tools.bin.join("python.zip"),
        tools.bin.parent().unwrap_or(&tools.bin).join("app.cdcedit.editor").join("link-tools").join("python.zip"),
        tools.bin.parent().unwrap_or(&tools.bin).join("concat").join("link-tools").join("python.zip"),
        native_libs.join("libpython.zip.so"),
    ];
    if let Some(py_zip) = py_candidates.into_iter().find(|p| p.is_file()) {
        extract_zip(&py_zip, &py_home);
    }

    let ff_candidates = [
        tools.bin.join("ffmpeg.zip"),
        tools.bin.parent().unwrap_or(&tools.bin).join("app.cdcedit.editor").join("link-tools").join("ffmpeg.zip"),
        tools.bin.parent().unwrap_or(&tools.bin).join("concat").join("link-tools").join("ffmpeg.zip"),
        native_libs.join("libffmpeg.zip.so"),
    ];
    if let Some(ff_zip) = ff_candidates.into_iter().find(|p| p.is_file()) {
        extract_zip(&ff_zip, &py_home);
    }

    // Unpack ytdlp zipapp into a directory so Python runs it directly as script/module
    // without requiring zipimport decompression on startup.
    let ytdlp_candidates = [
        tools.bin.join("ytdlp"),
        tools.bin.parent().unwrap_or(&tools.bin).join("app.cdcedit.editor").join("link-tools").join("ytdlp"),
        tools.bin.parent().unwrap_or(&tools.bin).join("concat").join("link-tools").join("ytdlp"),
        tools.bin.join("yt-dlp"),
        native_libs.join("libytdlp.so"),
    ];
    if let Some(ytdlp_file) = ytdlp_candidates.into_iter().find(|p| p.is_file()) {
        let ytdlp_pkg = py_home.join("ytdlp_pkg");
        let _ = fs::create_dir_all(&ytdlp_pkg);
        extract_zip(&ytdlp_file, &ytdlp_pkg);
    }

    let _ = fs::write(&marker, b"ok");
}

fn video_command(tools: &Tools, cookies: Option<&Path>) -> Command {
    #[cfg(target_os = "android")]
    {
        if let Ok(native_libs) = std::env::var("CONCAT_NATIVE_LIBS") {
            let lib_dir = PathBuf::from(&native_libs);
            let py_exe = lib_dir.join("libpython.so");
            let ytdlp_candidates = [
                tools.bin.join("ytdlp"),
                tools.bin.parent().unwrap_or(&tools.bin).join("app.cdcedit.editor").join("link-tools").join("ytdlp"),
                tools.bin.parent().unwrap_or(&tools.bin).join("concat").join("link-tools").join("ytdlp"),
                tools.bin.join("yt-dlp"),
                lib_dir.join("libytdlp.so"),
            ];
            let ytdlp = ytdlp_candidates.into_iter().find(|p| p.is_file());
            if py_exe.is_file() && ytdlp.is_some() {
                let ytdlp = ytdlp.unwrap();
                ensure_android_python(tools, &lib_dir);
                let py_home = tools.bin.parent().unwrap_or(&tools.bin).join("python");
                let usr_dir = if py_home.join("usr").is_dir() {
                    py_home.join("usr")
                } else {
                    py_home.clone()
                };
                let ytdlp_pkg_main = py_home.join("ytdlp_pkg").join("__main__.py");
                let ytdlp_arg = if ytdlp_pkg_main.is_file() {
                    ytdlp_pkg_main
                } else {
                    ytdlp
                };
                let mut c = Command::new(&py_exe);
                c.arg(&ytdlp_arg);
                c.env("PYTHONHOME", &usr_dir);
                c.env("HOME", &usr_dir);
                let usr_lib = usr_dir.join("lib");
                let py_ver = usr_lib.join("python3.12");
                let py_dyn = py_ver.join("lib-dynload");
                let py_site = py_ver.join("site-packages");
                c.env(
                    "PYTHONPATH",
                    format!("{}:{}:{}", py_ver.display(), py_dyn.display(), py_site.display()),
                );
                let cert = usr_dir.join("etc").join("tls").join("cert.pem");
                if cert.is_file() {
                    c.env("SSL_CERT_FILE", &cert);
                }
                c.env(
                    "LD_LIBRARY_PATH",
                    format!("{}:{}:/system/lib64:/system/lib", usr_lib.display(), lib_dir.display()),
                );
                let tmp = tools.bin.parent().unwrap_or(&tools.bin).join("tmp");
                let _ = fs::create_dir_all(&tmp);
                c.env("TMPDIR", &tmp);
                let usr_bin = usr_dir.join("bin");
                c.env("PATH", format!("{}:{}:/system/bin", usr_bin.display(), lib_dir.display()));
                c.args([
                    "--ignore-config",
                    "--no-playlist",
                    "--no-warnings",
                    "--no-color",
                    "--socket-timeout",
                    "30",
                    "--retries",
                    "3",
                    "--extractor-args",
                    "youtube:player_client=android,web",
                ]);
                let ff_exe = lib_dir.join("libffmpeg.so");
                if ff_exe.is_file() {
                    c.arg("--ffmpeg-location").arg(&ff_exe);
                } else {
                    c.arg("--ffmpeg-location").arg(&lib_dir);
                }
                let qjs = lib_dir.join("libqjs.so");
                if qjs.is_file() {
                    c.arg("--js-runtimes").arg(format!("quickjs:{}", qjs.display()));
                }
                if let Some(p) = cookies {
                    c.arg("--cookies").arg(p);
                }
                return c;
            }
        }
    }
    let mut c = tools.command("yt-dlp");
    c.args([
        "--ignore-config",
        "--no-playlist",
        "--no-warnings",
        "--no-color",
        "--socket-timeout",
        "25",
        "--retries",
        "3",
        "--extractor-args",
        "youtube:player_client=android,web",
    ]);
    let ffmpeg = tools.exe("ffmpeg");
    if let Some(dir) = ffmpeg.parent() {
        c.arg("--ffmpeg-location").arg(dir);
    } else {
        c.arg("--ffmpeg-location").arg(&tools.bin);
    }
    let deno = tools.exe("deno");
    if deno.is_file() {
        c.arg("--js-runtimes").arg(format!("deno:{}", deno.display()));
    }
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
fn clean_title(title: &str) -> String {
    let mut words = Vec::new();
    for word in title.split_whitespace() {
        if word.starts_with('#') {
            continue;
        }
        words.push(word);
    }
    let combined = words.join(" ");
    let filtered: String = combined
        .chars()
        .filter(|c| !c.is_control() && !matches!(c, '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*'))
        .take(70)
        .collect();
    let trimmed = filtered.trim().trim_matches(['.', ' ', '-']);
    if trimmed.is_empty() {
        "video".to_string()
    } else {
        trimmed.to_string()
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
                if heights.is_empty() {
                    if let Some(formats) = v["formats"].as_array() {
                        for f in formats {
                            let fid = f["format_id"].as_str().unwrap_or("");
                            if fid.contains("hd") {
                                heights.push(1080);
                            } else if fid.contains("sd") {
                                heights.push(480);
                            }
                        }
                    }
                    if heights.is_empty() {
                        heights.extend([1080, 720, 480]);
                    }
                }
                heights.sort_unstable();
                heights.dedup();
                result.push(Item {
                    title: clean_title(v["title"].as_str().unwrap_or("Media")),
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
            let clean_name = clean_title(&item.title);
            c.arg("-o").arg(dir.join(format!("{clean_name}.%(ext)s")));
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
                    "bestvideo+bestaudio/best".to_owned()
                } else {
                    format!("bestvideo[height<={height}]+bestaudio/best[height<={height}]/best")
                };
                c.args(["-f", &format, "--merge-output-format", "mp4"]);
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
