// SPDX-License-Identifier: AGPL-3.0-or-later
//! Post-processing utilities for Flow and Vids generated outputs:
//! - Extract last frame for seamless Scene Flow continuity (scene N last frame -> scene N+1 start frame)
//! - Clean watermark / logo
//! - 1080p / 2K Lanczos + CAS super-resolution upscale
use std::path::{Path, PathBuf};
use std::process::Command;

pub(super) fn find_ffmpeg() -> Option<PathBuf> {
    // 1. Concat link-tools directory via AppDirs
    if let Ok(dirs) = concat_host::dirs::AppDirs::locate() {
        let tools = concat_host::link_import::Tools::new(&dirs);
        let ff = tools.exe("ffmpeg");
        if ff.is_file() {
            return Some(ff);
        }
    }
    // 2. Beside running executable
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            let next_to = dir.join(if cfg!(windows) { "ffmpeg.exe" } else { "ffmpeg" });
            if next_to.is_file() {
                return Some(next_to);
            }
        }
    }
    // 3. FFMPEG_DIR environment variable
    if let Ok(ff_dir) = std::env::var("FFMPEG_DIR") {
        let bin = Path::new(&ff_dir).join("bin").join(if cfg!(windows) { "ffmpeg.exe" } else { "ffmpeg" });
        if bin.is_file() {
            return Some(bin);
        }
        let direct = Path::new(&ff_dir).join(if cfg!(windows) { "ffmpeg.exe" } else { "ffmpeg" });
        if direct.is_file() {
            return Some(direct);
        }
    }
    // 4. System PATH
    if let Some(paths) = std::env::var_os("PATH") {
        for path in std::env::split_paths(&paths) {
            let candidate = path.join(if cfg!(windows) { "ffmpeg.exe" } else { "ffmpeg" });
            if candidate.is_file() {
                return Some(candidate);
            }
        }
    }
    // 5. Project's .local-tools (searching up from current directory and current exe)
    let mut check_roots = Vec::new();
    if let Ok(cwd) = std::env::current_dir() {
        check_roots.push(cwd);
    }
    if let Ok(exe) = std::env::current_exe() {
        if let Some(p) = exe.parent() {
            check_roots.push(p.to_path_buf());
        }
    }
    for root in check_roots {
        let mut cur = Some(root.as_path());
        while let Some(dir) = cur {
            let local = dir.join(".local-tools").join("ffmpeg");
            if local.is_dir() {
                if let Ok(entries) = std::fs::read_dir(&local) {
                    for entry in entries.flatten() {
                        let bin = entry.path().join("bin").join(if cfg!(windows) { "ffmpeg.exe" } else { "ffmpeg" });
                        if bin.is_file() {
                            return Some(bin);
                        }
                        let direct = entry.path().join(if cfg!(windows) { "ffmpeg.exe" } else { "ffmpeg" });
                        if direct.is_file() {
                            return Some(direct);
                        }
                    }
                }
            }
            cur = dir.parent();
        }
    }
    None
}

pub(super) fn get_video_dimensions(video_path: &Path) -> (u32, u32) {
    if let Ok(summary) = concat_host::media::probe(&video_path.to_string_lossy()) {
        if let Some(video) = summary.video {
            if video.width > 0 && video.height > 0 {
                return (video.width, video.height);
            }
        }
    }
    // Fallback: ffprobe if available
    if let Some(ffmpeg) = find_ffmpeg() {
        let ffprobe = ffmpeg.with_file_name(if cfg!(windows) { "ffprobe.exe" } else { "ffprobe" });
        if ffprobe.is_file() {
            let mut cmd = Command::new(&ffprobe);
            cmd.args(["-v", "error", "-select_streams", "v:0", "-show_entries", "stream=width,height", "-of", "csv=s=x:p=0"])
                .arg(video_path);
            #[cfg(windows)]
            {
                use std::os::windows::process::CommandExt;
                cmd.creation_flags(0x08000000);
            }
            if let Ok(output) = cmd.output() {
                if output.status.success() {
                    let out_str = String::from_utf8_lossy(&output.stdout);
                    let parts: Vec<&str> = out_str.trim().split('x').collect();
                    if parts.len() == 2 {
                        if let (Ok(w), Ok(h)) = (parts[0].parse::<u32>(), parts[1].parse::<u32>()) {
                            if w > 0 && h > 0 {
                                return (w, h);
                            }
                        }
                    }
                }
            }
        }
    }
    (1280, 720)
}

pub(super) fn extract_last_frame(video_path: &Path, output_png: &Path) -> Result<PathBuf, String> {
    let ffmpeg = find_ffmpeg().ok_or_else(|| "Không tìm thấy ffmpeg để trích xuất frame cuối.".to_owned())?;
    if let Some(parent) = output_png.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let mut cmd = Command::new(ffmpeg);
    cmd.args(["-y", "-sseof", "-0.1", "-i"])
        .arg(video_path)
        .args(["-frames:v", "1", "-q:v", "2"])
        .arg(output_png);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x08000000);
    }
    let output = cmd.output().map_err(|e| format!("Lỗi khi chạy ffmpeg: {e}"))?;
    if !output.status.success() || !output_png.is_file() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("Không thể trích xuất frame cuối: {stderr}"));
    }
    Ok(output_png.to_owned())
}

pub(super) fn upscale_video(video_path: &Path, target_height: u32) -> Result<PathBuf, String> {
    let ffmpeg = find_ffmpeg().ok_or_else(|| "Không tìm thấy ffmpeg để upscale video.".to_owned())?;
    let stem = video_path.file_stem().and_then(|s| s.to_str()).unwrap_or("video");
    let ext = video_path.extension().and_then(|s| s.to_str()).unwrap_or("mp4");
    let label = if target_height >= 1440 { "2K" } else { "1080p" };
    let output_path = video_path.with_file_name(format!("{stem}_{label}.{ext}"));

    let (w, h) = get_video_dimensions(video_path);
    // If portrait (height > width), target width as short edge; otherwise target height
    let scale_filter = if h > w {
        format!("scale={target_height}:-2:flags=lanczos,cas=0.5")
    } else {
        format!("scale=-2:{target_height}:flags=lanczos,cas=0.5")
    };

    let mut cmd = Command::new(ffmpeg);
    cmd.args(["-y", "-i"])
        .arg(video_path)
        .args(["-vf", &scale_filter, "-c:v", "libx264", "-crf", "18", "-preset", "fast", "-c:a", "copy"])
        .arg(&output_path);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x08000000);
    }
    let output = cmd.output().map_err(|e| format!("Lỗi khi chạy ffmpeg upscale: {e}"))?;
    if !output.status.success() || !output_path.is_file() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("Lỗi upscale video: {stderr}"));
    }
    Ok(output_path)
}

pub(super) fn clean_watermark(video_path: &Path) -> Result<PathBuf, String> {
    let ffmpeg = find_ffmpeg().ok_or_else(|| "Không tìm thấy ffmpeg để làm sạch watermark.".to_owned())?;
    let stem = video_path.file_stem().and_then(|s| s.to_str()).unwrap_or("video");
    let ext = video_path.extension().and_then(|s| s.to_str()).unwrap_or("mp4");
    let output_path = video_path.with_file_name(format!("{stem}_clean.{ext}"));

    let (w, h) = get_video_dimensions(video_path);
    // On 1280x720 video, Veo watermark is roughly 140x60 in bottom-right corner with 10px margin.
    // Scale delogo box proportionally for larger resolutions.
    let scale = (w as f32 / 1280.0).max(h as f32 / 720.0).max(1.0);
    let logo_w = (140.0 * scale).round() as u32;
    let logo_h = (60.0 * scale).round() as u32;
    let margin_x = (10.0 * scale).round() as u32;
    let margin_y = (10.0 * scale).round() as u32;
    let x = w.saturating_sub(logo_w + margin_x);
    let y = h.saturating_sub(logo_h + margin_y);

    let filter = format!("delogo=x={x}:y={y}:w={logo_w}:h={logo_h}:show=0");
    let mut cmd = Command::new(ffmpeg);
    cmd.args(["-y", "-i"])
        .arg(video_path)
        .args(["-vf", &filter, "-c:v", "libx264", "-crf", "18", "-preset", "fast", "-c:a", "copy"])
        .arg(&output_path);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x08000000);
    }
    let output = cmd.output().map_err(|e| format!("Lỗi khi làm sạch watermark: {e}"))?;
    if !output.status.success() || !output_path.is_file() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("Lỗi làm sạch watermark: {stderr}"));
    }
    Ok(output_path)
}

pub(super) fn extract_thumbnail(video_path: &Path) -> Option<PathBuf> {
    if !video_path.is_file() {
        return None;
    }
    let thumb_path = video_path.with_extension("thumb.jpg");
    if thumb_path.is_file() {
        return Some(thumb_path);
    }
    let ffmpeg = find_ffmpeg()?;
    let mut cmd = Command::new(ffmpeg);
    cmd.args(["-y", "-ss", "0.5", "-i"])
        .arg(video_path)
        .args(["-frames:v", "1", "-q:v", "3", "-vf", "scale=320:-2"])
        .arg(&thumb_path);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x08000000);
    }
    let output = cmd.output().ok()?;
    if output.status.success() && thumb_path.is_file() {
        Some(thumb_path)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_ffmpeg_binary() {
        let ffmpeg = find_ffmpeg();
        assert!(ffmpeg.is_some(), "ffmpeg must be found");
        assert!(ffmpeg.unwrap().is_file());
    }
}

