// SPDX-License-Identifier: AGPL-3.0-or-later
//! UI wrapper for exporting a CDCAi timeline into a CapCut PC draft.

use std::path::Path;

pub use concat_host::capcut::{capcut_drafts_dirs, capcut_root_meta_file};

use crate::studio::Studio;

/// Exports the active Studio timeline into a CapCut PC draft and optionally launches CapCut.
pub fn export_studio_to_capcut(studio: &mut Studio, launch: bool) {
    if studio.timeline().clips.is_empty() {
        studio.notify(
            "Timeline đang trống! Hãy thêm clip vào timeline trước khi xuất sang CapCut.",
            true,
        );
        return;
    }

    let proj_dir = Path::new(studio.session.as_ref().map(|s| s.path()).unwrap_or(""));
    match concat_host::capcut::export_project_to_capcut(
        studio.project(),
        proj_dir,
        &studio.project_name,
        &studio.prefs.capcut_export,
        None,
        &studio.host.dirs,
    ) {
        Ok(result) => {
            let mut launched = false;
            let _ = &mut launched;
            if launch && let Some(exe) = concat_host::capcut::find_capcut_exe() {
                let _ = &exe;
                #[cfg(target_os = "windows")]
                {
                    if std::process::Command::new(&exe).spawn().is_ok() {
                        launched = true;
                    }
                }
                #[cfg(target_os = "macos")]
                {
                    if std::process::Command::new("open")
                        .arg("-a")
                        .arg(&exe)
                        .spawn()
                        .is_ok()
                    {
                        launched = true;
                    }
                }
            }

            if !launched {
                let _ = crate::platform::reveal(&result.draft_dir.to_string_lossy());
            }

            if launched {
                studio.notify(
                    "🚀 Đã đồng bộ sang CapCut và tự động mở ứng dụng CapCut PC!",
                    false,
                );
            } else if result.in_capcut_app {
                let note = if result.has_unbaked_effects {
                    "\n(Mẹo: Clip có hiệu ứng hoặc AI cutout có thể click chuột phải chọn 'Render clip kèm hiệu ứng (Bake video cho CapCut)' trước khi xuất để giữ nguyên 100% hiệu ứng)."
                } else {
                    ""
                };
                studio.notify(
                    &format!(
                        "✨ Đã xuất dự án sang CapCut thành công!\nHãy mở CapCut PC để xem dự án trong trang chủ.{note}"
                    ),
                    false,
                );
            } else {
                studio.notify(
                    &format!(
                        "✨ Đã tạo thư mục dự án CapCut Draft tại:\n{}\nBạn có thể copy thư mục này vào máy đã cài CapCut.",
                        result.draft_dir.display()
                    ),
                    false,
                );
            }
        }
        Err(err) => {
            studio.notify(&format!("Lỗi khi xuất dự án sang CapCut: {err}"), true);
        }
    }
}
