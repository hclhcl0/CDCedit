// SPDX-License-Identifier: AGPL-3.0-or-later
//! Import a CapCut PC Draft directly into a Concat / CDCedit project.
//!
//! CapCut PC stores drafts containing `draft_content.json` and `draft_meta_info.json`.
//! This importer parses all tracks (video, audio, text/subtitles), resolves materials,
//! converts coordinate systems and styles, and opens the imported timeline in CDCedit.

use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;

use concat_project::model::{
    Background, Clip, ClipKind, ColorSpace, MediaItem, MediaKind,
    Project, TextAlign, TextStyle, Timeline, Track, VideoSettings,
};
use serde_json::Value;

use crate::studio::Studio;

/// Main entry point: opens file picker to choose a CapCut `draft_content.json`, `draft_info.json`, or draft folder/zip.
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

/// Parse and import a CapCut draft from a given file or folder path.
pub fn import_capcut_draft_path(studio: &mut Studio, input_path: &Path) {
    // If user provided a .zip archive (common way to backup/share mobile drafts)
    let extracted_dir = if input_path.is_file()
        && input_path.extension().map_or(false, |ext| ext.eq_ignore_ascii_case("zip"))
    {
        let stem = input_path.file_stem().and_then(|s| s.to_str()).unwrap_or("capcut_draft");
        let dest = studio.host.dirs.data.join("CapCut").join(stem);
        if let Ok(file) = std::fs::File::open(input_path) {
            if let Ok(mut archive) = zip::ZipArchive::new(file) {
                let _ = std::fs::create_dir_all(&dest);
                for i in 0..archive.len() {
                    if let Ok(mut f) = archive.by_index(i) {
                        if let Some(enclosed) = f.enclosed_name() {
                            let outpath = dest.join(enclosed);
                            if f.is_dir() {
                                let _ = std::fs::create_dir_all(&outpath);
                            } else {
                                if let Some(p) = outpath.parent() {
                                    let _ = std::fs::create_dir_all(p);
                                }
                                if let Ok(mut outfile) = std::fs::File::create(&outpath) {
                                    let _ = std::io::copy(&mut f, &mut outfile);
                                }
                            }
                        }
                    }
                }
                Some(dest)
            } else {
                None
            }
        } else {
            None
        }
    } else {
        None
    };

    let target_path = extracted_dir.as_deref().unwrap_or(input_path);

    let (draft_file, draft_dir) = if target_path.is_dir() {
        // Look for candidate draft files directly or 1 level down
        let mut found = None;
        let cand_names = [
            "draft_content.json",
            "draft_info.json",
            "draft_meta_info.json",
            "template_content.json",
            "root_meta_info.json",
        ];
        for name in &cand_names {
            let p = target_path.join(name);
            if p.is_file() {
                found = Some((p, target_path.to_path_buf()));
                break;
            }
        }
        if found.is_none() {
            if let Ok(entries) = std::fs::read_dir(target_path) {
                for entry in entries.flatten() {
                    let sub = entry.path();
                    if sub.is_dir() {
                        for name in &cand_names {
                            let p = sub.join(name);
                            if p.is_file() {
                                found = Some((p, sub));
                                break;
                            }
                        }
                        if found.is_some() {
                            break;
                        }
                    }
                }
            }
        }
        found.unwrap_or_else(|| (target_path.join("draft_content.json"), target_path.to_path_buf()))
    } else if target_path.is_file() {
        let parent = target_path.parent().unwrap_or(Path::new(".")).to_path_buf();
        (target_path.to_path_buf(), parent)
    } else {
        studio.notify(&format!("Đường dẫn không hợp lệ: {}", target_path.display()), true);
        return;
    };

    let draft_file = if draft_file.is_file() {
        draft_file
    } else {
        // Check fallback filenames in draft_dir
        let candidates = [
            draft_dir.join("draft_content.json"),
            draft_dir.join("draft_info.json"),
            draft_dir.join("draft_meta_info.json"),
        ];
        let mut found = candidates.into_iter().find(|p| p.is_file());

        // Modern CapCut: draft_content.json or draft_info.json inside Timelines/<main_timeline_id>/
        if found.is_none() {
            let project_json = draft_dir.join("Timelines").join("project.json");
            if project_json.is_file() {
                if let Ok(raw) = std::fs::read_to_string(&project_json) {
                    if let Ok(pj) = serde_json::from_str::<Value>(&raw) {
                        if let Some(main_id) = pj["main_timeline_id"].as_str() {
                            let c1 = draft_dir.join("Timelines").join(main_id).join("draft_content.json");
                            let c2 = draft_dir.join("Timelines").join(main_id).join("draft_info.json");
                            if c1.is_file() {
                                found = Some(c1);
                            } else if c2.is_file() {
                                found = Some(c2);
                            }
                        }
                    }
                }
            }
        }
        if found.is_none() {
            if let Ok(entries) = std::fs::read_dir(draft_dir.join("Timelines")) {
                for e in entries.flatten() {
                    let cand1 = e.path().join("draft_content.json");
                    let cand2 = e.path().join("draft_info.json");
                    if cand1.is_file() {
                        found = Some(cand1);
                        break;
                    } else if cand2.is_file() {
                        found = Some(cand2);
                        break;
                    }
                }
            }
        }
        found.unwrap_or(draft_file)
    };

    if !draft_file.is_file() {
        studio.notify(
            &format!("Không tìm thấy tệp draft_content.json hoặc draft_info.json tại: {}", draft_dir.display()),
            true,
        );
        return;
    }

    let raw_content = match std::fs::read_to_string(&draft_file) {
        Ok(s) => s,
        Err(e) => {
            studio.notify(&format!("Không thể đọc draft_content.json: {e}"), true);
            return;
        }
    };

    let draft: Value = match serde_json::from_str(&raw_content) {
        Ok(v) => v,
        Err(e) => {
            studio.notify(&format!("Lỗi phân tích cú pháp JSON CapCut: {e}"), true);
            return;
        }
    };

    // 1. Project name from draft metadata or directory name
    let mut project_name = draft_dir
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("Dự án CapCut")
        .to_string();

    if let Some(name) = draft["draft_name"].as_str().or_else(|| draft["name"].as_str()).filter(|s| !s.trim().is_empty()) {
        project_name = name.trim().to_string();
    } else {
        for meta_name in ["draft_meta_info.json", "draft_info.json"] {
            let meta_file = draft_dir.join(meta_name);
            if meta_file.is_file() {
                if let Ok(meta_raw) = std::fs::read_to_string(&meta_file) {
                    if let Ok(meta_val) = serde_json::from_str::<Value>(&meta_raw) {
                        if let Some(name) = meta_val["draft_name"].as_str().or_else(|| meta_val["name"].as_str()).filter(|s| !s.trim().is_empty()) {
                            project_name = name.trim().to_string();
                            break;
                        }
                    }
                }
            }
        }
    }

    // Sanitize project name
    let clean_name: String = project_name
        .chars()
        .map(|c| if c.is_alphanumeric() || c == '_' || c == '-' || c == ' ' { c } else { '_' })
        .collect();
    let import_proj_name = format!("{clean_name}_imported");

    // 2. Video resolution & frame rate
    let mut width = draft["canvas_config"]["width"]
        .as_u64()
        .or_else(|| draft["width"].as_u64())
        .unwrap_or(0) as u32;
    let mut height = draft["canvas_config"]["height"]
        .as_u64()
        .or_else(|| draft["height"].as_u64())
        .unwrap_or(0) as u32;
    if width == 0 || height == 0 {
        let ratio = draft["ratio"]
            .as_str()
            .or_else(|| draft["canvas_config"]["ratio"].as_str())
            .unwrap_or("");
        if ratio == "9:16" {
            width = 1080;
            height = 1920;
        } else if ratio == "1:1" {
            width = 1080;
            height = 1080;
        } else if ratio == "4:3" {
            width = 1440;
            height = 1080;
        } else {
            width = 1920;
            height = 1080;
        }
    }
    let fps = draft["fps"].as_f64().unwrap_or(30.0);
    let (rate_num, rate_den) = if (fps - 29.97).abs() < 0.05 {
        (30000, 1001)
    } else if (fps - 23.976).abs() < 0.05 {
        (24000, 1001)
    } else if (fps - 59.94).abs() < 0.05 {
        (60000, 1001)
    } else {
        (fps.round().max(1.0) as i64, 1)
    };

    // 3. Parse Materials
    let materials = &draft["materials"];
    let mut media_list: Vec<MediaItem> = Vec::new();
    let mut media_map: HashMap<String, (String, bool, String)> = HashMap::new(); // capcut_id -> (concat_media_id, is_photo, name)
    let mut media_counter = 1usize;

    // Helper to find actual file on disk
    let resolve_file_path = |p: &str| -> Option<String> {
        let p_norm = p.replace('\\', "/");
        if Path::new(&p_norm).exists() {
            return Some(p_norm);
        }
        if let Some(file_name) = Path::new(&p_norm).file_name() {
            for sub_cand in [
                draft_dir.join(file_name),
                draft_dir.join("Resources").join(file_name),
                draft_dir.join("materials").join(file_name),
                draft_dir.join("media").join(file_name),
                draft_dir.join("audio").join(file_name),
                draft_dir.join("video").join(file_name),
            ] {
                if sub_cand.exists() {
                    return Some(sub_cand.to_string_lossy().replace('\\', "/"));
                }
            }
            #[cfg(target_os = "android")]
            {
                for folder in [
                    "/storage/emulated/0/DCIM/Camera",
                    "/storage/emulated/0/DCIM",
                    "/storage/emulated/0/Download",
                    "/storage/emulated/0/Movies",
                    "/storage/emulated/0/Pictures",
                    "/storage/emulated/0/CapCut",
                ] {
                    let cand = Path::new(folder).join(file_name);
                    if cand.exists() {
                        return Some(cand.to_string_lossy().replace('\\', "/"));
                    }
                }
            }
        }
        if !p.is_empty() {
            Some(p_norm)
        } else {
            None
        }
    };

    // Videos and Photos
    if let Some(videos) = materials["videos"].as_array() {
        for v in videos {
            let capcut_id = v["id"].as_str().unwrap_or_default().to_string();
            let raw_path = v["path"]
                .as_str()
                .or_else(|| v["extra_info"].as_str())
                .or_else(|| v["file_path"].as_str())
                .or_else(|| v["origin_path"].as_str())
                .or_else(|| v["material_name"].as_str())
                .unwrap_or_default();
            let real_path = resolve_file_path(raw_path).unwrap_or_else(|| raw_path.to_string());

            let name = v["material_name"].as_str().map(|s| s.to_string()).unwrap_or_else(|| {
                Path::new(&real_path)
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_else(|| format!("video_{media_counter}"))
            });
            let is_photo = v["type"].as_str() == Some("photo");
            let dur_us = v["duration"].as_i64().unwrap_or(10_000_000);
            let duration_sec = dur_us as f64 / 1_000_000.0;
            let item_w = v["width"].as_u64().unwrap_or(width as u64) as u32;
            let item_h = v["height"].as_u64().unwrap_or(height as u64) as u32;
            let has_audio = v["has_audio"].as_bool().unwrap_or(!is_photo);

            let concat_id = format!("m{media_counter}");
            media_counter += 1;

            let exists = Path::new(&real_path).exists();
            let media_item = if exists && let Ok(summary) = concat_host::media::probe(&real_path) {
                let new_m = summary.to_new_media();
                MediaItem {
                    id: concat_id.clone(),
                    path: new_m.path,
                    name: new_m.name,
                    duration: new_m.duration.or(Some(duration_sec)),
                    kind: new_m.kind,
                    width: new_m.width.or(Some(item_w)),
                    height: new_m.height.or(Some(item_h)),
                    frame_rate: new_m.frame_rate,
                    frame_rate_fraction: new_m.frame_rate_fraction,
                    video_codec: new_m.video_codec,
                    audio_codec: new_m.audio_codec,
                    has_audio: new_m.has_audio,
                    audio_tracks: new_m.audio_tracks,
                    placeholder: false,
                    origin: None,
                    color_range: None,
                    color_space: new_m.color_space,
                    extra: Default::default(),
                }
            } else {
                MediaItem {
                    id: concat_id.clone(),
                    path: real_path,
                    name: name.clone(),
                    duration: Some(duration_sec),
                    kind: if is_photo { MediaKind::Image } else { MediaKind::Video },
                    width: Some(item_w),
                    height: Some(item_h),
                    frame_rate: Some(fps),
                    frame_rate_fraction: Some(format!("{rate_num}/{rate_den}")),
                    video_codec: Some("h264".into()),
                    audio_codec: if has_audio { Some("aac".into()) } else { None },
                    has_audio,
                    audio_tracks: Vec::new(),
                    placeholder: !exists,
                    origin: None,
                    color_range: None,
                    color_space: ColorSpace::Sdr,
                    extra: Default::default(),
                }
            };

            media_map.insert(capcut_id, (concat_id, is_photo, name));
            media_list.push(media_item);
        }
    }

    // Images (CapCut mobile stores photos here as well)
    if let Some(images) = materials["images"].as_array() {
        for img in images {
            let capcut_id = img["id"].as_str().unwrap_or_default().to_string();
            if media_map.contains_key(&capcut_id) {
                continue;
            }
            let raw_path = img["path"]
                .as_str()
                .or_else(|| img["extra_info"].as_str())
                .or_else(|| img["file_path"].as_str())
                .or_else(|| img["origin_path"].as_str())
                .or_else(|| img["material_name"].as_str())
                .unwrap_or_default();
            let real_path = resolve_file_path(raw_path).unwrap_or_else(|| raw_path.to_string());

            let name = img["material_name"].as_str().map(|s| s.to_string()).unwrap_or_else(|| {
                Path::new(&real_path)
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_else(|| format!("photo_{media_counter}"))
            });
            let dur_us = img["duration"].as_i64().unwrap_or(5_000_000);
            let duration_sec = dur_us as f64 / 1_000_000.0;
            let item_w = img["width"].as_u64().unwrap_or(width as u64) as u32;
            let item_h = img["height"].as_u64().unwrap_or(height as u64) as u32;

            let concat_id = format!("m{media_counter}");
            media_counter += 1;

            let exists = Path::new(&real_path).exists();
            let media_item = if exists && let Ok(summary) = concat_host::media::probe(&real_path) {
                let new_m = summary.to_new_media();
                MediaItem {
                    id: concat_id.clone(),
                    path: new_m.path,
                    name: new_m.name,
                    duration: new_m.duration.or(Some(duration_sec)),
                    kind: MediaKind::Image,
                    width: new_m.width.or(Some(item_w)),
                    height: new_m.height.or(Some(item_h)),
                    frame_rate: None,
                    frame_rate_fraction: None,
                    video_codec: None,
                    audio_codec: None,
                    has_audio: false,
                    audio_tracks: Vec::new(),
                    placeholder: false,
                    origin: None,
                    color_range: None,
                    color_space: new_m.color_space,
                    extra: Default::default(),
                }
            } else {
                MediaItem {
                    id: concat_id.clone(),
                    path: real_path,
                    name: name.clone(),
                    duration: Some(duration_sec),
                    kind: MediaKind::Image,
                    width: Some(item_w),
                    height: Some(item_h),
                    frame_rate: None,
                    frame_rate_fraction: None,
                    video_codec: None,
                    audio_codec: None,
                    has_audio: false,
                    audio_tracks: Vec::new(),
                    placeholder: !exists,
                    origin: None,
                    color_range: None,
                    color_space: ColorSpace::Sdr,
                    extra: Default::default(),
                }
            };

            media_map.insert(capcut_id, (concat_id, true, name));
            media_list.push(media_item);
        }
    }

    // Audios
    if let Some(audios) = materials["audios"].as_array() {
        for a in audios {
            let capcut_id = a["id"].as_str().unwrap_or_default().to_string();
            let raw_path = a["path"]
                .as_str()
                .or_else(|| a["extra_info"].as_str())
                .or_else(|| a["file_path"].as_str())
                .or_else(|| a["name"].as_str())
                .unwrap_or_default();
            let real_path = resolve_file_path(raw_path).unwrap_or_else(|| raw_path.to_string());

            let name = a["name"].as_str().map(|s| s.to_string()).unwrap_or_else(|| {
                Path::new(&real_path)
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_else(|| format!("audio_{media_counter}"))
            });
            let dur_us = a["duration"].as_i64().unwrap_or(10_000_000);
            let duration_sec = dur_us as f64 / 1_000_000.0;

            let concat_id = format!("m{media_counter}");
            media_counter += 1;

            let exists = Path::new(&real_path).exists();
            let media_item = if exists && let Ok(summary) = concat_host::media::probe(&real_path) {
                let new_m = summary.to_new_media();
                MediaItem {
                    id: concat_id.clone(),
                    path: new_m.path,
                    name: new_m.name,
                    duration: new_m.duration.or(Some(duration_sec)),
                    kind: MediaKind::Audio,
                    width: None,
                    height: None,
                    frame_rate: None,
                    frame_rate_fraction: None,
                    video_codec: None,
                    audio_codec: new_m.audio_codec.or(Some("aac".into())),
                    has_audio: true,
                    audio_tracks: new_m.audio_tracks,
                    placeholder: false,
                    origin: None,
                    color_range: None,
                    color_space: ColorSpace::Sdr,
                    extra: Default::default(),
                }
            } else {
                MediaItem {
                    id: concat_id.clone(),
                    path: real_path,
                    name: name.clone(),
                    duration: Some(duration_sec),
                    kind: MediaKind::Audio,
                    width: None,
                    height: None,
                    frame_rate: None,
                    frame_rate_fraction: None,
                    video_codec: None,
                    audio_codec: Some("aac".into()),
                    has_audio: true,
                    audio_tracks: Vec::new(),
                    placeholder: !exists,
                    origin: None,
                    color_range: None,
                    color_space: ColorSpace::Sdr,
                    extra: Default::default(),
                }
            };

            media_map.insert(capcut_id, (concat_id, false, name));
            media_list.push(media_item);
        }
    }

    // Texts and Subtitles
    let mut text_map: HashMap<String, (String, TextStyle)> = HashMap::new();
    if let Some(texts) = materials["texts"].as_array() {
        for t in texts {
            let capcut_id = t["id"].as_str().unwrap_or_default().to_string();
            let content_str = t["content"].as_str().unwrap_or_default();
            let parsed: Value = serde_json::from_str(content_str).unwrap_or(Value::Null);

            let text_body = if !parsed["text"].as_str().unwrap_or_default().is_empty() {
                parsed["text"].as_str().unwrap_or_default().to_string()
            } else if !content_str.is_empty() && !content_str.starts_with('{') {
                content_str.to_string()
            } else {
                t["text"].as_str().unwrap_or("Phụ đề").to_string()
            };
            let styles = parsed["styles"].as_array();
            let first_style = styles.and_then(|s| s.get(0));

            // Font size
            let capcut_size = first_style
                .and_then(|s| s["size"].as_f64())
                .or_else(|| t["font_size"].as_f64())
                .unwrap_or(12.0);
            let font_size = (capcut_size / 250.0).clamp(0.02, 0.25);

            // Color
            let color = if let Some(rgb) = first_style.and_then(|s| s["fill"]["content"]["solid"]["color"].as_array()) {
                let r = (rgb.get(0).and_then(|v| v.as_f64()).unwrap_or(1.0) * 255.0).round() as u8;
                let g = (rgb.get(1).and_then(|v| v.as_f64()).unwrap_or(1.0) * 255.0).round() as u8;
                let b = (rgb.get(2).and_then(|v| v.as_f64()).unwrap_or(1.0) * 255.0).round() as u8;
                format!("#{r:02x}{g:02x}{b:02x}")
            } else if let Some(tc) = t["text_color"].as_str().filter(|s| !s.is_empty()) {
                tc.to_string()
            } else {
                "#ffffff".to_string()
            };

            // Font family
            let font_family = t["font_title"]
                .as_str()
                .or_else(|| t["font_name"].as_str())
                .filter(|s| *s != "none" && !s.is_empty())
                .unwrap_or("Hanken Grotesk")
                .to_string();

            // Font weight, italic, underline
            let bold_width = t["bold_width"].as_f64().unwrap_or(0.0);
            let font_weight = if bold_width > 0.001 { 700.0 } else { 400.0 };
            let italic = t["italic_degree"].as_i64().unwrap_or(0) > 0;
            let underline = t["underline"].as_bool().unwrap_or(false);

            // Strokes
            let mut stroke_width = 0.0;
            let mut stroke_color = "#000000".to_string();
            if let Some(strokes) = first_style.and_then(|s| s["strokes"].as_array()).and_then(|s| s.get(0)) {
                let w = strokes["width"].as_f64().unwrap_or(0.0);
                stroke_width = (w / 0.24).clamp(0.0, 1.0);
                if let Some(rgb) = strokes["content"]["solid"]["color"].as_array() {
                    let r = (rgb.get(0).and_then(|v| v.as_f64()).unwrap_or(0.0) * 255.0).round() as u8;
                    let g = (rgb.get(1).and_then(|v| v.as_f64()).unwrap_or(0.0) * 255.0).round() as u8;
                    let b = (rgb.get(2).and_then(|v| v.as_f64()).unwrap_or(0.0) * 255.0).round() as u8;
                    stroke_color = format!("#{r:02x}{g:02x}{b:02x}");
                }
            } else if let Some(bw) = t["border_width"].as_f64().filter(|&w| w > 0.0) {
                stroke_width = (bw / 0.24).clamp(0.0, 1.0);
                if let Some(bc) = t["border_color"].as_str().filter(|s| !s.is_empty()) {
                    stroke_color = bc.to_string();
                }
            }

            // Alignment
            let alignment = t["alignment"].as_i64().unwrap_or(1);
            let align = match alignment {
                0 => TextAlign::Left,
                1 => TextAlign::Center,
                2 => TextAlign::Right,
                _ => TextAlign::Center,
            };

            let shadow = t["has_shadow"].as_bool().unwrap_or(false);

            let style = TextStyle {
                content: text_body.clone(),
                font_family,
                font_size,
                font_weight,
                italic,
                underline,
                color,
                align,
                stroke_width,
                stroke_color,
                shadow,
                opacity: 1.0,
                ..TextStyle::default()
            };

            text_map.insert(capcut_id, (text_body, style));
        }
    }

    // Text templates mapping (templates that point to text materials)
    let mut template_to_text: HashMap<String, String> = HashMap::new();
    if let Some(templates) = materials["text_templates"].as_array() {
        for tmpl in templates {
            let tmpl_id = tmpl["id"].as_str().unwrap_or_default().to_string();
            if let Some(res) = tmpl["text_info_resources"].as_array().and_then(|r| r.get(0)) {
                if let Some(text_id) = res["text_material_id"].as_str() {
                    template_to_text.insert(tmpl_id, text_id.to_string());
                }
            }
        }
    }

    // 4. Parse Tracks and Segments
    let mut tracks: Vec<Track> = Vec::new();
    let mut clips: Vec<Arc<Clip>> = Vec::new();
    let mut clip_counter = 1usize;

    if let Some(capcut_tracks) = draft["tracks"].as_array() {
        for (tr_idx, track) in capcut_tracks.iter().enumerate() {
            let track_id = format!("T{}", tr_idx + 1);
            let track_type = track["type"].as_str().unwrap_or("video");

            tracks.push(Track {
                id: track_id.clone(),
                visible: true,
                muted: false,
                extra: Default::default(),
            });

            if let Some(segments) = track["segments"].as_array() {
                for seg in segments {
                    let mat_id = seg["material_id"].as_str().unwrap_or_default();

                    let start = seg["target_timerange"]["start"].as_i64().unwrap_or(0) as f64 / 1_000_000.0;
                    let duration = (seg["target_timerange"]["duration"].as_i64().unwrap_or(1_000_000) as f64 / 1_000_000.0).max(0.05);
                    let source_start = seg.get("source_timerange")
                        .and_then(|st| st["start"].as_i64())
                        .unwrap_or(0) as f64 / 1_000_000.0;
                    let speed = seg["speed"].as_f64().unwrap_or(1.0);
                    let volume = seg["volume"].as_f64().unwrap_or(1.0);

                    // Transform
                    let clip_meta = seg.get("clip");
                    let opacity = clip_meta.and_then(|c| c["alpha"].as_f64()).unwrap_or(1.0);
                    let rotation = clip_meta.and_then(|c| c["rotation"].as_f64()).unwrap_or(0.0);
                    let scale = clip_meta.and_then(|c| c["scale"]["x"].as_f64()).unwrap_or(1.0);
                    let tx = clip_meta.and_then(|c| c["transform"]["x"].as_f64()).unwrap_or(0.0);
                    let ty = clip_meta.and_then(|c| c["transform"]["y"].as_f64()).unwrap_or(0.0);
                    let flip_h = clip_meta.and_then(|c| c["flip"]["horizontal"].as_bool()).unwrap_or(false);
                    let flip_v = clip_meta.and_then(|c| c["flip"]["vertical"].as_bool()).unwrap_or(false);

                    // Coordinate conversion: CapCut [-1.0, 1.0] -> Concat [-0.5, 0.5]
                    let offset_x = tx / 2.0;
                    let offset_y = -ty / 2.0;

                    let clip_id = format!("c{clip_counter}");
                    clip_counter += 1;

                    if track_type == "text" {
                        let resolved_text = if let Some(direct) = text_map.get(mat_id) {
                            Some(direct)
                        } else if let Some(ref_id) = template_to_text.get(mat_id) {
                            text_map.get(ref_id.as_str())
                        } else {
                            None
                        };

                        let (text_body, text_style) = if let Some((body, style)) = resolved_text {
                            (body.clone(), style.clone())
                        } else {
                            ("Phụ đề".to_string(), TextStyle::default())
                        };

                        let mut clip = Clip::blank(
                            &clip_id,
                            &track_id,
                            ClipKind::Text,
                            &text_body,
                            start,
                            duration,
                        );
                        clip.text = Some(text_style);
                        clip.opacity = opacity;
                        clip.scale = scale;
                        clip.rotation = rotation;
                        clip.offset_x = offset_x;
                        clip.offset_y = offset_y;
                        clip.flip_h = flip_h;
                        clip.flip_v = flip_v;
                        clips.push(Arc::new(clip));
                    } else if let Some((mapped_media_id, is_photo, name)) = media_map.get(mat_id) {
                        let kind = if track_type == "audio" {
                            ClipKind::Audio
                        } else if *is_photo {
                            ClipKind::Image
                        } else {
                            ClipKind::Video
                        };

                        let mut clip = Clip::blank(
                            &clip_id,
                            &track_id,
                            kind,
                            name,
                            start,
                            duration,
                        );
                        clip.media_id = mapped_media_id.clone();
                        clip.source_start = source_start;
                        clip.speed = speed;
                        clip.volume = volume;
                        clip.opacity = opacity;
                        clip.scale = scale;
                        clip.rotation = rotation;
                        clip.offset_x = offset_x;
                        clip.offset_y = offset_y;
                        clip.flip_h = flip_h;
                        clip.flip_v = flip_v;
                        clips.push(Arc::new(clip));
                    } else if track_type == "video" || track_type == "audio" {
                        let is_audio = track_type == "audio";
                        let fallback_id = format!("m_fallback_{mat_id}");
                        let kind = if is_audio { ClipKind::Audio } else { ClipKind::Video };
                        let mut clip = Clip::blank(
                            &clip_id,
                            &track_id,
                            kind,
                            format!("Clip {clip_counter}"),
                            start,
                            duration,
                        );
                        clip.media_id = fallback_id.clone();
                        clip.source_start = source_start;
                        clip.speed = speed;
                        clip.volume = volume;
                        clip.opacity = opacity;
                        clip.scale = scale;
                        clip.rotation = rotation;
                        clip.offset_x = offset_x;
                        clip.offset_y = offset_y;
                        clip.flip_h = flip_h;
                        clip.flip_v = flip_v;
                        clips.push(Arc::new(clip));
                    }
                }
            }
        }
    }

    if tracks.is_empty() {
        tracks.push(Track {
            id: "T1".into(),
            visible: true,
            muted: false,
            extra: Default::default(),
        });
    }

    // 5. Build Project model
    let project = Project {
        media: media_list,
        fonts: Vec::new(),
        timelines: vec![Arc::new(Timeline {
            id: "tl1".into(),
            name: "Timeline 1".into(),
            video: VideoSettings {
                width,
                height,
                rate_num,
                rate_den,
                canvas: [0, 0, 0, 255],
                color_space: ColorSpace::Sdr,
            },
            tracks,
            clips,
            background: Background::None,
            linkage: true,
            extra: Default::default(),
        })],
        active_timeline_id: "tl1".into(),
        extra: Default::default(),
    };

    // 6. Write Concat Project to disk
    let default_loc = if cfg!(target_os = "android") {
        let home_candidate = crate::studio::home_folder("CDCedit");
        if !home_candidate.is_empty() && Path::new(&home_candidate).is_dir() {
            home_candidate
        } else {
            let android_p = Path::new("/storage/emulated/0/CDCedit");
            if android_p.is_dir() || std::fs::create_dir_all(android_p).is_ok() {
                android_p.to_string_lossy().into_owned()
            } else {
                let p = studio.host.dirs.data.join("Projects");
                let _ = std::fs::create_dir_all(&p);
                p.to_string_lossy().into_owned()
            }
        }
    } else if cfg!(target_os = "ios") {
        crate::studio::home_folder("Documents/CDCedit")
    } else {
        crate::studio::home_folder("Desktop")
    };
    let mut location = studio
        .prefs
        .last_project_location
        .clone()
        .filter(|l| !l.is_empty() && Path::new(l).is_dir())
        .unwrap_or_else(|| default_loc);

    if location.is_empty() || std::fs::create_dir_all(&location).is_err() {
        let p = studio.host.dirs.data.join("Projects");
        let _ = std::fs::create_dir_all(&p);
        location = p.to_string_lossy().into_owned();
    }

    let mut import_proj_name = format!("{clean_name}_imported");
    let mut counter = 2;
    while Path::new(&location).join(&import_proj_name).exists() {
        import_proj_name = format!("{clean_name}_imported ({counter})");
        counter += 1;
    }

    let info = match concat_host::projects::create(
        &location,
        &import_proj_name,
        width,
        height,
        rate_num,
        rate_den,
        [0, 0, 0, 255],
    ) {
        Ok(info) => info,
        Err(e) => {
            studio.notify(&format!("Không thể tạo dự án Concat mới: {e}"), true);
            return;
        }
    };

    let manifest_path = Path::new(&info.path).join("concat.json");
    if let Ok(json_str) = serde_json::to_string_pretty(&project) {
        let _ = std::fs::write(&manifest_path, json_str);
    }

    let clip_count = project.timelines[0].clips.len();
    let track_count = project.timelines[0].tracks.len();

    match studio.open_project(info) {
        Ok(()) => {
            studio.notify(
                &format!(
                    "✨ Đã nhập thành công dự án CapCut '{project_name}'!\nĐã phục hồi {clip_count} clips trên {track_count} rãnh timeline."
                ),
                false,
            );
        }
        Err(e) => {
            studio.notify(&format!("Lỗi khi mở dự án vừa nhập: {e}"), true);
        }
    }
}

