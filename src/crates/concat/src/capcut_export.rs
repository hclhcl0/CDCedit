// SPDX-License-Identifier: AGPL-3.0-or-later
//! Export a Concat / CDCedit timeline and project directly into a CapCut PC Draft.
//!
//! CapCut PC stores drafts locally at:
//! Windows: `%LOCALAPPDATA%\CapCut\User Data\Projects\com.lveditor.draft\<Draft_Name>\`
//! macOS: `~/Movies/CapCut/User Data/Projects/com.lveditor.draft/<Draft_Name>/`
//!
//! The draft contains `draft_meta_info.json` and `draft_content.json`.
//! All timeranges in CapCut are in microseconds (1 second = 1,000,000 microseconds).

use std::collections::HashMap;
use std::hash::{BuildHasher, Hasher};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

use concat_project::model::{ClipKind, MediaKind, TextAlign, TextStyle};
use serde_json::{Map, Value, json};

use crate::studio::Studio;

const MINIMAL_COVER_JPEG: &[u8] = &[
    0xff, 0xd8, 0xff, 0xe0, 0x00, 0x10, 0x4a, 0x46, 0x49, 0x46, 0x00, 0x01, 0x01, 0x01, 0x00, 0x48,
    0x00, 0x48, 0x00, 0x00, 0xff, 0xdb, 0x00, 0x43, 0x00, 0x08, 0x06, 0x06, 0x07, 0x06, 0x05, 0x08,
    0x07, 0x07, 0x07, 0x09, 0x09, 0x08, 0x0a, 0x0c, 0x14, 0x0d, 0x0c, 0x0b, 0x0b, 0x0c, 0x19, 0x12,
    0x13, 0x0f, 0x14, 0x1d, 0x1a, 0x1f, 0x1e, 0x1d, 0x1a, 0x1c, 0x1c, 0x20, 0x24, 0x2e, 0x27, 0x20,
    0x22, 0x2c, 0x23, 0x1c, 0x1c, 0x28, 0x37, 0x29, 0x2c, 0x30, 0x31, 0x34, 0x34, 0x34, 0x1f, 0x27,
    0x39, 0x3d, 0x38, 0x32, 0x3c, 0x2e, 0x33, 0x34, 0x32, 0xff, 0xc0, 0x00, 0x0b, 0x08, 0x00, 0x01,
    0x00, 0x01, 0x01, 0x01, 0x11, 0x00, 0xff, 0xc4, 0x00, 0x1f, 0x00, 0x00, 0x01, 0x05, 0x01, 0x01,
    0x01, 0x01, 0x01, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x01, 0x02, 0x03, 0x04,
    0x05, 0x06, 0x07, 0x08, 0x09, 0x0a, 0x0b, 0xff, 0xda, 0x00, 0x08, 0x01, 0x01, 0x00, 0x00, 0x3f,
    0x00, 0xbf, 0x00, 0xff, 0xd9,
];


/// Generates a unique UUID v4 in uppercase format matching CapCut's identifier style.
pub fn new_uuid() -> String {
    static COUNTER: AtomicU64 = AtomicU64::new(1);
    let salt = COUNTER.fetch_add(1, Ordering::Relaxed);
    let word = |s: u64| {
        let mut hasher = std::collections::hash_map::RandomState::new().build_hasher();
        hasher.write_u64(s ^ salt);
        hasher.finish()
    };
    let w1 = word(0x9e3779b97f4a7c15);
    let w2 = word(0xbf58476d1ce4e5b9);
    format!(
        "{:08X}-{:04X}-{:04X}-{:04X}-{:012X}",
        (w1 >> 32) as u32,
        (w1 >> 16) as u16,
        w1 as u16,
        (w2 >> 48) as u16,
        w2 & 0x0000_ffff_ffff_ffff
    )
}

/// Convert seconds to microseconds for CapCut.
#[inline]
pub fn to_us(seconds: f64) -> i64 {
    (seconds * 1_000_000.0).round().max(0.0) as i64
}

/// Detect the default CapCut drafts directory on the current machine.
pub fn default_capcut_drafts_dir() -> Option<PathBuf> {
    #[cfg(target_os = "windows")]
    {
        if let Some(appdata) = std::env::var_os("LOCALAPPDATA") {
            let config = PathBuf::from(&appdata)
                .join("CapCut")
                .join("User Data")
                .join("Config")
                .join("globalSetting");
            if config.is_file() {
                if let Ok(content) = std::fs::read_to_string(&config) {
                    for line in content.lines() {
                        let trimmed = line.trim();
                        if let Some(rest) = trimmed.strip_prefix("currentCustomDraftPath=") {
                            let clean = rest.trim().replace('\\', "/").replace("//", "/");
                            let p = PathBuf::from(&clean);
                            if p.is_dir() {
                                return Some(p);
                            }
                        }
                    }
                }
            }
            let default_dir = PathBuf::from(appdata)
                .join("CapCut")
                .join("User Data")
                .join("Projects")
                .join("com.lveditor.draft");
            if default_dir.exists() {
                return Some(default_dir);
            }
        }
    }
    #[cfg(target_os = "macos")]
    {
        if let Some(home) = dirs::home_dir() {
            let config = home
                .join("Movies")
                .join("CapCut")
                .join("User Data")
                .join("Config")
                .join("globalSetting");
            if config.is_file() {
                if let Ok(content) = std::fs::read_to_string(&config) {
                    for line in content.lines() {
                        let trimmed = line.trim();
                        if let Some(rest) = trimmed.strip_prefix("currentCustomDraftPath=") {
                            let p = PathBuf::from(rest.trim());
                            if p.is_dir() {
                                return Some(p);
                            }
                        }
                    }
                }
            }
            let default_dir = home
                .join("Movies")
                .join("CapCut")
                .join("User Data")
                .join("Projects")
                .join("com.lveditor.draft");
            if default_dir.exists() {
                return Some(default_dir);
            }
        }
    }
    None
}

/// CapCut's root_meta_info.json is always located in CapCut's local app data draft folder.
pub fn capcut_root_meta_file() -> Option<PathBuf> {
    #[cfg(target_os = "windows")]
    {
        if let Some(appdata) = std::env::var_os("LOCALAPPDATA") {
            let p = PathBuf::from(appdata)
                .join("CapCut")
                .join("User Data")
                .join("Projects")
                .join("com.lveditor.draft")
                .join("root_meta_info.json");
            if p.is_file() {
                return Some(p);
            }
        }
    }
    #[cfg(target_os = "macos")]
    {
        if let Some(home) = dirs::home_dir() {
            let p = home
                .join("Movies")
                .join("CapCut")
                .join("User Data")
                .join("Projects")
                .join("com.lveditor.draft")
                .join("root_meta_info.json");
            if p.is_file() {
                return Some(p);
            }
        }
    }
    None
}

/// Main exporter: converts current Studio timeline into a CapCut Draft.
pub fn export_studio_to_capcut(studio: &mut Studio) {
    if studio.timeline().clips.is_empty() {
        studio.notify(
            "Timeline đang trống! Hãy thêm clip vào timeline trước khi xuất sang CapCut.",
            true,
        );
        return;
    }

    let has_unbaked_effects = studio.timeline().clips.iter().any(|c| {
        !c.video_effects.is_empty()
            || c.cutout.is_some()
            || c.mask.is_some()
            || !c.filters.is_empty()
            || c.crop.is_some()
    });

    match perform_export(studio) {
        Ok((draft_dir, in_capcut_app)) => {
            let _ = crate::platform::reveal(&draft_dir.to_string_lossy());
            if in_capcut_app {
                let note = if has_unbaked_effects {
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
                        draft_dir.display()
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

fn build_text_material(
    id: &str,
    text: &str,
    style_opt: Option<&TextStyle>,
    prefs: &crate::prefs::CapCutExportPrefs,
) -> Value {
    let default_style = TextStyle::default();
    let style = style_opt.unwrap_or(&default_style);

    let color_hex = if !style.color.is_empty() {
        &style.color
    } else {
        "#ffffff"
    };
    let (r, g, b) = parse_hex_color(color_hex);

    let font_size_capcut = prefs.font_size.clamp(1.0, 100.0);

    // Stroke / outline: use clip's own stroke if configured, or fallback to export prefs
    let has_stroke = if style.stroke_width > 0.0 {
        true
    } else {
        prefs.stroke_enabled
    };
    let stroke_width_pct = if style.stroke_width > 0.0 {
        style.stroke_width
    } else {
        prefs.stroke_width / 100.0
    };
    let stroke_width_val = (stroke_width_pct * 0.24).clamp(0.01, 0.5);
    let (sr, sg, sb) = if has_stroke {
        if !style.stroke_color.is_empty() {
            parse_hex_color(&style.stroke_color)
        } else {
            parse_hex_color(&prefs.stroke_color)
        }
    } else {
        (0.0, 0.0, 0.0)
    };

    let strokes_json = if has_stroke {
        json!([{
            "content": {
                "render_type": "solid",
                "solid": { "color": [sr, sg, sb] }
            },
            "width": stroke_width_val,
            "mode": 0
        }])
    } else {
        json!([])
    };

    let font_name = if !style.font_family.is_empty() {
        style.font_family.replace('"', "")
    } else {
        "none".to_string()
    };

    let bold_width = if style.font_weight >= 600.0 { 0.008 } else { 0.0 };
    let italic_degree = if style.italic { 10 } else { 0 };

    let mut style_obj = json!({
        "fill": {
            "content": {
                "render_type": "solid",
                "solid": { "color": [r, g, b] }
            }
        },
        "size": font_size_capcut,
        "useLetterColor": true,
        "range": [0, text.chars().count()]
    });

    if has_stroke {
        style_obj["strokes"] = strokes_json;
    }
    if font_name != "none" {
        style_obj["font"] = json!({ "id": "", "path": "" });
    }

    let capcut_content = json!({
        "text": text,
        "styles": [style_obj]
    })
    .to_string();

    let alignment = match style.align {
        TextAlign::Left => 0,
        TextAlign::Center => 1,
        TextAlign::Right => 2,
    };

    let mut m = Map::new();
    m.insert("add_type".into(), json!(0));
    m.insert("alignment".into(), json!(alignment));
    m.insert("background_alpha".into(), json!(1.0));
    m.insert("background_color".into(), json!(""));
    m.insert("background_fill".into(), json!(""));
    m.insert("background_height".into(), json!(0.14));
    m.insert("background_horizontal_offset".into(), json!(0.0));
    m.insert("background_round_radius".into(), json!(0.0));
    m.insert("background_style".into(), json!(0));
    m.insert("background_vertical_offset".into(), json!(0.0));
    m.insert("background_width".into(), json!(0.14));
    m.insert("base_content".into(), json!(""));
    m.insert("bold_width".into(), json!(bold_width));
    m.insert("border_alpha".into(), json!(if has_stroke { 1.0 } else { 0.0 }));
    m.insert("border_color".into(), json!(if has_stroke { &prefs.stroke_color } else { "" }));
    m.insert("border_mode".into(), json!(0));
    m.insert("border_width".into(), json!(stroke_width_val));
    m.insert("check_flag".into(), json!(47));
    m.insert("content".into(), json!(capcut_content));
    m.insert("fixed_height".into(), json!(-1.0));
    m.insert("fixed_width".into(), json!(-1.0));
    m.insert("font_path".into(), json!(""));
    m.insert("font_size".into(), json!(font_size_capcut));
    m.insert("font_title".into(), json!(font_name));
    m.insert("font_name".into(), json!(font_name));
    m.insert("global_alpha".into(), json!(style.opacity));
    m.insert("has_shadow".into(), json!(style.shadow));
    if style.shadow {
        m.insert("shadow_alpha".into(), json!(0.7));
        m.insert("shadow_angle".into(), json!(-45.0));
        m.insert("shadow_color".into(), json!("#000000"));
        m.insert("shadow_distance".into(), json!(5.0));
        m.insert("shadow_smoothing".into(), json!(0.2));
    } else {
        m.insert("shadow_alpha".into(), json!(0.0));
    }
    m.insert("id".into(), json!(id));
    m.insert("initial_scale".into(), json!(1.0));
    m.insert("inner_padding".into(), json!(-1.0));
    m.insert("italic_degree".into(), json!(italic_degree));
    m.insert("layer_weight".into(), json!(1));
    m.insert("letter_spacing".into(), json!(0.0));
    m.insert("line_feed".into(), json!(1));
    m.insert("line_max_width".into(), json!(0.82));
    m.insert("line_spacing".into(), json!(0.02));
    m.insert("name".into(), json!(""));
    m.insert("text_alpha".into(), json!(1.0));
    m.insert("text_color".into(), json!(color_hex));
    m.insert("text_size".into(), json!(30));
    m.insert("type".into(), json!("text"));
    m.insert("typesetting".into(), json!(0));
    m.insert("underline".into(), json!(style.underline));
    Value::Object(m)
}

fn build_materials_map(
    videos: Vec<Value>,
    audios: Vec<Value>,
    texts: Vec<Value>,
    speeds: Vec<Value>,
) -> Map<String, Value> {
    let empty_keys = [
        "ai_translates",
        "audio_balances",
        "audio_effects",
        "audio_fades",
        "audio_pannings",
        "audio_pitch_shifts",
        "audio_track_indexes",
        "beats",
        "canvases",
        "chromas",
        "color_curves",
        "common_mask",
        "digital_human_model_dressing",
        "digital_humans",
        "drafts",
        "effects",
        "flowers",
        "green_screens",
        "handwrites",
        "hsl",
        "hsl_curves",
        "images",
        "log_color_wheels",
        "loudnesses",
        "manual_beautys",
        "manual_deformations",
        "material_animations",
        "material_colors",
        "multi_language_refs",
        "placeholder_infos",
        "placeholders",
        "plugin_effects",
        "primary_color_wheels",
        "realtime_denoises",
        "shapes",
        "smart_crops",
        "smart_relights",
        "sound_channel_mappings",
        "stickers",
        "tail_leaders",
        "text_templates",
        "time_marks",
        "transitions",
        "video_effects",
        "video_radius",
        "video_shadows",
        "video_strokes",
        "video_trackings",
        "vocal_beautifys",
        "vocal_separations",
    ];

    let mut m = Map::new();
    for k in empty_keys {
        m.insert(k.into(), json!([]));
    }
    m.insert("videos".into(), Value::Array(videos));
    m.insert("audios".into(), Value::Array(audios));
    m.insert("texts".into(), Value::Array(texts));
    m.insert("speeds".into(), Value::Array(speeds));
    m
}


fn detect_capcut_device_platform(target_base_dir: &std::path::Path) -> (Value, String) {
    let mut candidate_dirs = vec![target_base_dir.to_path_buf()];
    if let Some(capcut_dir) = default_capcut_drafts_dir() {
        if capcut_dir != target_base_dir {
            candidate_dirs.push(capcut_dir);
        }
    }
    for base in candidate_dirs {
        if let Ok(entries) = std::fs::read_dir(&base) {
            for entry in entries.flatten() {
                let candidate = entry.path().join("draft_content.json");
                if candidate.is_file() {
                    if let Ok(raw) = std::fs::read_to_string(&candidate) {
                        if let Ok(val) = serde_json::from_str::<Value>(&raw) {
                            if let Some(p) = val.get("platform").cloned() {
                                if p.get("device_id").is_some() || p.get("mac_address").is_some() {
                                    let new_ver = val
                                        .get("new_version")
                                        .and_then(|v| v.as_str())
                                        .unwrap_or("191.0.0")
                                        .to_string();
                                    return (p, new_ver);
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    #[cfg(target_os = "windows")]
    let default_p = json!({
        "app_id": 359289,
        "app_source": "cc",
        "app_version": "9.7.0",
        "device_id": "e928d29977d0f04f0fd45f18b40a57e6",
        "hard_disk_id": "",
        "mac_address": "06c209888c1190076cf27bea50d5038d",
        "os": "windows",
        "os_version": "10.0.26100"
    });
    #[cfg(not(target_os = "windows"))]
    let default_p = json!({
        "app_id": 359289,
        "app_source": "cc",
        "app_version": "9.7.0",
        "os": "mac"
    });

    (default_p, "191.0.0".to_string())
}

fn perform_export(studio: &Studio) -> Result<(PathBuf, bool), String> {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|e| e.to_string())?;
    let now_us = (now.as_micros()) as i64;
    let now_sec = now.as_secs() as i64;

    // 1. Determine destination folder
    let (target_base_dir, in_capcut_app): (PathBuf, bool) = if let Some(capcut_dir) = default_capcut_drafts_dir() {
        (capcut_dir, true)
    } else {
        // Fallback: create in data/CapCut_Drafts
        let fallback = studio.host.dirs.data.join("CapCut_Drafts");
        let _ = std::fs::create_dir_all(&fallback);
        (fallback, false)
    };

    let proj_name = if !studio.project_name.trim().is_empty() {
        studio.project_name.trim()
    } else if !studio.timeline().name.trim().is_empty() {
        studio.timeline().name.trim()
    } else {
        "Dự án Concat"
    };

    // Sanitize folder name
    let safe_name: String = proj_name
        .chars()
        .map(|c: char| {
            if c.is_alphanumeric() || c == '_' || c == '-' || c == ' ' {
                c
            } else {
                '_'
            }
        })
        .collect();
    let draft_folder_name = if !safe_name.trim().is_empty() {
        safe_name.trim().to_string()
    } else {
        "Dự án CapCut".to_string()
    };
    let draft_dir: PathBuf = target_base_dir.join(&draft_folder_name);
    let mut draft_id = new_uuid();
    let mut main_tl_id = new_uuid();

    // If overwriting an existing draft, reuse its existing IDs so CapCut seamlessly reloads it
    if draft_dir.is_dir() {
        if let Ok(raw) = std::fs::read_to_string(draft_dir.join("draft_meta_info.json")) {
            if let Ok(val) = serde_json::from_str::<Value>(&raw) {
                if let Some(id) = val["draft_id"].as_str().filter(|s| !s.is_empty()) {
                    draft_id = id.to_string();
                }
            }
        }
        if let Ok(raw) = std::fs::read_to_string(draft_dir.join("Timelines").join("project.json")) {
            if let Ok(val) = serde_json::from_str::<Value>(&raw) {
                if let Some(tl_id) = val["main_timeline_id"].as_str().filter(|s| !s.is_empty()) {
                    main_tl_id = tl_id.to_string();
                }
            }
        }
    } else {
        std::fs::create_dir_all(&draft_dir)
            .map_err(|e| format!("Không thể tạo thư mục draft tại {}: {e}", draft_dir.display()))?;
    }

    // 2. Timeline properties
    let timeline = studio.timeline();
    let width = if timeline.video.width > 0 {
        timeline.video.width as i64
    } else {
        1920
    };
    let height = if timeline.video.height > 0 {
        timeline.video.height as i64
    } else {
        1080
    };
    let fps = if timeline.video.rate_den > 0 {
        timeline.video.rate_num as f64 / timeline.video.rate_den as f64
    } else {
        30.0
    };

    let total_duration = timeline
        .clips
        .iter()
        .map(|c| c.start + c.duration)
        .fold(0.0f64, f64::max);
    let total_duration_us = to_us(total_duration);

    // 3. Collect Materials
    let mut videos_mat: Vec<Value> = Vec::new();
    let mut audios_mat: Vec<Value> = Vec::new();
    let mut texts_mat: Vec<Value> = Vec::new();
    let mut speeds_mat: Vec<Value> = Vec::new();

    // Cache media items from Concat project
    let mut media_mat_map: HashMap<String, (String, bool)> = HashMap::new();

    for item in &studio.project().media {
        let mat_id = new_uuid();
        let path_norm = item.path.replace('\\', "/");
        let name = PathBuf::from(&item.path)
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| item.name.clone());
        let item_dur_us = to_us(item.duration.unwrap_or(10.0));
        let is_video = matches!(item.kind, MediaKind::Video | MediaKind::Image);
        let item_w = item.width.unwrap_or(width as u32) as i64;
        let item_h = item.height.unwrap_or(height as u32) as i64;

        if is_video {
            videos_mat.push(json!({
                "category_id": "",
                "category_name": "local",
                "check_flag": 62978047,
                "crop": {
                    "lower_left_x": 0.0, "lower_left_y": 1.0,
                    "lower_right_x": 1.0, "lower_right_y": 1.0,
                    "upper_left_x": 0.0, "upper_left_y": 0.0,
                    "upper_right_x": 1.0, "upper_right_y": 0.0
                },
                "crop_ratio": "free",
                "crop_scale": 1.0,
                "duration": item_dur_us,
                "extra_type_option": 0,
                "has_audio": item.has_audio,
                "height": item_h,
                "id": mat_id,
                "material_name": name,
                "path": path_norm,
                "type": if item.kind == MediaKind::Image { "photo" } else { "video" },
                "width": item_w
            }));
            media_mat_map.insert(item.id.clone(), (mat_id, true));
        } else {
            audios_mat.push(json!({
                "app_id": 0,
                "category_id": "",
                "category_name": "local",
                "check_flag": 1,
                "duration": item_dur_us,
                "extra_info": "",
                "has_audio": true,
                "id": mat_id,
                "intensifies_path": "",
                "local_material_id": "",
                "music_id": "",
                "name": name,
                "path": path_norm,
                "query": "",
                "request_id": "",
                "resource_id": "",
                "search_id": "",
                "source_platform": 0,
                "team_id": "",
                "text_id": "",
                "tone_category_id": "",
                "tone_category_name": "",
                "tone_effect_id": "",
                "tone_effect_name": "",
                "tone_speaker": "",
                "tone_type": "",
                "type": "extract_music",
                "video_id": "",
                "wave_points": []
            }));
            media_mat_map.insert(item.id.clone(), (mat_id, false));
        }
    }

    // 4. Build Tracks & Segments
    let mut tracks_json: Vec<Value> = Vec::new();

    for track in &timeline.tracks {
        let track_clips: Vec<_> = timeline
            .clips
            .iter()
            .filter(|c| c.track_id == track.id)
            .collect();
        if track_clips.is_empty() {
            continue;
        }

        let has_video = track_clips
            .iter()
            .any(|c| matches!(c.kind, ClipKind::Video | ClipKind::Image));
        let has_text = track_clips
            .iter()
            .any(|c| matches!(c.kind, ClipKind::Text));
        let track_type = if has_video {
            "video"
        } else if has_text {
            "text"
        } else {
            "audio"
        };

        let capcut_track_id = new_uuid();
        let mut segments: Vec<Value> = Vec::new();

        for (idx, clip) in track_clips.iter().enumerate() {
            let seg_id = new_uuid();
            let speed_id = new_uuid();
            speeds_mat.push(json!({
                "curve_speed": null,
                "id": speed_id,
                "mode": 0,
                "speed": clip.speed,
                "type": "speed"
            }));

            let target_start = to_us(clip.start);
            let target_dur = to_us(clip.duration);
            let source_start = to_us(clip.source_start);
            let source_dur = to_us(clip.duration * clip.speed);

            if clip.kind == ClipKind::Text {
                let text_mat_id = new_uuid();
                let content_text = clip
                    .text
                    .as_ref()
                    .map(|t| t.content.as_str())
                    .unwrap_or(clip.name.as_str());

                texts_mat.push(build_text_material(
                    &text_mat_id,
                    content_text,
                    clip.text.as_ref(),
                    &studio.prefs.capcut_export,
                ));

                let scale_val = (studio.prefs.capcut_export.scale_percent / 100.0).clamp(0.01, 10.0);
                let h_f64 = height as f64;
                let pos_y = if h_f64 > 0.0 {
                    studio.prefs.capcut_export.position_y / h_f64
                } else {
                    -0.453125
                };

                segments.push(json!({
                    "caption_info": null,
                    "cartoon": false,
                    "clip": {
                        "alpha": clip.opacity,
                        "flip": { "horizontal": clip.flip_h, "vertical": clip.flip_v },
                        "rotation": clip.rotation,
                        "scale": { "x": scale_val, "y": scale_val },
                        "transform": { "x": 0.0, "y": pos_y }
                    },
                    "color_correct_alg_result": "",
                    "common_keyframes": [],
                    "desc": "",
                    "digital_human_template_group_id": "",
                    "enable_adjust": false,
                    "enable_adjust_mask": false,
                    "enable_color_correct_adjust": false,
                    "enable_color_curves": true,
                    "enable_color_match_adjust": false,
                    "enable_color_wheels": true,
                    "enable_hsl": false,
                    "enable_hsl_curves": true,
                    "enable_lut": false,
                    "enable_mask_shadow": false,
                    "enable_mask_stroke": false,
                    "enable_smart_color_adjust": false,
                    "enable_video_mask": true,
                    "extra_material_refs": [],
                    "group_id": "",
                    "hdr_settings": null,
                    "id": seg_id,
                    "intensifies_audio": false,
                    "is_loop": false,
                    "is_placeholder": false,
                    "is_tone_modify": false,
                    "keyframe_refs": [],
                    "last_nonzero_volume": 1.0,
                    "lyric_keyframes": null,
                    "material_id": text_mat_id,
                    "raw_segment_id": "",
                    "render_index": idx,
                    "render_timerange": { "duration": 0, "start": 0 },
                    "reverse": false,
                    "source": "segmentsourcenormal",
                    "source_timerange": null,
                    "speed": 1.0,
                    "state": 0,
                    "target_timerange": { "duration": target_dur, "start": target_start },
                    "template_id": "",
                    "template_scene": "default",
                    "track_attribute": 0,
                    "track_render_index": 0,
                    "uniform_scale": { "on": true, "value": scale_val },
                    "visible": true,
                    "volume": 1.0
                }));
            } else {
                let mat_id = if let Some((id, _)) = media_mat_map.get(&clip.media_id) {
                    id.clone()
                } else {
                    let fallback_id = new_uuid();
                    videos_mat.push(json!({
                        "category_id": "",
                        "category_name": "local",
                        "check_flag": 62978047,
                        "duration": source_dur,
                        "has_audio": true,
                        "height": height,
                        "id": fallback_id,
                        "material_name": clip.name,
                        "path": clip.name,
                        "type": "video",
                        "width": width
                    }));
                    fallback_id
                };

                segments.push(json!({
                    "caption_info": null,
                    "cartoon": false,
                    "clip": {
                        "alpha": clip.opacity,
                        "flip": { "horizontal": clip.flip_h, "vertical": clip.flip_v },
                        "rotation": clip.rotation,
                        "scale": { "x": clip.scale, "y": clip.scale },
                        "transform": { "x": clip.offset_x, "y": -clip.offset_y }
                    },
                    "color_correct_alg_result": "",
                    "common_keyframes": [],
                    "desc": "",
                    "digital_human_template_group_id": "",
                    "enable_adjust": true,
                    "enable_adjust_mask": false,
                    "enable_color_correct_adjust": false,
                    "enable_color_curves": true,
                    "enable_color_match_adjust": false,
                    "enable_color_wheels": true,
                    "enable_hsl": false,
                    "enable_hsl_curves": true,
                    "enable_lut": true,
                    "enable_mask_shadow": false,
                    "enable_mask_stroke": false,
                    "enable_smart_color_adjust": false,
                    "enable_video_mask": true,
                    "extra_material_refs": [speed_id],
                    "group_id": "",
                    "hdr_settings": null,
                    "id": seg_id,
                    "intensifies_audio": false,
                    "is_loop": false,
                    "is_placeholder": false,
                    "is_tone_modify": false,
                    "keyframe_refs": [],
                    "last_nonzero_volume": clip.volume,
                    "lyric_keyframes": null,
                    "material_id": mat_id,
                    "raw_segment_id": "",
                    "render_index": idx,
                    "render_timerange": { "duration": 0, "start": 0 },
                    "reverse": false,
                    "source": "segmentsourcenormal",
                    "source_timerange": { "duration": source_dur, "start": source_start },
                    "speed": clip.speed,
                    "state": 0,
                    "target_timerange": { "duration": target_dur, "start": target_start },
                    "template_id": "",
                    "template_scene": "default",
                    "track_attribute": 0,
                    "track_render_index": 0,
                    "uniform_scale": { "on": true, "value": 1.0 },
                    "visible": true,
                    "volume": clip.volume
                }));
            }
        }

        tracks_json.push(json!({
            "attribute": 0,
            "flag": 0,
            "id": capcut_track_id,
            "is_default_name": true,
            "name": track.id,
            "segments": segments,
            "type": track_type
        }));
    }

    // Sort tracks: video (0), text (1), audio (2) so video is always the primary track
    tracks_json.sort_by_key(|t| {
        match t.get("type").and_then(|v| v.as_str()) {
            Some("video") => 0,
            Some("text") => 1,
            _ => 2,
        }
    });
    for (idx, track) in tracks_json.iter_mut().enumerate() {
        if let Some(segs) = track.get_mut("segments").and_then(|v| v.as_array_mut()) {
            for seg in segs {
                seg["track_render_index"] = json!(idx);
            }
        }
    }

    // 5. Construct `draft_meta_info.json`
    let draft_fold_path_str = draft_dir.to_string_lossy().replace('\\', "/");
    let draft_root_path_str = target_base_dir.to_string_lossy().replace('\\', "/");
    let (detected_platform, detected_new_ver) = detect_capcut_device_platform(&target_base_dir);

    let mut total_materials_size: u64 = 0;
    for item in &studio.project().media {
        if let Ok(meta) = std::fs::metadata(&item.path) {
            total_materials_size += meta.len();
        }
    }
    if total_materials_size == 0 {
        total_materials_size = 10_000_000;
    }

    let mut meta_materials_videos: Vec<Value> = Vec::new();
    for item in &studio.project().media {
        let path_norm = item.path.replace('\\', "/");
        let name = PathBuf::from(&item.path)
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| item.name.clone());
        let item_dur_us = to_us(item.duration.unwrap_or(10.0));
        let is_video = matches!(item.kind, MediaKind::Video | MediaKind::Image);
        let item_w = item.width.unwrap_or(width as u32) as i64;
        let item_h = item.height.unwrap_or(height as u32) as i64;
        let mat_id = media_mat_map
            .get(&item.id)
            .map(|(id, _)| id.clone())
            .unwrap_or_else(new_uuid);

        meta_materials_videos.push(json!({
            "ai_group_type": "",
            "create_time": now_sec,
            "duration": item_dur_us,
            "enter_from": 0,
            "extra_info": name,
            "file_Path": path_norm,
            "height": item_h,
            "id": mat_id,
            "import_time": now_sec,
            "import_time_ms": now_us,
            "item_source": 1,
            "md5": "",
            "metetype": if is_video { "video" } else { "music" },
            "roughcut_time_range": { "duration": item_dur_us, "start": 0 },
            "sub_time_range": { "duration": -1, "start": -1 },
            "type": 0,
            "width": item_w
        }));
    }

    let meta_info = json!({
        "cloud_draft_cover": false,
        "cloud_draft_sync": false,
        "cloud_package_completed_time": "",
        "draft_cloud_capcut_purchase_info": "",
        "draft_cloud_last_action_download": false,
        "draft_cloud_package_type": "",
        "draft_cloud_purchase_info": "",
        "draft_cloud_template_id": "",
        "draft_cloud_tutorial_info": "",
        "draft_cloud_videocut_purchase_info": "",
        "draft_cover": "draft_cover.jpg",
        "draft_deeplink_url": "",
        "draft_enterprise_info": {
            "draft_enterprise_extra": "",
            "draft_enterprise_id": "",
            "draft_enterprise_name": "",
            "enterprise_material": []
        },
        "draft_fold_path": draft_fold_path_str,
        "draft_has_unfinished_aigc_video_effect": false,
        "draft_id": draft_id,
        "draft_is_ae_produce": false,
        "draft_is_ai_packaging_used": false,
        "draft_is_ai_shorts": false,
        "draft_is_ai_translate": false,
        "draft_is_article_video_draft": false,
        "draft_is_cloud_temp_draft": false,
        "draft_is_from_deeplink": "false",
        "draft_is_infinite_canvas_draft": false,
        "draft_is_invisible": false,
        "draft_is_pippit_draft": false,
        "draft_is_web_article_video": false,
        "draft_materials": [
            { "type": 0, "value": meta_materials_videos },
            { "type": 1, "value": [] },
            { "type": 2, "value": [] },
            { "type": 3, "value": [] },
            { "type": 6, "value": [] },
            { "type": 7, "value": [] },
            { "type": 8, "value": [] }
        ],
        "draft_materials_copied_info": [],
        "draft_name": draft_folder_name,
        "draft_need_rename_folder": false,
        "draft_new_version": "",
        "draft_removable_storage_device": "",
        "draft_root_path": draft_root_path_str,
        "draft_segment_extra_info": [],
        "draft_timeline_materials_size_": total_materials_size,
        "draft_type": "",
        "draft_web_article_video_enter_from": "",
        "tm_draft_cloud_completed": "",
        "tm_draft_cloud_entry_id": -1,
        "tm_draft_cloud_modified": 0,
        "tm_draft_cloud_parent_entry_id": -1,
        "tm_draft_cloud_space_id": -1,
        "tm_draft_cloud_user_id": -1,
        "tm_draft_create": now_us,
        "tm_draft_modified": now_us,
        "tm_draft_removed": 0,
        "tm_duration": total_duration_us
    });

    // 6. Construct `draft_content.json`
    let materials_map = build_materials_map(videos_mat, audios_mat, texts_mat, speeds_mat);

    let draft_content = json!({
        "canvas_config": {
            "background": null,
            "height": height,
            "ratio": "16:9",
            "width": width
        },
        "color_space": 0,
        "config": {
            "adjust_max_index": 1,
            "attachment_info": [],
            "combination_max_index": 1,
            "export_range": null,
            "extract_audio_last_index": 1,
            "hdr_vivid": false,
            "lyrics_recognition_id": "",
            "lyrics_sync": true,
            "lyrics_taskinfo": [],
            "maintrack_adsorb": true,
            "material_save_mode": 0,
            "multi_language_current": "none",
            "multi_language_list": [],
            "multi_language_main": "none",
            "multi_language_mode": "none",
            "original_sound_last_index": 1,
            "record_audio_last_index": 1,
            "sticker_max_index": 1,
            "subtitle_keywords_config": null,
            "subtitle_recognition_id": "",
            "subtitle_sync": true,
            "subtitle_taskinfo": [],
            "system_font_list": [],
            "use_float_render": false,
            "video_mute": false,
            "voice_change_sync": false,
            "zoom_info_params": null
        },
        "cover": null,
        "create_time": now_sec,
        "draft_type": "video",
        "duration": total_duration_us,
        "extra_info": null,
        "fps": fps,
        "free_render_index_mode_on": false,
        "function_assistant_info": null,
        "group_container": { "groups": [] },
        "id": main_tl_id,
        "is_drop_frame_timecode": false,
        "keyframe_graph_list": [],
        "keyframes": {
            "adjusts": [],
            "audios": [],
            "effects": [],
            "filters": [],
            "handwrites": [],
            "stickers": [],
            "texts": [],
            "videos": []
        },
        "last_modified_platform": detected_platform.clone(),
        "lyrics_effects": [],
        "materials": Value::Object(materials_map),
        "mixed_track_mode_on": false,
        "mutable_config": null,
        "name": "",
        "new_version": detected_new_ver,
        "path": "",
        "platform": detected_platform,
        "relationships": [],
        "render_index_track_mode_on": true,
        "retouch_cover": null,
        "smart_ads_info": null,
        "source": "default",
        "static_cover_image_path": "",
        "time_marks": null,
        "tracks": tracks_json,
        "uneven_animation_template_info": null,
        "update_time": now_sec,
        "version": 360000
    });

    // 7. Write JSON and cover files
    let meta_path = draft_dir.join("draft_meta_info.json");
    let content_path = draft_dir.join("draft_content.json");
    let cover_path = draft_dir.join("draft_cover.jpg");

    // Extract real cover from current project
    let mut cover_written = false;
    let proj_dir = studio.session.as_ref().map(|s| s.path()).unwrap_or("");
    if let Ok(bytes) = concat_host::media::poster_frame(proj_dir) {
        if !bytes.is_empty() {
            let _ = std::fs::write(&cover_path, bytes);
            cover_written = true;
        }
    }
    if !cover_written {
        for item in &studio.project().media {
            let p = std::path::Path::new(&item.path);
            let ext = p.extension().and_then(|e| e.to_str()).unwrap_or("").to_lowercase();
            if ext == "jpg" || ext == "jpeg" {
                if std::fs::copy(p, &cover_path).is_ok() {
                    cover_written = true;
                    break;
                }
            } else if matches!(item.kind, MediaKind::Video | MediaKind::Image) {
                if let Ok(frame) = concat_host::media::still_at(&item.path, 0.0, 480) {
                    if let Ok(bytes) = concat_media::jpeg(&frame, 4) {
                        let _ = std::fs::write(&cover_path, bytes);
                        cover_written = true;
                        break;
                    }
                }
            }
        }
    }
    if !cover_written {
        let _ = std::fs::write(&cover_path, MINIMAL_COVER_JPEG);
    }

    std::fs::write(&meta_path, serde_json::to_string_pretty(&meta_info).unwrap_or_default())
        .map_err(|e| format!("Không thể ghi {}: {e}", meta_path.display()))?;

    let content_json_str = serde_json::to_string(&draft_content).unwrap_or_default();
    std::fs::write(&content_path, &content_json_str)
        .map_err(|e| format!("Không thể ghi {}: {e}", content_path.display()))?;

    // Write backups and template files for root
    let _ = std::fs::write(draft_dir.join("draft_content.json.bak"), &content_json_str);
    let _ = std::fs::write(draft_dir.join("template-2.tmp"), &content_json_str);

    // Create modern CapCut 9.x Timelines structure
    let timelines_dir = draft_dir.join("Timelines");
    let main_tl_dir = timelines_dir.join(&main_tl_id);
    let _ = std::fs::create_dir_all(&main_tl_dir);

    let project_json = json!({
        "config": {
            "color_space": -1,
            "hdr_vivid": false,
            "mixed_track_mode_on": false,
            "render_index_track_mode_on": false,
            "use_float_render": false
        },
        "create_time": now_us,
        "id": new_uuid(),
        "main_timeline_id": main_tl_id,
        "timelines": [
            {
                "create_time": now_us,
                "id": main_tl_id,
                "is_marked_delete": false,
                "name": "Dòng thời gian 01",
                "update_time": now_us
            }
        ],
        "update_time": now_us,
        "version": 0
    });
    let proj_json_str = serde_json::to_string(&project_json).unwrap_or_default();
    let _ = std::fs::write(timelines_dir.join("project.json"), &proj_json_str);
    let _ = std::fs::write(timelines_dir.join("project.json.bak"), &proj_json_str);

    // Write timeline content files
    let _ = std::fs::write(main_tl_dir.join("draft_content.json"), &content_json_str);
    let _ = std::fs::write(main_tl_dir.join("draft_content.json.bak"), &content_json_str);
    let _ = std::fs::write(main_tl_dir.join("template-2.tmp"), &content_json_str);
    if cover_path.is_file() {
        let _ = std::fs::copy(&cover_path, main_tl_dir.join("draft_cover.jpg"));
    }

    let _ = std::fs::write(
        draft_dir.join("draft_agency_config.json"),
        r#"{"is_auto_agency_enabled":false,"is_auto_agency_popup":false,"is_single_agency_mode":false,"marterials":null,"use_converter":false,"video_resolution":720}"#,
    );
    let _ = std::fs::write(draft_dir.join("draft_biz_config.json"), "");

    let attachment_pc_common_str = r#"{"ai_packaging_infos":[],"ai_packaging_report_info":{"caption_id_list":[],"commercial_material":"","material_source":"","method":"","page_from":"","style":"","task_id":"","text_style":"","tos_id":"","video_category":""},"broll":{"ai_packaging_infos":[],"ai_packaging_report_info":{"caption_id_list":[],"commercial_material":"","material_source":"","method":"","page_from":"","style":"","task_id":"","text_style":"","tos_id":"","video_category":""}},"commercial_music_category_ids":[],"pc_feature_flag":0,"recognize_tasks":[],"reference_lines_config":{"horizontal_lines":[],"is_lock":false,"is_visible":false,"vertical_lines":[]},"safe_area_type":0,"template_item_infos":[],"unlock_template_ids":[]}"#;
    let _ = std::fs::write(draft_dir.join("attachment_pc_common.json"), attachment_pc_common_str);
    let _ = std::fs::write(main_tl_dir.join("attachment_pc_common.json"), attachment_pc_common_str);

    let attachment_editing_str = r#"{"editing_draft":{"ai_remove_filter_words":{"enter_source":"","right_id":""},"ai_shorts_info":{"report_params":"","type":0},"cover_extra_info":{"draft_id":"","position":0,"select_segment_id":"","select_segment_source_start":0,"select_segment_target_start":0,"slot_image_path":"","slot_info_config":{"slot_image_path":"","used_video_algorithm_configs":[]},"type":1,"video_draft_source":-1},"crop_info_extra":{"crop_mirror_type":0,"crop_rotate":0.0,"crop_rotate_total":0.0},"cutsame_to_image_edit_template_id":"","digital_human_template_to_video_info":{"has_upload_material":false,"template_type":0},"draft_used_recommend_function":"","edit_type":0,"eye_correct_enabled_multi_face_time":0,"has_adjusted_render_layer":false,"image_ai_chat_info":{"before_chat_edit":false,"draft_modify_time":0,"generate_type":"","inspiration_item_id":"","inspiration_item_name":"","keyword_content":"","keyword_id":"","keyword_name":"","keyword_type":"","message_id":"","model_name":"","need_restore":false,"picture_id":"","prompt_content":"","prompt_from":"","skill_ids":"","sugs_info":[]},"image_ai_template_info":{"first_draw_type":"","inspiration_id":"","item_type":"","request_id":""},"is_open_expand_player":false,"is_template_text_ai_generate":false,"is_use_adjust":false,"is_use_ai_expand":false,"is_use_ai_image":false,"is_use_ai_remove":false,"is_use_ai_video":false,"is_use_audio_separation":false,"is_use_chroma_key":false,"is_use_curve_speed":false,"is_use_digital_human":false,"is_use_edit_multi_camera":false,"is_use_lip_sync":false,"is_use_lock_object":false,"is_use_loudness_unify":false,"is_use_noise_reduction":false,"is_use_one_click_beauty":false,"is_use_one_click_ultra_hd":false,"is_use_retouch_face":false,"is_use_smart_adjust_color":false,"is_use_smart_body_beautify":false,"is_use_smart_motion":false,"is_use_subtitle_recognition":false,"is_use_text_to_audio":false,"material_edit_session":{"material_edit_info":[],"session_id":"","session_time":0},"paste_segment_list":[],"profile_entrance_type":"","publish_enter_from":"","publish_type":"","single_function_type":0,"text_convert_case_types":[],"version":"1.0.0","video_recording_create_draft":"","video_to_image_edit":{"enter_from":"","image":{"draft_id":"","expected_version":"","generation_id":"","height":0,"index":0,"model_id":"","model_name":"","origin_path":"","path":"","picture_id":"","skill_id":"","status":0,"width":0},"message_id":"","output_type":0,"source_duration":0,"source_vid":""},"video_to_image_info":{"create_time":0,"enter_from":"","error_code":0,"finish_time":0,"images":[],"log_id":"","message_id":"","origin_request_finish_time":0,"origin_request_id":"","output_type":0,"request_id":"","source_cover_path":"","source_duration":0,"source_local_identifier":"","source_vid":"","source_video_path":"","status":0,"title":""}}}"#;
    let _ = std::fs::write(draft_dir.join("attachment_editing.json"), attachment_editing_str);
    let _ = std::fs::write(main_tl_dir.join("attachment_editing.json"), attachment_editing_str);

    let _ = std::fs::write(
        draft_dir.join("draft_virtual_store.json"),
        r#"{"draft_materials":[],"draft_virtual_store":[{"type":0,"value":[{"creation_time":0,"display_name":"","filter_type":0,"id":"","import_time":0,"import_time_us":0,"material_color_tag":"","sort_sub_type":0,"sort_type":0,"subdraft_filter_type":0}]},{"type":1,"value":[]},{"type":2,"value":[]}]}"#,
    );
    let _ = std::fs::write(draft_dir.join("key_value.json"), "{}");
    let _ = std::fs::write(draft_dir.join("performance_opt_info.json"), r#"{"opt_info":{"is_opt":false,"opt_type":0}}"#);

    // Build common_attachment with id_mapping for both root and timeline
    let mut seg_ids: Vec<String> = Vec::new();
    for track in &tracks_json {
        if let Some(segs) = track.get("segments").and_then(|v| v.as_array()) {
            for seg in segs {
                if let Some(id) = seg.get("id").and_then(|v| v.as_str()) {
                    seg_ids.push(id.to_string());
                }
            }
        }
    }
    let mapping: Vec<Value> = seg_ids
        .iter()
        .enumerate()
        .map(|(i, sid)| {
            json!({
                "short_id": format!("{}", 1000 + i),
                "uuid": sid
            })
        })
        .collect();
    let id_mapping_json = json!({
        "id_mapping": {
            "mapping": mapping,
            "next_index": 1000 + seg_ids.len(),
            "version": "1.0.0"
        }
    });
    let id_mapping_str = serde_json::to_string(&id_mapping_json).unwrap_or_default();

    for ca_dir in [draft_dir.join("common_attachment"), main_tl_dir.join("common_attachment")] {
        let _ = std::fs::create_dir_all(&ca_dir);
        let _ = std::fs::write(ca_dir.join("attachment_id_mapping.json"), &id_mapping_str);
        let _ = std::fs::write(ca_dir.join("attachment_action_scene.json"), r#"{"action_scene":{"removed_segments":[],"segment_infos":[]}}"#);
        let _ = std::fs::write(ca_dir.join("attachment_gen_ai_info.json"), r#"{"gen_ai":{"ai_func_config":{"ai_common_configs":[],"ai_effect_configs":[],"ai_func_list":[],"aigc_generation_configs":[]},"cc_agent_info":{"agent_stringent_section_id_list":[],"agent_stringent_used_tool_list":[],"click_cnt":0,"consume_credits_function_list":[],"conversation_ids":[],"generate_success_cnt":0,"is_agent_stringent_used":false,"is_agent_used":false,"local_section_id_list":[],"real_skill_list":[],"request_cnt":0,"request_from":[],"tool_list":[],"user_select_skill_list":[]},"id":"","scene":"","version":"1.0.0"}}"#);
        let _ = std::fs::write(ca_dir.join("attachment_pc_timeline.json"), r#"{"reference_lines_config":{"horizontal_lines":[],"is_lock":false,"is_visible":false,"vertical_lines":[]},"safe_area_type":0}"#);
        let _ = std::fs::write(ca_dir.join("attachment_plugin_draft.json"), r#"{"plugin_draft":{"plugin_segments":[],"version":"1.0.0"}}"#);
        let _ = std::fs::write(ca_dir.join("attachment_script_video.json"), r#"{"script_video":{"attachment_valid":false,"language":"","overdub_recover":[],"overdub_sentence_ids":[],"parts":[],"sync_subtitle":false,"translate_segments":[],"translate_type":"","version":"1.0.0"}}"#);
    }

    let timeline_layout_json = json!({
        "dockItems": [{ "dockIndex": 0, "ratio": 1, "timelineIds": [main_tl_id], "timelineNames": ["Dòng thời gian 01"] }],
        "layoutOrientation": 1
    });
    let _ = std::fs::write(draft_dir.join("timeline_layout.json"), serde_json::to_string(&timeline_layout_json).unwrap_or_default());

    // Update root_meta_info.json in CapCut if available so draft shows up immediately
    if in_capcut_app {
        let root_meta_file = capcut_root_meta_file().unwrap_or_else(|| target_base_dir.join("root_meta_info.json"));
        if root_meta_file.is_file() {
            if let Ok(root_raw) = std::fs::read_to_string(&root_meta_file) {
                if let Ok(mut root_val) = serde_json::from_str::<Value>(&root_raw) {
                    if let Some(store) = root_val.get_mut("all_draft_store").and_then(|v| v.as_array_mut()) {
                        store.retain(|item| {
                            let name_match = item.get("draft_name").and_then(|v| v.as_str()) == Some(&draft_folder_name);
                            let path_match = item.get("draft_fold_path").and_then(|v| v.as_str()) == Some(&draft_fold_path_str);
                            let id_match = item.get("draft_id").and_then(|v| v.as_str()) == Some(&draft_id);
                            !name_match && !path_match && !id_match
                        });
                        let new_entry = json!({
                            "cloud_draft_cover": false,
                            "cloud_draft_sync": false,
                            "draft_cloud_last_action_download": false,
                            "draft_cloud_purchase_info": "",
                            "draft_cloud_template_id": "",
                            "draft_cloud_tutorial_info": "",
                            "draft_cloud_videocut_purchase_info": "",
                            "draft_cover": format!("{draft_fold_path_str}/draft_cover.jpg"),
                            "draft_fold_path": draft_fold_path_str,
                            "draft_id": draft_id,
                            "draft_is_ai_shorts": false,
                            "draft_is_cloud_temp_draft": false,
                            "draft_is_infinite_canvas_draft": false,
                            "draft_is_invisible": false,
                            "draft_is_pippit_draft": false,
                            "draft_is_web_article_video": false,
                            "draft_json_file": format!("{draft_fold_path_str}/draft_content.json"),
                            "draft_name": draft_folder_name,
                            "draft_new_version": "",
                            "draft_root_path": draft_root_path_str,
                            "draft_timeline_materials_size": total_materials_size,
                            "draft_type": "",
                            "draft_web_article_video_enter_from": "",
                            "pippit_avatar_url": "",
                            "pippit_extra_info": "",
                            "pippit_id": "",
                            "pippit_user_name": "",
                            "streaming_edit_draft_ready": true,
                            "tm_draft_cloud_completed": "",
                            "tm_draft_cloud_entry_id": -1,
                            "tm_draft_cloud_modified": 0,
                            "tm_draft_cloud_parent_entry_id": -1,
                            "tm_draft_cloud_space_id": -1,
                            "tm_draft_cloud_user_id": -1,
                            "tm_draft_create": now_us,
                            "tm_draft_modified": now_us,
                            "tm_draft_removed": 0,
                            "tm_duration": total_duration_us
                        });
                        store.insert(0, new_entry);
                        if let Some(cnt) = root_val.get_mut("draft_ids").and_then(|v| v.as_i64()) {
                            root_val["draft_ids"] = json!(cnt + 1);
                        }
                        let _ = std::fs::write(&root_meta_file, serde_json::to_string(&root_val).unwrap_or_default());
                    }
                }
            }
        }
    }

    Ok((draft_dir, in_capcut_app))
}

fn parse_hex_color(hex: &str) -> (f64, f64, f64) {
    let clean = hex.trim_start_matches('#');
    if clean.len() >= 6 {
        let r = u8::from_str_radix(&clean[0..2], 16).unwrap_or(255) as f64 / 255.0;
        let g = u8::from_str_radix(&clean[2..4], 16).unwrap_or(255) as f64 / 255.0;
        let b = u8::from_str_radix(&clean[4..6], 16).unwrap_or(255) as f64 / 255.0;
        (r, g, b)
    } else {
        (1.0, 1.0, 1.0)
    }
}
