// SPDX-License-Identifier: AGPL-3.0-or-later
//! UI wrapper for importing a CapCut PC draft into CDCAi.

use std::path::{Path, PathBuf};

use crate::studio::Studio;

/// Opens a file picker to choose a CapCut `draft_content.json`, `draft_info.json`, or draft `.zip`.
pub fn import_capcut_dialog(_studio: &mut Studio) {
    crate::host::pick_files_then(
        "Chọn tệp dự án CapCut (draft_content.json, draft_info.json hoặc .zip)".to_string(),
        Some(("CapCut Draft (*.json, *.zip)".to_string(), &["json", "zip"])),
        |studio, _, _, paths| {
            if let Some(path) = paths.into_iter().next() {
                import_capcut_draft_path(studio, &path);
            }
        },
    );
}

/// Parses and imports a CapCut draft from a given file or folder path into the window.
pub fn import_capcut_draft_path(studio: &mut Studio, input_path: &Path) {
    let default_loc = if cfg!(target_os = "android") {
        let home_candidate = crate::studio::home_folder("CDCAi");
        if !home_candidate.is_empty() && Path::new(&home_candidate).is_dir() {
            home_candidate
        } else {
            let android_p = Path::new("/storage/emulated/0/CDCAi");
            if android_p.is_dir() || std::fs::create_dir_all(android_p).is_ok() {
                android_p.to_string_lossy().into_owned()
            } else {
                let p = studio.host.dirs.data.join("Projects");
                let _ = std::fs::create_dir_all(&p);
                p.to_string_lossy().into_owned()
            }
        }
    } else if cfg!(target_os = "ios") {
        crate::studio::home_folder("Documents/CDCAi")
    } else {
        crate::studio::home_folder("Desktop")
    };

    let mut location = studio
        .prefs
        .last_project_location
        .clone()
        .filter(|l| !l.is_empty() && Path::new(l).is_dir())
        .unwrap_or(default_loc);

    if location.is_empty() || std::fs::create_dir_all(&location).is_err() {
        let p = studio.host.dirs.data.join("Projects");
        let _ = std::fs::create_dir_all(&p);
        location = p.to_string_lossy().into_owned();
    }

    match concat_host::capcut::import_capcut_draft(
        input_path,
        &PathBuf::from(&location),
        None,
        true,
        &studio.host.dirs,
    ) {
        Ok(res) => {
            let folder_name = Path::new(&res.project.path)
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or(&res.project.name)
                .to_owned();
            match studio.open_project(res.project.clone()) {
                Ok(()) => {
                    if res.reused_existing {
                        studio.notify(
                            &format!("📂 Đã mở dự án '{folder_name}' có sẵn trên máy."),
                            false,
                        );
                    } else {
                        studio.notify(
                            &format!(
                                "✨ Đã nhập thành công dự án CapCut '{}'!\nĐã phục hồi {} clips trên {} rãnh timeline.",
                                res.draft_title, res.clip_count, res.track_count
                            ),
                            false,
                        );
                    }
                }
                Err(e) => {
                    studio.notify(&format!("Lỗi khi mở dự án vừa nhập: {e}"), true);
                }
            }
        }
        Err(err) => {
            studio.notify(&format!("Không thể nhập dự án CapCut: {err}"), true);
        }
    }
}
