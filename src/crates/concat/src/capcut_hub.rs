// SPDX-License-Identifier: AGPL-3.0-or-later
// SPDX-FileCopyrightText: 2026 Concat contributors

//! CapCut Project Hub: Discovers, lists, and manages CapCut drafts
//! allowing CDCedit to browse CapCut projects just like CapCut's desktop home page.

use std::path::{Path, PathBuf};
use serde_json::Value;

use crate::capcut_export::{capcut_root_meta_file, default_capcut_drafts_dir};

#[derive(Clone, Debug)]
pub struct CapCutDraft {
    pub path: String,
    pub name: String,
    pub cover_path: Option<PathBuf>,
    pub duration_sec: f64,
    pub size_bytes: u64,
    pub modified_at_millis: u64,
    pub width: u32,
    pub height: u32,
    pub exists: bool,
}

/// Discover all CapCut drafts on this computer.
pub fn list_capcut_drafts() -> Vec<CapCutDraft> {
    let mut drafts = Vec::new();
    let mut seen_paths = std::collections::HashSet::new();

    // 1. Read root_meta_info.json (Primary index maintained by CapCut)
    if let Some(meta_path) = capcut_root_meta_file() {
        if let Ok(raw) = std::fs::read_to_string(&meta_path) {
            if let Ok(root) = serde_json::from_str::<Value>(&raw) {
                if let Some(arr) = root["all_draft_store"].as_array() {
                    for item in arr {
                        let fold_raw = item["draft_fold_path"].as_str().unwrap_or("").trim();
                        if fold_raw.is_empty() {
                            continue;
                        }
                        let fold_norm = fold_raw.replace('\\', "/");
                        if !seen_paths.insert(fold_norm.to_lowercase()) {
                            continue;
                        }

                        let fold_path = PathBuf::from(&fold_raw);
                        let exists = fold_path.is_dir();

                        let name = item["draft_name"].as_str().unwrap_or("").trim();
                        let name = if name.is_empty() {
                            fold_path
                                .file_name()
                                .and_then(|n| n.to_str())
                                .unwrap_or("Dự án CapCut")
                                .to_string()
                        } else {
                            name.to_string()
                        };

                        // Duration is in microseconds
                        let dur_us = item["tm_duration"].as_i64().unwrap_or(0);
                        let duration_sec = dur_us.max(0) as f64 / 1_000_000.0;

                        // Size in bytes
                        let size_bytes = item["draft_timeline_materials_size"].as_u64().unwrap_or(0);

                        // Modified time: microseconds -> milliseconds
                        let mod_us = item["tm_draft_modified"]
                            .as_i64()
                            .filter(|&v| v > 0)
                            .or_else(|| item["tm_draft_create"].as_i64())
                            .unwrap_or(0);
                        let modified_at_millis = (mod_us.max(0) / 1000) as u64;

                        // Cover image
                        let cover_raw = item["draft_cover"].as_str().unwrap_or("").trim();
                        let cover_path = if !cover_raw.is_empty() && Path::new(cover_raw).is_file() {
                            Some(PathBuf::from(cover_raw))
                        } else {
                            let fallback = fold_path.join("draft_cover.jpg");
                            if fallback.is_file() {
                                Some(fallback)
                            } else {
                                None
                            }
                        };

                        drafts.push(CapCutDraft {
                            path: fold_norm,
                            name,
                            cover_path,
                            duration_sec,
                            size_bytes,
                            modified_at_millis,
                            width: 1920,
                            height: 1080,
                            exists,
                        });
                    }
                }
            }
        }
    }

    // 2. Also scan default drafts directory for any folders not yet in root_meta_info.json
    if let Some(default_dir) = default_capcut_drafts_dir() {
        if let Ok(entries) = std::fs::read_dir(&default_dir) {
            for entry in entries.flatten() {
                let p = entry.path();
                if !p.is_dir() {
                    continue;
                }
                let norm = p.to_string_lossy().replace('\\', "/");
                if seen_paths.contains(&norm.to_lowercase()) {
                    continue;
                }
                if is_capcut_draft(&p) {
                    seen_paths.insert(norm.to_lowercase());
                    let name = p
                        .file_name()
                        .and_then(|n| n.to_str())
                        .unwrap_or("Dự án CapCut")
                        .to_string();
                    let cover = p.join("draft_cover.jpg");
                    let cover_path = if cover.is_file() { Some(cover) } else { None };
                    let modified_at_millis = entry
                        .metadata()
                        .ok()
                        .and_then(|m| m.modified().ok())
                        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                        .map(|d| d.as_millis() as u64)
                        .unwrap_or(0);

                    drafts.push(CapCutDraft {
                        path: norm,
                        name,
                        cover_path,
                        duration_sec: 0.0,
                        size_bytes: 0,
                        modified_at_millis,
                        width: 1920,
                        height: 1080,
                        exists: true,
                    });
                }
            }
        }
    }

    // Sort newest first by default
    drafts.sort_by(|a, b| b.modified_at_millis.cmp(&a.modified_at_millis));
    drafts
}

/// Check if a directory is a CapCut draft folder.
pub fn is_capcut_draft(path: &Path) -> bool {
    if !path.exists() {
        return false;
    }
    if path.is_file() {
        return path.file_name().map_or(false, |n| n == "draft_content.json");
    }
    path.join("draft_content.json").is_file()
        || path.join("Timelines").is_dir()
        || path.join("draft_meta_info.json").is_file()
}

/// Filter and sort CapCut drafts for the start screen grid.
pub fn shown_capcut<'a>(
    drafts: &'a [CapCutDraft],
    query: &str,
    sort: i32,
) -> Vec<&'a CapCutDraft> {
    let words: Vec<String> = query.split_whitespace().map(str::to_lowercase).collect();
    let mut shown: Vec<&'a CapCutDraft> = drafts
        .iter()
        .filter(|draft| {
            if words.is_empty() {
                return true;
            }
            let name = draft.name.to_lowercase();
            let path = draft.path.to_lowercase();
            words.iter().all(|w| name.contains(w.as_str()) || path.contains(w.as_str()))
        })
        .collect();

    if sort == crate::panes::start::SORT_NAME {
        shown.sort_by_cached_key(|draft| draft.name.to_lowercase());
    } else {
        shown.sort_by(|a, b| b.modified_at_millis.cmp(&a.modified_at_millis));
    }

    shown
}
