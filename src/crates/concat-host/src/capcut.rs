// SPDX-License-Identifier: AGPL-3.0-or-later
//! Import and export CapCut PC drafts to and from CDCAi projects.
//!
//! CapCut PC drafts live on disk with `draft_content.json` (or `draft_info.json`,
//! or inside `Timelines/<main_timeline_id>/`) alongside `draft_meta_info.json`.
//! All timeline timestamps in CapCut are measured in microseconds
//! (`1 s = 1_000_000 us`).

use std::collections::HashMap;
use std::hash::{BuildHasher, Hasher};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use concat_project::model::{
    Background, Clip, ClipKind, ColorSpace, MediaItem, MediaKind, Project, TextAlign, TextStyle,
    Timeline, Track, VideoSettings,
};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};

use crate::dirs::AppDirs;
use crate::projects::{self, ProjectInfo};

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

/// Preferences for subtitle styling and layout when exporting a project to a CapCut PC draft.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct CapCutTextOptions {
    /// Default text size on the CapCut canvas (`1.0..=100.0`).
    pub font_size: f64,
    /// Scale percentage for exported subtitle blocks (`100.0` = unscaled).
    pub scale_percent: f64,
    /// Vertical CapCut offset in design pixels (negative places lower on screen).
    pub position_y: f64,
    /// Whether subtitle outlines are enabled when the clip does not set its own stroke width.
    pub stroke_enabled: bool,
    /// Outline colour as `#rrggbb`.
    pub stroke_color: String,
    /// Outline thickness slider value (`0..=100`).
    pub stroke_width: f64,
}

impl Default for CapCutTextOptions {
    fn default() -> Self {
        Self {
            font_size: 5.0,
            scale_percent: 83.0,
            position_y: -870.0,
            stroke_enabled: true,
            stroke_color: "#000000".to_string(),
            stroke_width: 25.0,
        }
    }
}

/// Outcome of importing a CapCut PC draft into a CDCAi project folder.
#[derive(Clone, Debug)]
pub struct CapCutImportResult {
    /// Opened or created CDCAi project summary.
    pub project: ProjectInfo,
    /// Display name read from the CapCut draft.
    pub draft_title: String,
    /// Number of clips reconstructed on the primary timeline.
    pub clip_count: usize,
    /// Number of tracks created on the primary timeline.
    pub track_count: usize,
    /// True if an already-imported matching project folder was reopened.
    pub reused_existing: bool,
}

/// Outcome of exporting a CDCAi project as a CapCut PC draft folder.
#[derive(Clone, Debug)]
pub struct CapCutExportResult {
    /// Folder containing `draft_content.json` and `draft_meta_info.json`.
    pub draft_dir: PathBuf,
    /// True when written into CapCut PC's detected local drafts directory.
    pub in_capcut_app: bool,
    /// True if timeline clips carry unbaked CDCAi effects/cutouts/masks.
    pub has_unbaked_effects: bool,
}

/// Generates an uppercase UUID matching CapCut's identifier format.
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

/// Converts seconds to CapCut microseconds.
#[inline]
pub fn to_us(seconds: f64) -> i64 {
    (seconds * 1_000_000.0).round().max(0.0) as i64
}

/// Returns candidate CapCut draft root directories on this machine.
pub fn capcut_drafts_dirs() -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    if let Some(d) = default_capcut_drafts_dir() {
        dirs.push(d);
    }
    dirs
}

/// Detects the default CapCut drafts directory on Windows or macOS.
pub fn default_capcut_drafts_dir() -> Option<PathBuf> {
    #[cfg(target_os = "windows")]
    {
        if let Some(appdata) = std::env::var_os("LOCALAPPDATA") {
            let config = PathBuf::from(&appdata)
                .join("CapCut")
                .join("User Data")
                .join("Config")
                .join("globalSetting");
            if config.is_file()
                && let Ok(content) = std::fs::read_to_string(&config)
            {
                for line in content.lines() {
                    if let Some(rest) = line.trim().strip_prefix("currentCustomDraftPath=") {
                        let clean = rest.trim().replace('\\', "/").replace("//", "/");
                        let p = PathBuf::from(&clean);
                        if p.is_dir() {
                            return Some(p);
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
        None
    }
    #[cfg(target_os = "macos")]
    {
        if let Some(home) = std::env::var_os("HOME").map(PathBuf::from) {
            let config = home
                .join("Movies")
                .join("CapCut")
                .join("User Data")
                .join("Config")
                .join("globalSetting");
            if config.is_file()
                && let Ok(content) = std::fs::read_to_string(&config)
            {
                for line in content.lines() {
                    if let Some(rest) = line.trim().strip_prefix("currentCustomDraftPath=") {
                        let p = PathBuf::from(rest.trim());
                        if p.is_dir() {
                            return Some(p);
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
        None
    }
    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    {
        None
    }
}

/// Locates CapCut's `root_meta_info.json` catalog file on Windows or macOS.
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
        None
    }
    #[cfg(target_os = "macos")]
    {
        if let Some(home) = std::env::var_os("HOME").map(PathBuf::from) {
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
        None
    }
    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    {
        None
    }
}

/// Locates the CapCut PC desktop executable if installed.
pub fn find_capcut_exe() -> Option<PathBuf> {
    #[cfg(target_os = "windows")]
    {
        if let Ok(local) = std::env::var("LOCALAPPDATA") {
            let direct = PathBuf::from(&local)
                .join("CapCut")
                .join("Apps")
                .join("CapCut.exe");
            if direct.is_file() {
                return Some(direct);
            }
            let apps_dir = PathBuf::from(&local).join("CapCut").join("Apps");
            if let Ok(entries) = std::fs::read_dir(&apps_dir) {
                let mut exes: Vec<PathBuf> = entries
                    .flatten()
                    .map(|entry| entry.path().join("CapCut.exe"))
                    .filter(|sub_exe| sub_exe.is_file())
                    .collect();
                exes.sort();
                if let Some(latest) = exes.into_iter().next_back() {
                    return Some(latest);
                }
            }
        }
        for prog in ["ProgramFiles", "ProgramFiles(x86)"] {
            if let Ok(pf) = std::env::var(prog) {
                let direct = PathBuf::from(&pf).join("CapCut").join("CapCut.exe");
                if direct.is_file() {
                    return Some(direct);
                }
            }
        }
        None
    }
    #[cfg(target_os = "macos")]
    {
        let mac_app = PathBuf::from("/Applications/CapCut.app");
        mac_app.is_dir().then_some(mac_app)
    }
    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    {
        None
    }
}

/// Parses a CapCut PC draft file, folder, or `.zip` archive and writes a new
/// CDCAi project under `location`.
pub fn import_capcut_draft(
    input_path: &Path,
    location: &Path,
    custom_name: Option<&str>,
    reuse_existing: bool,
    dirs: &AppDirs,
) -> Result<CapCutImportResult, String> {
    let extracted_dir = extract_if_zip(input_path, dirs);
    let target_path = extracted_dir.as_deref().unwrap_or(input_path);
    let (draft_file, draft_dir) = resolve_draft_file(target_path)?;

    let raw_content = std::fs::read_to_string(&draft_file)
        .map_err(|e| format!("Could not read {}: {e}", draft_file.display()))?;
    let draft: Value = serde_json::from_str(&raw_content)
        .map_err(|e| format!("Invalid CapCut JSON in {}: {e}", draft_file.display()))?;

    let project_name = detect_draft_title(&draft, &draft_dir);
    let clean_name: String = project_name
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '_' || c == '-' || c == ' ' {
                c
            } else {
                '_'
            }
        })
        .collect();

    let (width, height, fps, rate_num, rate_den) = read_canvas_and_fps(&draft);
    let (media_list, media_map) =
        parse_materials(&draft, &draft_dir, width, height, fps, rate_num, rate_den);
    let (text_map, template_to_text) = parse_text_materials(&draft);
    let (tracks, clips) = parse_tracks(&draft, &media_map, &text_map, &template_to_text);

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

    std::fs::create_dir_all(location).map_err(|e| {
        format!(
            "Could not prepare project location {}: {e}",
            location.display()
        )
    })?;

    if custom_name.is_none() && reuse_existing {
        let existing_imported = location.join(format!("{clean_name}_imported"));
        if projects::is_project(&existing_imported)
            && let Ok(info) = projects::open(existing_imported.to_str().unwrap_or(""))
        {
            return Ok(CapCutImportResult {
                project: info,
                draft_title: project_name,
                clip_count: project.timelines[0].clips.len(),
                track_count: project.timelines[0].tracks.len(),
                reused_existing: true,
            });
        }
    }

    let import_proj_name = match custom_name.map(str::trim).filter(|n| !n.is_empty()) {
        Some(name) => name.to_owned(),
        None => {
            let mut candidate = format!("{clean_name}_imported");
            let mut counter = 2;
            while location.join(&candidate).exists() {
                candidate = format!("{clean_name}_imported ({counter})");
                counter += 1;
            }
            candidate
        }
    };

    let location_str = location.to_string_lossy().into_owned();
    let info = projects::create(
        &location_str,
        &import_proj_name,
        width,
        height,
        rate_num,
        rate_den,
        [0, 0, 0, 255],
    )?;

    let clip_count = project.timelines[0].clips.len();
    let track_count = project.timelines[0].tracks.len();
    let doc = concat_project::Editor::with_project(project).to_document(
        &concat_project::DocumentSettings {
            name: info.name.clone(),
            width: info.width,
            height: info.height,
            rate_num: info.rate_num,
            rate_den: info.rate_den,
        },
    );
    projects::save(&info.path, &doc)?;

    Ok(CapCutImportResult {
        project: info,
        draft_title: project_name,
        clip_count,
        track_count,
        reused_existing: false,
    })
}

/// Exports `project` to a CapCut PC draft directory (into CapCut's detected
/// local drafts folder when `output_dir` is `None`, or `dirs.data/CapCut_Drafts`
/// fallback, or explicitly inside `output_dir`).
pub fn export_project_to_capcut(
    project: &Project,
    project_dir: &Path,
    project_name: &str,
    text_options: &CapCutTextOptions,
    output_dir: Option<&Path>,
    dirs: &AppDirs,
) -> Result<CapCutExportResult, String> {
    let timeline = project.active();
    if timeline.clips.is_empty() {
        return Err("The active timeline is empty; add clips before exporting to CapCut.".into());
    }

    let has_unbaked_effects = timeline.clips.iter().any(|c| {
        !c.video_effects.is_empty()
            || c.cutout.is_some()
            || c.mask.is_some()
            || !c.filters.is_empty()
            || c.crop.is_some()
    });

    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|e| e.to_string())?;
    let now_us = now.as_micros() as i64;
    let now_sec = now.as_secs() as i64;

    let (target_base_dir, in_capcut_app): (PathBuf, bool) = if let Some(explicit) = output_dir {
        std::fs::create_dir_all(explicit)
            .map_err(|e| format!("Could not create {}: {e}", explicit.display()))?;
        (explicit.to_path_buf(), false)
    } else if let Some(capcut_dir) = default_capcut_drafts_dir() {
        (capcut_dir, true)
    } else {
        let fallback = dirs.data.join("CapCut_Drafts");
        std::fs::create_dir_all(&fallback)
            .map_err(|e| format!("Could not create {}: {e}", fallback.display()))?;
        (fallback, false)
    };

    let raw_name = if !project_name.trim().is_empty() {
        project_name.trim()
    } else if !timeline.name.trim().is_empty() {
        timeline.name.trim()
    } else {
        "CDCAi Project"
    };
    let safe_name: String = raw_name
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
        "CapCut Draft".to_string()
    };
    let draft_dir: PathBuf = target_base_dir.join(&draft_folder_name);
    let mut draft_id = new_uuid();
    let mut main_tl_id = new_uuid();

    if draft_dir.is_dir() {
        if let Ok(raw) = std::fs::read_to_string(draft_dir.join("draft_meta_info.json"))
            && let Ok(val) = serde_json::from_str::<Value>(&raw)
            && let Some(id) = val["draft_id"].as_str().filter(|s| !s.is_empty())
        {
            draft_id = id.to_string();
        }
        if let Ok(raw) = std::fs::read_to_string(draft_dir.join("Timelines").join("project.json"))
            && let Ok(val) = serde_json::from_str::<Value>(&raw)
            && let Some(tl_id) = val["main_timeline_id"].as_str().filter(|s| !s.is_empty())
        {
            main_tl_id = tl_id.to_string();
        }
    } else {
        std::fs::create_dir_all(&draft_dir).map_err(|e| {
            format!(
                "Could not create draft directory {}: {e}",
                draft_dir.display()
            )
        })?;
    }

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

    let mut videos_mat: Vec<Value> = Vec::new();
    let mut audios_mat: Vec<Value> = Vec::new();
    let mut texts_mat: Vec<Value> = Vec::new();
    let mut speeds_mat: Vec<Value> = Vec::new();
    let mut media_mat_map: HashMap<String, (String, bool)> = HashMap::new();

    for item in &project.media {
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
        let has_text = track_clips.iter().any(|c| matches!(c.kind, ClipKind::Text));
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
                    text_options,
                ));

                let scale_val = (text_options.scale_percent / 100.0).clamp(0.01, 10.0);
                let h_f64 = height as f64;
                let pos_y = if h_f64 > 0.0 {
                    text_options.position_y / h_f64
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

    tracks_json.sort_by_key(|t| match t.get("type").and_then(|v| v.as_str()) {
        Some("video") => 0,
        Some("text") => 1,
        _ => 2,
    });
    for (idx, track) in tracks_json.iter_mut().enumerate() {
        if let Some(segs) = track.get_mut("segments").and_then(|v| v.as_array_mut()) {
            for seg in segs {
                seg["track_render_index"] = json!(idx);
            }
        }
    }

    let draft_fold_path_str = draft_dir.to_string_lossy().replace('\\', "/");
    let draft_root_path_str = target_base_dir.to_string_lossy().replace('\\', "/");
    let (detected_platform, detected_new_ver) = detect_capcut_device_platform(&target_base_dir);

    let mut total_materials_size: u64 = project
        .media
        .iter()
        .filter_map(|item| std::fs::metadata(&item.path).ok())
        .map(|meta| meta.len())
        .sum();
    if total_materials_size == 0 {
        total_materials_size = 10_000_000;
    }

    let mut meta_materials_videos: Vec<Value> = Vec::new();
    for item in &project.media {
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

    let meta_path = draft_dir.join("draft_meta_info.json");
    let content_path = draft_dir.join("draft_content.json");
    let cover_path = draft_dir.join("draft_cover.jpg");
    write_draft_cover(project, project_dir, &cover_path);

    std::fs::write(
        &meta_path,
        serde_json::to_string_pretty(&meta_info).unwrap_or_default(),
    )
    .map_err(|e| format!("Could not write {}: {e}", meta_path.display()))?;
    let content_json_str = serde_json::to_string(&draft_content).unwrap_or_default();
    std::fs::write(&content_path, &content_json_str)
        .map_err(|e| format!("Could not write {}: {e}", content_path.display()))?;
    let _ = std::fs::write(draft_dir.join("draft_content.json.bak"), &content_json_str);
    let _ = std::fs::write(draft_dir.join("template-2.tmp"), &content_json_str);

    write_modern_timeline_files(
        &draft_dir,
        &main_tl_id,
        &cover_path,
        &content_json_str,
        &tracks_json,
        now_us,
    );

    if in_capcut_app {
        update_root_meta_info(
            &target_base_dir,
            &draft_folder_name,
            &draft_fold_path_str,
            &draft_root_path_str,
            &draft_id,
            total_materials_size,
            total_duration_us,
            now_us,
        );
    }

    Ok(CapCutExportResult {
        draft_dir,
        in_capcut_app,
        has_unbaked_effects,
    })
}

fn extract_if_zip(input_path: &Path, dirs: &AppDirs) -> Option<PathBuf> {
    if !input_path.is_file()
        || !input_path
            .extension()
            .is_some_and(|ext| ext.eq_ignore_ascii_case("zip"))
    {
        return None;
    }
    let stem = input_path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("capcut_draft");
    let dest = dirs.data.join("CapCut").join(stem);
    let file = std::fs::File::open(input_path).ok()?;
    let mut archive = zip::ZipArchive::new(file).ok()?;
    let _ = std::fs::create_dir_all(&dest);
    for i in 0..archive.len() {
        if let Ok(mut f) = archive.by_index(i)
            && let Some(enclosed) = f.enclosed_name()
        {
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
    Some(dest)
}

fn resolve_draft_file(target_path: &Path) -> Result<(PathBuf, PathBuf), String> {
    let cand_names = [
        "draft_content.json",
        "draft_info.json",
        "draft_meta_info.json",
        "template_content.json",
        "root_meta_info.json",
    ];
    let (draft_file, draft_dir) = if target_path.is_dir() {
        let mut found = cand_names.iter().find_map(|name| {
            let p = target_path.join(name);
            p.is_file().then(|| (p, target_path.to_path_buf()))
        });
        if found.is_none()
            && let Ok(entries) = std::fs::read_dir(target_path)
        {
            for entry in entries.flatten() {
                let sub = entry.path();
                if sub.is_dir()
                    && let Some(hit) = cand_names.iter().find_map(|name| {
                        let p = sub.join(name);
                        p.is_file().then(|| (p, sub.clone()))
                    })
                {
                    found = Some(hit);
                    break;
                }
            }
        }
        found.unwrap_or_else(|| {
            (
                target_path.join("draft_content.json"),
                target_path.to_path_buf(),
            )
        })
    } else if target_path.is_file() {
        let parent = target_path.parent().unwrap_or(Path::new(".")).to_path_buf();
        (target_path.to_path_buf(), parent)
    } else {
        return Err(format!(
            "Invalid CapCut draft path: {}",
            target_path.display()
        ));
    };

    let draft_file = if draft_file.is_file() {
        draft_file
    } else {
        let mut found = [
            draft_dir.join("draft_content.json"),
            draft_dir.join("draft_info.json"),
            draft_dir.join("draft_meta_info.json"),
        ]
        .into_iter()
        .find(|p| p.is_file());

        if found.is_none() {
            let project_json = draft_dir.join("Timelines").join("project.json");
            if project_json.is_file()
                && let Ok(raw) = std::fs::read_to_string(&project_json)
                && let Ok(pj) = serde_json::from_str::<Value>(&raw)
                && let Some(main_id) = pj["main_timeline_id"].as_str()
            {
                let c1 = draft_dir
                    .join("Timelines")
                    .join(main_id)
                    .join("draft_content.json");
                let c2 = draft_dir
                    .join("Timelines")
                    .join(main_id)
                    .join("draft_info.json");
                if c1.is_file() {
                    found = Some(c1);
                } else if c2.is_file() {
                    found = Some(c2);
                }
            }
        }
        if found.is_none()
            && let Ok(entries) = std::fs::read_dir(draft_dir.join("Timelines"))
        {
            for e in entries.flatten() {
                let c1 = e.path().join("draft_content.json");
                let c2 = e.path().join("draft_info.json");
                if c1.is_file() {
                    found = Some(c1);
                    break;
                } else if c2.is_file() {
                    found = Some(c2);
                    break;
                }
            }
        }
        found.unwrap_or(draft_file)
    };

    if !draft_file.is_file() {
        return Err(format!(
            "Could not find draft_content.json or draft_info.json in {}",
            draft_dir.display()
        ));
    }
    Ok((draft_file, draft_dir))
}

fn detect_draft_title(draft: &Value, draft_dir: &Path) -> String {
    if let Some(name) = draft["draft_name"]
        .as_str()
        .or_else(|| draft["name"].as_str())
        .filter(|s| !s.trim().is_empty())
    {
        return name.trim().to_string();
    }
    for meta_name in ["draft_meta_info.json", "draft_info.json"] {
        let meta_file = draft_dir.join(meta_name);
        if meta_file.is_file()
            && let Ok(meta_raw) = std::fs::read_to_string(&meta_file)
            && let Ok(meta_val) = serde_json::from_str::<Value>(&meta_raw)
            && let Some(name) = meta_val["draft_name"]
                .as_str()
                .or_else(|| meta_val["name"].as_str())
                .filter(|s| !s.trim().is_empty())
        {
            return name.trim().to_string();
        }
    }
    draft_dir
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("CapCut Draft")
        .to_string()
}

fn read_canvas_and_fps(draft: &Value) -> (u32, u32, f64, i64, i64) {
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
        match ratio {
            "9:16" => {
                width = 1080;
                height = 1920;
            }
            "1:1" => {
                width = 1080;
                height = 1080;
            }
            "4:3" => {
                width = 1440;
                height = 1080;
            }
            _ => {
                width = 1920;
                height = 1080;
            }
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
    (width, height, fps, rate_num, rate_den)
}

fn resolve_draft_media_path(draft_dir: &Path, p: &str) -> Option<String> {
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
    (!p.is_empty()).then_some(p_norm)
}

type MediaMap = HashMap<String, (String, bool, String)>;

fn parse_materials(
    draft: &Value,
    draft_dir: &Path,
    width: u32,
    height: u32,
    fps: f64,
    rate_num: i64,
    rate_den: i64,
) -> (Vec<MediaItem>, MediaMap) {
    let materials = &draft["materials"];
    let mut media_list: Vec<MediaItem> = Vec::new();
    let mut media_map: MediaMap = HashMap::new();
    let mut media_counter = 1usize;

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
            let real_path = resolve_draft_media_path(draft_dir, raw_path)
                .unwrap_or_else(|| raw_path.to_string());
            let name = v["material_name"]
                .as_str()
                .map(|s| s.to_string())
                .unwrap_or_else(|| {
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
            let media_item = if exists && let Ok(summary) = crate::media::probe(&real_path) {
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
                    kind: if is_photo {
                        MediaKind::Image
                    } else {
                        MediaKind::Video
                    },
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
            let real_path = resolve_draft_media_path(draft_dir, raw_path)
                .unwrap_or_else(|| raw_path.to_string());
            let name = img["material_name"]
                .as_str()
                .map(|s| s.to_string())
                .unwrap_or_else(|| {
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
            let media_item = if exists && let Ok(summary) = crate::media::probe(&real_path) {
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

    if let Some(audios) = materials["audios"].as_array() {
        for a in audios {
            let capcut_id = a["id"].as_str().unwrap_or_default().to_string();
            let raw_path = a["path"]
                .as_str()
                .or_else(|| a["extra_info"].as_str())
                .or_else(|| a["file_path"].as_str())
                .or_else(|| a["name"].as_str())
                .unwrap_or_default();
            let real_path = resolve_draft_media_path(draft_dir, raw_path)
                .unwrap_or_else(|| raw_path.to_string());
            let name = a["name"]
                .as_str()
                .map(|s| s.to_string())
                .unwrap_or_else(|| {
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
            let media_item = if exists && let Ok(summary) = crate::media::probe(&real_path) {
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

    (media_list, media_map)
}

type TextMap = HashMap<String, (String, TextStyle)>;

fn parse_text_materials(draft: &Value) -> (TextMap, HashMap<String, String>) {
    let materials = &draft["materials"];
    let mut text_map: TextMap = HashMap::new();

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
                t["text"].as_str().unwrap_or("Subtitle").to_string()
            };
            let styles = parsed["styles"].as_array();
            let first_style = styles.and_then(|s| s.first());

            let capcut_size = first_style
                .and_then(|s| s["size"].as_f64())
                .or_else(|| t["font_size"].as_f64())
                .unwrap_or(12.0);
            let font_size = (capcut_size / 250.0).clamp(0.02, 0.25);

            let color = if let Some(rgb) =
                first_style.and_then(|s| s["fill"]["content"]["solid"]["color"].as_array())
            {
                let r = (rgb.first().and_then(|v| v.as_f64()).unwrap_or(1.0) * 255.0).round() as u8;
                let g = (rgb.get(1).and_then(|v| v.as_f64()).unwrap_or(1.0) * 255.0).round() as u8;
                let b = (rgb.get(2).and_then(|v| v.as_f64()).unwrap_or(1.0) * 255.0).round() as u8;
                format!("#{r:02x}{g:02x}{b:02x}")
            } else if let Some(tc) = t["text_color"].as_str().filter(|s| !s.is_empty()) {
                tc.to_string()
            } else {
                "#ffffff".to_string()
            };

            let font_family = t["font_title"]
                .as_str()
                .or_else(|| t["font_name"].as_str())
                .filter(|s| *s != "none" && !s.is_empty())
                .unwrap_or("Hanken Grotesk")
                .to_string();

            let bold_width = t["bold_width"].as_f64().unwrap_or(0.0);
            let font_weight = if bold_width > 0.001 { 700.0 } else { 400.0 };
            let italic = t["italic_degree"].as_i64().unwrap_or(0) > 0;
            let underline = t["underline"].as_bool().unwrap_or(false);

            let mut stroke_width = 0.0;
            let mut stroke_color = "#000000".to_string();
            if let Some(strokes) = first_style
                .and_then(|s| s["strokes"].as_array())
                .and_then(|s| s.first())
            {
                let w = strokes["width"].as_f64().unwrap_or(0.0);
                stroke_width = (w / 0.24).clamp(0.0, 1.0);
                if let Some(rgb) = strokes["content"]["solid"]["color"].as_array() {
                    let r =
                        (rgb.first().and_then(|v| v.as_f64()).unwrap_or(0.0) * 255.0).round() as u8;
                    let g =
                        (rgb.get(1).and_then(|v| v.as_f64()).unwrap_or(0.0) * 255.0).round() as u8;
                    let b =
                        (rgb.get(2).and_then(|v| v.as_f64()).unwrap_or(0.0) * 255.0).round() as u8;
                    stroke_color = format!("#{r:02x}{g:02x}{b:02x}");
                }
            } else if let Some(bw) = t["border_width"].as_f64().filter(|&w| w > 0.0) {
                stroke_width = (bw / 0.24).clamp(0.0, 1.0);
                if let Some(bc) = t["border_color"].as_str().filter(|s| !s.is_empty()) {
                    stroke_color = bc.to_string();
                }
            }

            let align = match t["alignment"].as_i64().unwrap_or(1) {
                0 => TextAlign::Left,
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

    let mut template_to_text: HashMap<String, String> = HashMap::new();
    if let Some(templates) = materials["text_templates"].as_array() {
        for tmpl in templates {
            let tmpl_id = tmpl["id"].as_str().unwrap_or_default().to_string();
            if let Some(res) = tmpl["text_info_resources"]
                .as_array()
                .and_then(|r| r.first())
                && let Some(text_id) = res["text_material_id"].as_str()
            {
                template_to_text.insert(tmpl_id, text_id.to_string());
            }
        }
    }
    (text_map, template_to_text)
}

fn parse_tracks(
    draft: &Value,
    media_map: &MediaMap,
    text_map: &TextMap,
    template_to_text: &HashMap<String, String>,
) -> (Vec<Track>, Vec<Arc<Clip>>) {
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
                    let start =
                        seg["target_timerange"]["start"].as_i64().unwrap_or(0) as f64 / 1_000_000.0;
                    let duration = (seg["target_timerange"]["duration"]
                        .as_i64()
                        .unwrap_or(1_000_000) as f64
                        / 1_000_000.0)
                        .max(0.05);
                    let source_start = seg
                        .get("source_timerange")
                        .and_then(|st| st["start"].as_i64())
                        .unwrap_or(0) as f64
                        / 1_000_000.0;
                    let speed = seg["speed"].as_f64().unwrap_or(1.0);
                    let volume = seg["volume"].as_f64().unwrap_or(1.0);

                    let clip_meta = seg.get("clip");
                    let opacity = clip_meta.and_then(|c| c["alpha"].as_f64()).unwrap_or(1.0);
                    let rotation = clip_meta
                        .and_then(|c| c["rotation"].as_f64())
                        .unwrap_or(0.0);
                    let scale = clip_meta
                        .and_then(|c| c["scale"]["x"].as_f64())
                        .unwrap_or(1.0);
                    let tx = clip_meta
                        .and_then(|c| c["transform"]["x"].as_f64())
                        .unwrap_or(0.0);
                    let ty = clip_meta
                        .and_then(|c| c["transform"]["y"].as_f64())
                        .unwrap_or(0.0);
                    let flip_h = clip_meta
                        .and_then(|c| c["flip"]["horizontal"].as_bool())
                        .unwrap_or(false);
                    let flip_v = clip_meta
                        .and_then(|c| c["flip"]["vertical"].as_bool())
                        .unwrap_or(false);

                    let offset_x = tx / 2.0;
                    let offset_y = -ty / 2.0;
                    let clip_id = format!("c{clip_counter}");
                    clip_counter += 1;

                    if track_type == "text" {
                        let resolved_text = text_map.get(mat_id).or_else(|| {
                            template_to_text
                                .get(mat_id)
                                .and_then(|ref_id| text_map.get(ref_id.as_str()))
                        });
                        let (text_body, text_style) = match resolved_text {
                            Some((body, style)) => (body.clone(), style.clone()),
                            None => ("Subtitle".to_string(), TextStyle::default()),
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
                        let mut clip =
                            Clip::blank(&clip_id, &track_id, kind, name, start, duration);
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
                        let kind = if track_type == "audio" {
                            ClipKind::Audio
                        } else {
                            ClipKind::Video
                        };
                        let mut clip = Clip::blank(
                            &clip_id,
                            &track_id,
                            kind,
                            format!("Clip {clip_counter}"),
                            start,
                            duration,
                        );
                        clip.media_id = format!("m_fallback_{mat_id}");
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
    (tracks, clips)
}

fn build_text_material(
    id: &str,
    text: &str,
    style_opt: Option<&TextStyle>,
    prefs: &CapCutTextOptions,
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

    let has_stroke = style.stroke_width > 0.0 || prefs.stroke_enabled;
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
    let bold_width = if style.font_weight >= 600.0 {
        0.008
    } else {
        0.0
    };
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
    m.insert(
        "border_alpha".into(),
        json!(if has_stroke { 1.0 } else { 0.0 }),
    );
    m.insert(
        "border_color".into(),
        json!(if has_stroke { &prefs.stroke_color } else { "" }),
    );
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

fn detect_capcut_device_platform(target_base_dir: &Path) -> (Value, String) {
    let mut candidate_dirs = vec![target_base_dir.to_path_buf()];
    if let Some(capcut_dir) = default_capcut_drafts_dir()
        && capcut_dir != target_base_dir
    {
        candidate_dirs.push(capcut_dir);
    }
    for base in candidate_dirs {
        if let Ok(entries) = std::fs::read_dir(&base) {
            for entry in entries.flatten() {
                let candidate = entry.path().join("draft_content.json");
                if candidate.is_file()
                    && let Ok(raw) = std::fs::read_to_string(&candidate)
                    && let Ok(val) = serde_json::from_str::<Value>(&raw)
                    && let Some(p) = val.get("platform").cloned()
                    && (p.get("device_id").is_some() || p.get("mac_address").is_some())
                {
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

fn write_draft_cover(project: &Project, project_dir: &Path, cover_path: &Path) {
    let mut cover_written = false;
    let proj_dir = project_dir.to_str().unwrap_or("");
    if !proj_dir.is_empty()
        && let Ok(bytes) = crate::media::poster_frame(proj_dir)
        && !bytes.is_empty()
    {
        let _ = std::fs::write(cover_path, bytes);
        cover_written = true;
    }
    if !cover_written {
        for item in &project.media {
            let p = Path::new(&item.path);
            let ext = p
                .extension()
                .and_then(|e| e.to_str())
                .unwrap_or("")
                .to_lowercase();
            if ext == "jpg" || ext == "jpeg" {
                if std::fs::copy(p, cover_path).is_ok() {
                    cover_written = true;
                    break;
                }
            } else if matches!(item.kind, MediaKind::Video | MediaKind::Image)
                && p.exists()
                && let Ok(frame) = crate::media::still_at(&item.path, 0.0, 480)
                && let Ok(bytes) = concat_media::jpeg(&frame, 4)
            {
                let _ = std::fs::write(cover_path, bytes);
                cover_written = true;
                break;
            }
        }
    }
    if !cover_written {
        let _ = std::fs::write(cover_path, MINIMAL_COVER_JPEG);
    }
}

fn write_modern_timeline_files(
    draft_dir: &Path,
    main_tl_id: &str,
    cover_path: &Path,
    content_json_str: &str,
    tracks_json: &[Value],
    now_us: i64,
) {
    let timelines_dir = draft_dir.join("Timelines");
    let main_tl_dir = timelines_dir.join(main_tl_id);
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
                "name": "Timeline 01",
                "update_time": now_us
            }
        ],
        "update_time": now_us,
        "version": 0
    });
    let proj_json_str = serde_json::to_string(&project_json).unwrap_or_default();
    let _ = std::fs::write(timelines_dir.join("project.json"), &proj_json_str);
    let _ = std::fs::write(timelines_dir.join("project.json.bak"), &proj_json_str);

    let _ = std::fs::write(main_tl_dir.join("draft_content.json"), content_json_str);
    let _ = std::fs::write(main_tl_dir.join("draft_content.json.bak"), content_json_str);
    let _ = std::fs::write(main_tl_dir.join("template-2.tmp"), content_json_str);
    if cover_path.is_file() {
        let _ = std::fs::copy(cover_path, main_tl_dir.join("draft_cover.jpg"));
    }

    let _ = std::fs::write(
        draft_dir.join("draft_agency_config.json"),
        r#"{"is_auto_agency_enabled":false,"is_auto_agency_popup":false,"is_single_agency_mode":false,"marterials":null,"use_converter":false,"video_resolution":720}"#,
    );
    let _ = std::fs::write(draft_dir.join("draft_biz_config.json"), "");

    let attachment_pc_common_str = r#"{"ai_packaging_infos":[],"ai_packaging_report_info":{"caption_id_list":[],"commercial_material":"","material_source":"","method":"","page_from":"","style":"","task_id":"","text_style":"","tos_id":"","video_category":""},"broll":{"ai_packaging_infos":[],"ai_packaging_report_info":{"caption_id_list":[],"commercial_material":"","material_source":"","method":"","page_from":"","style":"","task_id":"","text_style":"","tos_id":"","video_category":""}},"commercial_music_category_ids":[],"pc_feature_flag":0,"recognize_tasks":[],"reference_lines_config":{"horizontal_lines":[],"is_lock":false,"is_visible":false,"vertical_lines":[]},"safe_area_type":0,"template_item_infos":[],"unlock_template_ids":[]}"#;
    let _ = std::fs::write(
        draft_dir.join("attachment_pc_common.json"),
        attachment_pc_common_str,
    );
    let _ = std::fs::write(
        main_tl_dir.join("attachment_pc_common.json"),
        attachment_pc_common_str,
    );

    let attachment_editing_str = r#"{"editing_draft":{"ai_remove_filter_words":{"enter_source":"","right_id":""},"ai_shorts_info":{"report_params":"","type":0},"cover_extra_info":{"draft_id":"","position":0,"select_segment_id":"","select_segment_source_start":0,"select_segment_target_start":0,"slot_image_path":"","slot_info_config":{"slot_image_path":"","used_video_algorithm_configs":[]},"type":1,"video_draft_source":-1},"crop_info_extra":{"crop_mirror_type":0,"crop_rotate":0.0,"crop_rotate_total":0.0},"cutsame_to_image_edit_template_id":"","digital_human_template_to_video_info":{"has_upload_material":false,"template_type":0},"draft_used_recommend_function":"","edit_type":0,"eye_correct_enabled_multi_face_time":0,"has_adjusted_render_layer":false,"image_ai_chat_info":{"before_chat_edit":false,"draft_modify_time":0,"generate_type":"","inspiration_item_id":"","inspiration_item_name":"","keyword_content":"","keyword_id":"","keyword_name":"","keyword_type":"","message_id":"","model_name":"","need_restore":false,"picture_id":"","prompt_content":"","prompt_from":"","skill_ids":"","sugs_info":[]},"image_ai_template_info":{"first_draw_type":"","inspiration_id":"","item_type":"","request_id":""},"is_open_expand_player":false,"is_template_text_ai_generate":false,"is_use_adjust":false,"is_use_ai_expand":false,"is_use_ai_image":false,"is_use_ai_remove":false,"is_use_ai_video":false,"is_use_audio_separation":false,"is_use_chroma_key":false,"is_use_curve_speed":false,"is_use_digital_human":false,"is_use_edit_multi_camera":false,"is_use_lip_sync":false,"is_use_lock_object":false,"is_use_loudness_unify":false,"is_use_noise_reduction":false,"is_use_one_click_beauty":false,"is_use_one_click_ultra_hd":false,"is_use_retouch_face":false,"is_use_smart_adjust_color":false,"is_use_smart_body_beautify":false,"is_use_smart_motion":false,"is_use_subtitle_recognition":false,"is_use_text_to_audio":false,"material_edit_session":{"material_edit_info":[],"session_id":"","session_time":0},"paste_segment_list":[],"profile_entrance_type":"","publish_enter_from":"","publish_type":"","single_function_type":0,"text_convert_case_types":[],"version":"1.0.0","video_recording_create_draft":"","video_to_image_edit":{"enter_from":"","image":{"draft_id":"","expected_version":"","generation_id":"","height":0,"index":0,"model_id":"","model_name":"","origin_path":"","path":"","picture_id":"","skill_id":"","status":0,"width":0},"message_id":"","output_type":0,"source_duration":0,"source_vid":""},"video_to_image_info":{"create_time":0,"enter_from":"","error_code":0,"finish_time":0,"images":[],"log_id":"","message_id":"","origin_request_finish_time":0,"origin_request_id":"","output_type":0,"request_id":"","source_cover_path":"","source_duration":0,"source_local_identifier":"","source_vid":"","source_video_path":"","status":0,"title":""}}}"#;
    let _ = std::fs::write(
        draft_dir.join("attachment_editing.json"),
        attachment_editing_str,
    );
    let _ = std::fs::write(
        main_tl_dir.join("attachment_editing.json"),
        attachment_editing_str,
    );
    let _ = std::fs::write(
        draft_dir.join("draft_virtual_store.json"),
        r#"{"draft_materials":[],"draft_virtual_store":[{"type":0,"value":[{"creation_time":0,"display_name":"","filter_type":0,"id":"","import_time":0,"import_time_us":0,"material_color_tag":"","sort_sub_type":0,"sort_type":0,"subdraft_filter_type":0}]},{"type":1,"value":[]},{"type":2,"value":[]}]}"#,
    );
    let _ = std::fs::write(draft_dir.join("key_value.json"), "{}");
    let _ = std::fs::write(
        draft_dir.join("performance_opt_info.json"),
        r#"{"opt_info":{"is_opt":false,"opt_type":0}}"#,
    );

    let mut seg_ids: Vec<String> = Vec::new();
    for track in tracks_json {
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
        .map(|(i, sid)| json!({ "short_id": format!("{}", 1000 + i), "uuid": sid }))
        .collect();
    let id_mapping_str = serde_json::to_string(&json!({
        "id_mapping": {
            "mapping": mapping,
            "next_index": 1000 + seg_ids.len(),
            "version": "1.0.0"
        }
    }))
    .unwrap_or_default();

    for ca_dir in [
        draft_dir.join("common_attachment"),
        main_tl_dir.join("common_attachment"),
    ] {
        let _ = std::fs::create_dir_all(&ca_dir);
        let _ = std::fs::write(ca_dir.join("attachment_id_mapping.json"), &id_mapping_str);
        let _ = std::fs::write(
            ca_dir.join("attachment_action_scene.json"),
            r#"{"action_scene":{"removed_segments":[],"segment_infos":[]}}"#,
        );
        let _ = std::fs::write(
            ca_dir.join("attachment_gen_ai_info.json"),
            r#"{"gen_ai":{"ai_func_config":{"ai_common_configs":[],"ai_effect_configs":[],"ai_func_list":[],"aigc_generation_configs":[]},"cc_agent_info":{"agent_stringent_section_id_list":[],"agent_stringent_used_tool_list":[],"click_cnt":0,"consume_credits_function_list":[],"conversation_ids":[],"generate_success_cnt":0,"is_agent_stringent_used":false,"is_agent_used":false,"local_section_id_list":[],"real_skill_list":[],"request_cnt":0,"request_from":[],"tool_list":[],"user_select_skill_list":[]},"id":"","scene":"","version":"1.0.0"}}"#,
        );
        let _ = std::fs::write(
            ca_dir.join("attachment_pc_timeline.json"),
            r#"{"reference_lines_config":{"horizontal_lines":[],"is_lock":false,"is_visible":false,"vertical_lines":[]},"safe_area_type":0}"#,
        );
        let _ = std::fs::write(
            ca_dir.join("attachment_plugin_draft.json"),
            r#"{"plugin_draft":{"plugin_segments":[],"version":"1.0.0"}}"#,
        );
        let _ = std::fs::write(
            ca_dir.join("attachment_script_video.json"),
            r#"{"script_video":{"attachment_valid":false,"language":"","overdub_recover":[],"overdub_sentence_ids":[],"parts":[],"sync_subtitle":false,"translate_segments":[],"translate_type":"","version":"1.0.0"}}"#,
        );
    }

    let timeline_layout_json = json!({
        "dockItems": [{ "dockIndex": 0, "ratio": 1, "timelineIds": [main_tl_id], "timelineNames": ["Timeline 01"] }],
        "layoutOrientation": 1
    });
    let _ = std::fs::write(
        draft_dir.join("timeline_layout.json"),
        serde_json::to_string(&timeline_layout_json).unwrap_or_default(),
    );
}

#[allow(clippy::too_many_arguments)]
fn update_root_meta_info(
    target_base_dir: &Path,
    draft_folder_name: &str,
    draft_fold_path_str: &str,
    draft_root_path_str: &str,
    draft_id: &str,
    total_materials_size: u64,
    total_duration_us: i64,
    now_us: i64,
) {
    let root_meta_file =
        capcut_root_meta_file().unwrap_or_else(|| target_base_dir.join("root_meta_info.json"));
    if !root_meta_file.is_file() {
        return;
    }
    let Ok(root_raw) = std::fs::read_to_string(&root_meta_file) else {
        return;
    };
    let Ok(mut root_val) = serde_json::from_str::<Value>(&root_raw) else {
        return;
    };
    let Some(store) = root_val
        .get_mut("all_draft_store")
        .and_then(|v| v.as_array_mut())
    else {
        return;
    };
    store.retain(|item| {
        let name_match = item.get("draft_name").and_then(|v| v.as_str()) == Some(draft_folder_name);
        let path_match =
            item.get("draft_fold_path").and_then(|v| v.as_str()) == Some(draft_fold_path_str);
        let id_match = item.get("draft_id").and_then(|v| v.as_str()) == Some(draft_id);
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
    let _ = std::fs::write(
        &root_meta_file,
        serde_json::to_string(&root_val).unwrap_or_default(),
    );
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

#[cfg(test)]
mod tests {
    use super::*;
    use concat_project::Command;

    #[test]
    fn capcut_export_and_import_round_trip() {
        let scratch = tempfile::tempdir().expect("scratch");
        let dirs = AppDirs {
            config: scratch.path().join("config"),
            data: scratch.path().join("data"),
        };
        let proj_dir = scratch.path().join("SourceProj");
        let info = projects::create(
            scratch.path().to_str().expect("utf8"),
            "SourceProj",
            1920,
            1080,
            30,
            1,
            [0, 0, 0, 255],
        )
        .expect("create");
        let mut session = crate::Session::open_info(&info).expect("open");
        session
            .apply(Command::AddTextClip {
                above: false,
                track_id: None,
                start: 1.0,
                style: Some(TextStyle {
                    content: "Hello CapCut".to_owned(),
                    color: "#00ff88".to_owned(),
                    ..TextStyle::default()
                }),
                duration: Some(2.5),
                offset_y: None,
            })
            .expect("add text");

        let drafts_out = scratch.path().join("DraftsOut");
        let exported = export_project_to_capcut(
            session.project(),
            &proj_dir,
            "RoundTrip",
            &CapCutTextOptions::default(),
            Some(&drafts_out),
            &dirs,
        )
        .expect("export capcut");
        assert!(exported.draft_dir.join("draft_content.json").is_file());
        assert!(exported.draft_dir.join("draft_meta_info.json").is_file());

        let dest = scratch.path().join("Reimported");
        let imported =
            import_capcut_draft(&exported.draft_dir, &dest, Some("MyDraft"), false, &dirs)
                .expect("import capcut");
        assert_eq!(imported.track_count, 1);
        assert_eq!(imported.clip_count, 1);
        let reopened = crate::Session::open_info(&imported.project).expect("reopen imported");
        let clips = &reopened.project().active().clips;
        assert_eq!(clips.len(), 1);
        assert_eq!(
            clips[0].text.as_ref().map(|t| t.content.as_str()),
            Some("Hello CapCut")
        );
        let _ = info;
    }
}
