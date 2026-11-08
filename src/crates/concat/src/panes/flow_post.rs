// SPDX-License-Identifier: AGPL-3.0-or-later
//! Post-processing utilities for Flow and Vids generated outputs:
//! - Extract last frame for seamless Scene Flow continuity (scene N last frame -> scene N+1 start frame)
//! - Clean watermark / logo
//! - 1080p / 2K Lanczos + CAS super-resolution upscale
use std::path::{Path, PathBuf};
use std::process::Command;

pub(super) fn find_ffmpeg() -> Option<PathBuf> {
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            let next_to = dir.join(if cfg!(windows) { "ffmpeg.exe" } else { "ffmpeg" });
            if next_to.is_file() {
                return Some(next_to);
            }
        }
    }
    // Search in PATH
    if let Some(paths) = std::env::var_os("PATH") {
        for path in std::env::split_paths(&paths) {
            let candidate = path.join(if cfg!(windows) { "ffmpeg.exe" } else { "ffmpeg" });
            if candidate.is_file() {
                return Some(candidate);
            }
        }
    }
    // Search in project's .local-tools if running in dev
    let local = Path::new(".local-tools/ffmpeg");
    if local.is_dir() {
        if let Ok(entries) = std::fs::read_dir(local) {
            for entry in entries.flatten() {
                let bin = entry.path().join("bin").join(if cfg!(windows) { "ffmpeg.exe" } else { "ffmpeg" });
                if bin.is_file() {
                    return Some(bin);
                }
            }
        }
    }
    None
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
    let scale_filter = format!("scale=-2:{target_height}:flags=lanczos,cas=0.5");
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
    // Delogo bottom right area where Veo / Google watermark appears
    // e.g. x=W-140, y=H-60, w=120, h=45
    let filter = "delogo=x=main_w-150:y=main_h-70:w=140:h=60:show=0";
    let mut cmd = Command::new(ffmpeg);
    cmd.args(["-y", "-i"])
        .arg(video_path)
        .args(["-vf", filter, "-c:v", "libx264", "-crf", "18", "-preset", "fast", "-c:a", "copy"])
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
