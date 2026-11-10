// SPDX-License-Identifier: AGPL-3.0-or-later
//! Google Flow's native sheet. No paid request starts without the explicit credit toggle.
use crate::{
    host::spawn_flow_in_project as spawn_in_project,
    panes::{Msg, media_bin::MediaMsg},
    studio::Studio,
    ui::{FlowData, FlowFile},
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::path::PathBuf;
#[path = "flow_cast.rs"]
mod cast_state;
use cast_state as flow_cast;
#[path = "flow_scene.rs"]
mod scene_state;
#[path = "flow_visual.rs"]
mod visual_state;
#[path = "flow_post.rs"]
mod post_process;
use scene_state::ScenePlan;
use visual_state::ReferenceImage;

#[derive(Clone, Copy, Debug)]
pub enum ImageTarget {
    Character(usize),
    Background(usize),
    BackgroundSheet(usize),
    Sheet(usize),
    Scene(usize),
    SceneAngles(usize),
    Preview(usize),
}

#[derive(Debug)]
/// UI actions and completions of the Flow worker.
pub enum FlowMsg {
    QueueTick,
    QueuedFinished(u64, usize, String, Result<Value, String>),
    /// Action id, row/index and text payload from the native sheet.
    Action(i32, i32, String),
    /// Completion of one serialized worker request.
    Finished(u64, Result<Value, String>),
    /// A Gemini editorial draft, separate from video worker results.
    AiFinished(u64, i32, Result<Value, String>),
    /// A locally saved Gemini reference image for a profile or location.
    ImageFinished(u64, bool, usize, Result<PathBuf, String>),
    VisualFinished(u64, ImageTarget, Result<Option<PathBuf>, String>),
    /// Native image picker result, guarded against changes to the profile list.
    ReferencePicked(u64, usize, Vec<String>, PathBuf),
    /// A read-only snapshot of the selected Flow project's character library.
    CharactersListed(u64, String, Result<Value, String>),
    /// Profile and portrait downloaded for one explicitly selected character.
    CharacterImported(u64, String, String, Result<Value, String>),
}
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
struct FlowCharacterOrigin {
    #[serde(default = "primary_account")]
    account_id: String,
    project_id: String,
    id: String,
    voice: String,
    portrait_media_id: String,
}
#[derive(Clone, Deserialize)]
struct FlowLibraryCharacter {
    project_id: String,
    id: String,
    name: String,
    voice: String,
    personality: String,
    portrait_media_id: String,
    has_portrait: bool,
}
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct CharacterPreset {
    pub id: String,
    pub name: String,
    pub role: String,
    pub profile: String,
    pub image_path: String,
    #[serde(default)]
    pub flow_id: String,
    #[serde(default)]
    pub flow_origin: Option<FlowCharacterOrigin>,
}

impl CharacterPreset {
    pub fn list(root: &std::path::Path) -> Vec<CharacterPreset> {
        let file = root.join("presets").join("characters.json");
        if let Ok(data) = std::fs::read_to_string(&file) {
            if let Ok(list) = serde_json::from_str::<Vec<CharacterPreset>>(&data) {
                return list;
            }
        }
        Vec::new()
    }

    pub fn save_all(root: &std::path::Path, list: &[CharacterPreset]) -> Result<(), String> {
        let dir = root.join("presets");
        let _ = std::fs::create_dir_all(&dir);
        let file = dir.join("characters.json");
        let json = serde_json::to_string_pretty(list).map_err(|e| e.to_string())?;
        std::fs::write(&file, json).map_err(|e| e.to_string())
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct ScenePreset {
    pub id: String,
    pub name: String,
    pub description: String,
    pub style: String,
    pub image_path: String,
}

impl ScenePreset {
    pub fn list(root: &std::path::Path) -> Vec<ScenePreset> {
        let file = root.join("presets").join("scenes.json");
        if let Ok(data) = std::fs::read_to_string(&file) {
            if let Ok(list) = serde_json::from_str::<Vec<ScenePreset>>(&data) {
                return list;
            }
        }
        Vec::new()
    }

    pub fn save_all(root: &std::path::Path, list: &[ScenePreset]) -> Result<(), String> {
        let dir = root.join("presets");
        let _ = std::fs::create_dir_all(&dir);
        let file = dir.join("scenes.json");
        let json = serde_json::to_string_pretty(list).map_err(|e| e.to_string())?;
        std::fs::write(&file, json).map_err(|e| e.to_string())
    }
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(default)]
struct Settings {
    source_account_id: String,
    url: String,
    prompt: String,
    reference: String,
    model: usize,
    duration: usize,
    ratio: usize,
    screenplay: String,
    continuity: String,
    style: String,
    scene: usize,
    characters: Vec<String>,
    character_ids: Vec<String>,
    character_roles: Vec<String>,
    scene_items: Vec<String>,
    brief: String,
    plans: Vec<ScenePlan>,
    backgrounds: Vec<String>,
    character_images: Vec<PathBuf>,
    character_galleries: std::collections::BTreeMap<String, Vec<ReferenceImage>>,
    character_flow_origins: Vec<FlowCharacterOrigin>,
    background_images: Vec<PathBuf>,
    image_model: String,
    image_provider: usize,
    video_provider: usize,
    flow_image_model: usize,
    flow_image_ratio: usize,
    writing_format: usize,
    writing_structure: usize,
    writing_scene_count: usize,
    writing_audience: String,
    writing_goal: String,
    logline: String,
    pub auto_import_timeline: bool,
}
impl Default for Settings {
    fn default() -> Self {
        Self {
            source_account_id: primary_account(),
            url: String::new(),
            prompt: String::new(),
            reference: String::new(),
            model: 0,
            duration: 5,
            ratio: 0,
            screenplay: String::new(),
            continuity: String::new(),
            style: String::new(),
            scene: 0,
            characters: Vec::new(),
            character_ids: Vec::new(),
            character_roles: Vec::new(),
            scene_items: Vec::new(),
            brief: String::new(),
            plans: Vec::new(),
            backgrounds: Vec::new(),
            character_images: Vec::new(),
            character_galleries: Default::default(),
            character_flow_origins: Vec::new(),
            background_images: Vec::new(),
            image_model: "gemini-2.5-flash-image".into(),
            image_provider: 0,
            video_provider: 0,
            flow_image_model: 0,
            flow_image_ratio: 0,
            writing_format: 0,
            writing_structure: 0,
            writing_scene_count: 4,
            writing_audience: String::new(),
            writing_goal: String::new(),
            logline: String::new(),
            auto_import_timeline: true,
        }
    }
}
#[derive(Default)]
/// The Flow sheet's project-scoped view state.
pub struct Flow {
    active_jobs: std::collections::BTreeMap<usize, String>,
    blocked_accounts: std::collections::HashSet<String>,
    account_cursor: usize,
    active_account: String,
    queue_failures: usize,
    /// Whether the sheet is currently visible.
    pub open: bool,
    settings: Settings,
    loaded: bool,
    busy: bool,
    credit: bool,
    message: String,
    files: Vec<PathBuf>,
    generation: u64,
    config_path: PathBuf,
    queue: std::collections::VecDeque<usize>,
    queue_running: bool,
    active_scene: Option<(usize, bool)>,
    background_edit: usize,
    transfer_path: String,
    images: std::cell::RefCell<std::collections::HashMap<PathBuf, slint::Image>>,
    remote_characters: Vec<FlowLibraryCharacter>,
    remote_project_url: String,
    remote_account_id: String,
    flow_link_target: String,
    copilot_prompt: String,
    copilot_collapsed: bool,
    ai_target_kind: i32,
    ai_sub_idx: usize,
}
// Flow no longer exposes the legacy Omni Flash model to new generations.
// Index 0 is deliberately the migration target for projects saved by builds
// that still offered it at index 2.
const MODELS: [&str; 4] = [
    "omni-flash-1.2",
    "veo-3.1-lite",
    "veo-3.1-fast",
    "veo-3.1-quality",
];
const IMAGE_MODELS: [&str; 3] = ["nano-banana-pro", "nano-banana-2", "nano-banana-2-lite"];
const IMAGE_RATIOS: [&str; 5] = ["1:1", "9:16", "16:9", "3:4", "4:3"];
fn primary_account() -> String {
    "primary".into()
}
fn normalize_model(saved: usize) -> usize {
    saved.min(MODELS.len() - 1)
}
impl Flow {
    /// Publishes the current form and locally saved output files.
    pub fn data(&self, studio: &Studio) -> FlowData {
        let plan = self
            .settings
            .plans
            .get(self.settings.scene)
            .cloned()
            .unwrap_or_default();
        let scene_status = plan.label().to_owned();
        let mut validation = Vec::new();
        if self
            .settings
            .scene_items
            .get(self.settings.scene)
            .is_none_or(|s| s.trim().is_empty())
        {
            validation.push("Chưa có nội dung cảnh.".to_owned());
        }
        if self.settings.prompt.trim().is_empty() {
            validation.push("Chưa có prompt cho cảnh này.".to_owned());
        }
        if plan.needs_prompt {
            validation.push("Nội dung đã đổi: cần ghép/cập nhật prompt.".to_owned());
        }
        if let Err(e) = self.scene_references(self.settings.scene, &self.settings.reference) {
            validation.push(e);
        }
        if let Err(e) = self.settings.validate_cast(self.settings.scene) {
            validation.push(e);
        }
        let files = self
            .files
            .iter()
            .map(|p| {
                let clean = crate::flow_worker::clean_path(p.clone());
                let thumb_path = clean.with_extension("thumb.jpg");
                let thumb_exists = thumb_path.is_file();
                let actual_thumb = if thumb_exists {
                    Some(thumb_path)
                } else {
                    post_process::extract_thumbnail(&clean)
                };
                let preview = actual_thumb
                    .and_then(|t| slint::Image::load_from_path(&t).ok())
                    .unwrap_or_default();
                let name = p
                    .file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .to_string();
                let status = if name.contains("_clean") && name.contains("_1080p") {
                    "1080p · Đã xóa logo"
                } else if name.contains("_clean") {
                    "Đã xóa logo · Sẵn sàng"
                } else if name.contains("_1080p") {
                    "Nét 1080p"
                } else if name.contains("_2K") {
                    "Nét 2K"
                } else {
                    "Gốc · 720p"
                };
                FlowFile {
                    name: name.into(),
                    path: p.to_string_lossy().as_ref().into(),
                    preview,
                    status: status.into(),
                }
            })
            .collect::<Vec<_>>();
        let labels = self
            .settings
            .character_labels()
            .into_iter()
            .enumerate()
            .map(|(i, label)| {
                if let Some(origin) = self
                    .settings
                    .character_flow_origins
                    .get(i)
                    .filter(|o| !o.id.is_empty())
                {
                    let id = if origin.account_id.is_empty() {
                        "primary"
                    } else {
                        origin.account_id.as_str()
                    };
                    let account = studio
                        .settings
                        .flow_accounts
                        .config
                        .accounts
                        .iter()
                        .find(|a| a.id == id)
                        .map(|a| a.name.as_str())
                        .unwrap_or("Tài khoản nguồn đã xóa");
                    format!("{label} · {account}")
                } else {
                    label
                }
            })
            .collect::<Vec<_>>();
        let mut running_accounts = crate::flow_worker::rendering_accounts();
        running_accounts.extend(self.active_jobs.values().cloned());
        if self.active_scene.is_some_and(|(_, paid)| paid) && !self.active_account.is_empty() {
            running_accounts.insert(self.active_account.clone());
        }
        FlowData {
            image_ready_message: if self.busy { "Đang xử lý tác vụ khác." }
                else if studio.settings.flow_accounts.config.get(&self.settings.source_account_id).is_err() { "Chọn tài khoản nguồn đang bật trong Cài đặt → Google Flow." }
                else { "Sẵn sàng. Mỗi lần bấm tạo gửi một ảnh; duyệt ảnh trước khi dùng cho cảnh/video." }.into(),
            character_galleries: slint::ModelRc::new(slint::VecModel::from((0..self.settings.characters.len()).map(|i| crate::ui::FlowGallery {
                items: self.visual_rows(self.gallery(i),self.settings.character_images.get(i)),
            }).collect::<Vec<_>>())),
            scene_images: self.visual_rows(&plan.image_history,Some(&plan.scene_image)),
            scene_image: self.asset(Some(&plan.scene_image)),
            scene_image_status: if plan.image_stale { "Ảnh cảnh đã cũ · cần tạo mới hoặc duyệt lại".into() } else { plan.image_status.as_str().into() },
            account_names: slint::ModelRc::new(slint::VecModel::from(
                studio
                    .settings
                    .flow_accounts
                    .config
                    .accounts
                    .iter()
                    .map(|a| {
                        let root = studio.host.dirs.data.join("google-flow");
                        let email = a
                            .detected_email(&root)
                            .filter(|e| !a.name.contains(e.as_str()))
                            .map(|e| format!(" ({e})"))
                            .unwrap_or_default();
                        slint::SharedString::from(format!(
                            "{}{}{}",
                            a.name,
                            email,
                            if a.enabled { "" } else { " · đang tắt" }
                        ))
                    })
                    .collect::<Vec<_>>(),
            )),
            scene_account_names: slint::ModelRc::new(slint::VecModel::from(
                std::iter::once(slint::SharedString::from("Tự phân bổ tài khoản đang bật"))
                    .chain(
                        studio
                            .settings
                            .flow_accounts
                            .config
                            .accounts
                            .iter()
                            .map(|a| {
                                let root = studio.host.dirs.data.join("google-flow");
                                let email = a
                                    .detected_email(&root)
                                    .filter(|e| !a.name.contains(e.as_str()))
                                    .map(|e| format!(" ({e})"))
                                    .unwrap_or_default();
                                slint::SharedString::from(format!(
                                    "{}{}{}",
                                    a.name,
                                    email,
                                    if a.enabled { "" } else { " · đang tắt" }
                                ))
                            }),
                    )
                    .collect::<Vec<_>>(),
            )),
            source_account: studio
                .settings
                .flow_accounts
                .config
                .accounts
                .iter()
                .position(|a| a.id == self.settings.source_account_id)
                .map_or(-1, |i| i as i32),
            scene_account: studio
                .settings
                .flow_accounts
                .config
                .accounts
                .iter()
                .position(|a| a.id == plan.account_id)
                .map_or(0, |i| i as i32 + 1),
            scene_project_url: plan.project_url.as_str().into(),
            account_summary: format!(
                "Giới hạn đã lưu: {} tài khoản · {} luồng. Đang chạy: {} luồng.",
                studio.settings.flow_accounts.config.parallel_accounts,
                studio.settings.flow_accounts.config.total_slots,
                running_accounts.len()
            )
            .into(),
            character_names: slint::ModelRc::new(slint::VecModel::from(
                std::iter::once(slint::SharedString::from("Chọn nhân vật / nhập mới"))
                    .chain(
                        labels
                            .iter()
                            .map(|name| slint::SharedString::from(name.as_str())),
                    )
                    .collect::<Vec<_>>(),
            )),
            character_roles: slint::ModelRc::new(slint::VecModel::from(
                self.settings
                    .character_roles
                    .iter()
                    .map(|role| slint::SharedString::from(role.as_str()))
                    .collect::<Vec<_>>(),
            )),
            dialogue_rows: slint::ModelRc::new(slint::VecModel::from(
                plan.dialogue_lines
                    .iter()
                    .map(|line| crate::ui::FlowDialogueRow {
                        speaker: self
                            .settings
                            .character_ids
                            .iter()
                            .position(|id| id == &line.character_id)
                            .map_or(0, |i| i as i32 + 1),
                        text: line.text.as_str().into(),
                    })
                    .collect::<Vec<_>>(),
            )),
            flow_link_target: self
                .settings
                .character_ids
                .iter()
                .position(|id| id == &self.flow_link_target)
                .map_or(0, |i| i as i32 + 1),
            open: self.open,
            busy: self.busy,
            credit: true,
            url: {
                let u = self.settings.url.trim();
                if !u.is_empty() {
                    u.into()
                } else {
                    self.effective_project_url(studio).as_str().into()
                }
            },
            prompt: self.settings.prompt.as_str().into(),
            reference: self.settings.reference.as_str().into(),
            model: self.settings.model as i32,
            duration: self.settings.duration as i32,
            ratio: self.settings.ratio as i32,
            message: self.message.as_str().into(),
            can_import: studio.session.is_some(),
            files: slint::ModelRc::new(slint::VecModel::from(files)),
            desktop: !cfg!(any(target_os = "android", target_os = "ios")),
            screenplay: self.settings.screenplay.as_str().into(),
            continuity: self.settings.continuity.as_str().into(),
            style: self.settings.style.as_str().into(),
            scene: self.settings.scene as i32,
            brief: self.settings.brief.as_str().into(),
            remote_characters: slint::ModelRc::new(slint::VecModel::from(
                self.remote_characters
                    .iter()
                    .map(|item| crate::ui::FlowRemoteCharacter {
                        name: item.name.as_str().into(),
                        detail: format!(
                            "{}{}",
                            if item.has_portrait {
                                "Có ảnh chân dung"
                            } else {
                                "Chỉ có hồ sơ"
                            },
                            if item.voice.is_empty() {
                                String::new()
                            } else {
                                format!(" · Mã giọng: {}", item.voice)
                            }
                        )
                        .into(),
                        imported: self.settings.character_flow_origins.iter().any(|origin| {
                            origin.id == item.id
                                && origin.project_id == item.project_id
                                && (origin.account_id == self.settings.source_account_id
                                    || (origin.account_id.is_empty()
                                        && self.settings.source_account_id == "primary"))
                        }),
                    })
                    .collect::<Vec<_>>(),
            )),
            character_presets: {
                let presets_root = studio.host.dirs.data.join("google-flow");
                let presets = CharacterPreset::list(&presets_root);
                slint::ModelRc::new(slint::VecModel::from(
                    presets
                        .into_iter()
                        .map(|p| {
                            let img_path = PathBuf::from(&p.image_path);
                            let asset = self.asset(if img_path.is_file() { Some(&img_path) } else { None });
                            let has_image = !asset.path.is_empty();
                            crate::ui::FlowCharacterPreset {
                                id: p.id.as_str().into(),
                                name: p.name.as_str().into(),
                                role: p.role.as_str().into(),
                                profile: p.profile.as_str().into(),
                                preview: asset.preview,
                                has_image,
                            }
                        })
                        .collect::<Vec<_>>(),
                ))
            },
            scene_presets: {
                let presets_root = studio.host.dirs.data.join("google-flow");
                let presets = ScenePreset::list(&presets_root);
                slint::ModelRc::new(slint::VecModel::from(
                    presets
                        .into_iter()
                        .map(|p| {
                            let img_path = PathBuf::from(&p.image_path);
                            let asset = self.asset(if img_path.is_file() { Some(&img_path) } else { None });
                            let has_image = !asset.path.is_empty();
                            crate::ui::FlowScenePreset {
                                id: p.id.as_str().into(),
                                name: p.name.as_str().into(),
                                description: p.description.as_str().into(),
                                style: p.style.as_str().into(),
                                preview: asset.preview,
                                has_image,
                            }
                        })
                        .collect::<Vec<_>>(),
                ))
            },
            writing_format: self.settings.writing_format.min(4) as i32,
            writing_structure: self.settings.writing_structure.min(3) as i32,
            writing_scene_count: self.settings.writing_scene_count.clamp(1, 12) as i32,
            writing_audience: self.settings.writing_audience.as_str().into(),
            writing_goal: self.settings.writing_goal.as_str().into(),
            logline: self.settings.logline.as_str().into(),
            camera: plan.camera.into(),
            shot: plan.shot.into(),
            movement: plan.movement.into(),
            sound: plan.sound.into(),
            dialogue: plan.dialogue.into(),
            lighting: plan.lighting.into(),
            cast_rows: slint::ModelRc::new(slint::VecModel::from(
                self.settings
                    .characters
                    .iter()
                    .enumerate()
                    .map(|(i, _profile)| {
                        let name = labels.get(i).cloned().unwrap_or_default();
                        let origin = self
                            .settings
                            .character_flow_origins
                            .get(i)
                            .filter(|o| !o.id.is_empty());
                        crate::ui::FlowCastRow {
                            action: plan
                                .character_actions
                                .get(&self.settings.character_ids[i])
                                .map(String::as_str)
                                .unwrap_or("")
                                .into(),
                            name: format!(
                                "{name}{}",
                                origin
                                    .map(|o| if o.voice.is_empty() {
                                        " · Gốc Flow"
                                    } else {
                                        " · Gốc Flow + giọng"
                                    })
                                    .unwrap_or("")
                            )
                            .into(),
                            selected: !plan.cast_configured || plan.cast.contains(&i),
                        }
                    })
                    .collect::<Vec<_>>(),
            )),
            backgrounds: slint::ModelRc::new(slint::VecModel::from(
                std::iter::once(slint::SharedString::from("Bối cảnh chung"))
                    .chain(self.settings.backgrounds.iter().enumerate().map(|(i, s)| {
                        slint::SharedString::from(format!(
                            "{} · {}",
                            i + 1,
                            s.lines()
                                .next()
                                .unwrap_or("")
                                .chars()
                                .take(45)
                                .collect::<String>()
                        ))
                    }))
                    .collect::<Vec<_>>(),
            )),
            background: plan.background.map(|i| i as i32 + 1).unwrap_or(0),
            background_edit: if self.background_edit >= self.settings.backgrounds.len() {
                0
            } else {
                self.background_edit as i32 + 1
            },
            background_text: self
                .settings
                .backgrounds
                .get(self.background_edit)
                .map(String::as_str)
                .unwrap_or("")
                .into(),
            character_images: slint::ModelRc::new(slint::VecModel::from(
                (0..self.settings.characters.len())
                    .map(|i| self.asset(self.settings.character_images.get(i)))
                    .collect::<Vec<_>>(),
            )),
            background_image: self.asset(self.settings.background_images.get(self.background_edit)),
            image_model: self.settings.image_model.as_str().into(),
            image_provider: self.settings.image_provider.min(1) as i32,
            video_provider: self.settings.video_provider.min(1) as i32,
            visual_mode: plan.visual_mode as i32,
            end_image: self.asset(Some(&plan.end_image)),
            scene_flow_auto: plan.scene_flow_auto,
            auto_import_timeline: self.settings.auto_import_timeline,
            copilot_prompt: self.copilot_prompt.as_str().into(),
            copilot_collapsed: self.copilot_collapsed,
            can_undo_copilot: !self.config_path.as_os_str().is_empty()
                && self.config_path.with_file_name("flow-before-copilot.json").exists(),
            flow_image_model: self.settings.flow_image_model.min(2) as i32,
            flow_image_ratio: self.settings.flow_image_ratio.min(4) as i32,
            transfer_path: self.transfer_path.as_str().into(),
            scene_status: scene_status.into(),
            validation: validation.join("\n").into(),
            queued: self.queue_running,
            queue_count: self.queue.len() as i32,
            scene_rows: slint::ModelRc::new(slint::VecModel::from(
                self.settings
                    .scene_items
                    .iter()
                    .enumerate()
                    .map(|(i, _)| {
                        let plan = self.settings.plans.get(i).cloned().unwrap_or_default();
                        crate::ui::FlowSceneRow {
                            name: format!("Cảnh {}", i + 1).into(),
                            status: plan.label().into(),
                            selected: plan.selected,
                        }
                    })
                    .collect::<Vec<_>>(),
            )),
            ai_ready: !concat_speech::api_keys::parse(studio.prefs.gemini_api_key.expose())
                .is_empty(),
            scene_text: self
                .settings
                .scene_items
                .get(self.settings.scene)
                .cloned()
                .unwrap_or_default()
                .into(),
            characters: slint::ModelRc::new(slint::VecModel::from(
                self.settings
                    .characters
                    .iter()
                    .map(|s| slint::SharedString::from(s.as_str()))
                    .collect::<Vec<_>>(),
            )),
            scenes: slint::ModelRc::new(slint::VecModel::from(
                self.settings
                    .scene_items
                    .iter()
                    .enumerate()
                    .map(|(i, _)| slint::SharedString::from(format!("Cảnh {}", i + 1)))
                    .collect::<Vec<_>>(),
            )),
        }
    }
    fn ensure_loaded(&mut self, studio: &mut Studio) {
        if self.loaded && !self.config_path.as_os_str().is_empty() {
            return;
        }
        self.flow_link_target.clear();
        self.config_path = studio
            .session
            .as_ref()
            .map(|s| std::path::Path::new(s.path()).join("google-flow.json"))
            .unwrap_or_else(|| studio.host.dirs.data.join("google-flow/settings.json"));
        if let Ok(bytes) = std::fs::read(&self.config_path) {
            if let Ok(settings) = serde_json::from_slice::<Settings>(&bytes) {
                self.settings = settings;
            }
        }
        self.settings.model = normalize_model(self.settings.model);
        self.settings.duration = self.settings.duration.min(12);
        self.settings.ratio = self.settings.ratio.min(1);
        if self.settings.scene_items.is_empty() {
            self.settings.scene_items = scene_blocks(&self.settings.screenplay);
        }
        self.settings.scene = self
            .settings
            .scene
            .min(self.settings.scene_items.len().saturating_sub(1));
        self.settings
            .plans
            .resize_with(self.settings.scene_items.len(), ScenePlan::default);
        for plan in &mut self.settings.plans {
            plan.recover();
        }
        if !self.settings.prompt.is_empty()
            && self.settings.plans.iter().all(|p| p.prompt.is_empty())
        {
            self.remember_prompt();
        }
        let index = self.settings.scene;
        self.select_scene(index, studio);
        self.settings.sync_character_ids();
        self.loaded = true;
    }
    fn write_sibling(&self, file_name: &str, bytes: impl AsRef<[u8]>) -> Result<PathBuf, String> {
        let base = if self.config_path.as_os_str().is_empty() {
            return Err("Chưa khởi tạo đường dẫn cấu hình dự án Flow.".into());
        } else {
            &self.config_path
        };
        let path = base.with_file_name(file_name);
        if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
            std::fs::create_dir_all(parent).map_err(|e| format!("Không tạo được thư mục: {e}"))?;
        }
        std::fs::write(&path, bytes).map_err(|e| format!("Không ghi được file {file_name}: {e}"))?;
        Ok(path)
    }
    fn save(&self) -> Result<(), String> {
        if self.config_path.as_os_str().is_empty() {
            return Ok(());
        }
        if let Some(parent) = self.config_path.parent().filter(|p| !p.as_os_str().is_empty()) {
            std::fs::create_dir_all(parent).map_err(|e| format!("Không tạo được thư mục cấu hình: {e}"))?;
        }
        let mut settings = self.settings.clone();
        settings
            .plans
            .resize_with(settings.scene_items.len(), ScenePlan::default);
        let bytes = serde_json::to_vec_pretty(&settings).map_err(|e| e.to_string())?;
        let temporary = self.config_path.with_extension("json.tmp");
        std::fs::write(&temporary, bytes).map_err(|e| format!("Không ghi được file cấu hình tạm: {e}"))?;
        std::fs::rename(&temporary, &self.config_path).map_err(|e| format!("Không lưu được file cấu hình: {e}"))
    }
    fn asset(&self, path: Option<&PathBuf>) -> crate::ui::FlowAsset {
        let Some(path) = path.filter(|p| !p.as_os_str().is_empty()) else {
            return Default::default();
        };
        let image = self
            .images
            .borrow_mut()
            .entry(path.clone())
            .or_insert_with(|| slint::Image::load_from_path(path).unwrap_or_default())
            .clone();
        crate::ui::FlowAsset {
            path: path.to_string_lossy().as_ref().into(),
            preview: image,
        }
    }
}

fn clean_visual_identity(raw: &str) -> String {
    for line in raw.lines() {
        let trimmed = line.trim();
        if let Some(visual) = trimmed.strip_prefix("Fixed visual identity:") {
            return visual.trim().to_string();
        }
    }
    let mut parts = Vec::new();
    for line in raw.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("Want & need:")
            || trimmed.starts_with("Flaw & motivation:")
            || trimmed.starts_with("Short arc:")
            || trimmed.starts_with("Voice:")
            || trimmed.starts_with("Internal monologue:")
            || trimmed.starts_with("Backstory:")
            || trimmed.starts_with("Profile:")
        {
            continue;
        }
        if let Some(ident) = trimmed.strip_prefix("Identity & role:") {
            parts.push(ident.trim());
        } else if !trimmed.is_empty() {
            parts.push(trimmed);
        }
    }
    if parts.is_empty() {
        raw.lines().next().unwrap_or("").trim().to_string()
    } else {
        parts.join(", ")
    }
}

pub(crate) fn clean_character_visual(raw: &str) -> String {
    let mut name = String::new();
    let mut role = String::new();
    let mut visual = String::new();
    let mut fallback_lines = Vec::new();

    for line in raw.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        if let Some(n) = trimmed.strip_prefix("Character:").or_else(|| trimmed.strip_prefix("Name:")) {
            name = n.trim().to_string();
        } else if let Some(v) = trimmed.strip_prefix("Fixed visual identity:").or_else(|| trimmed.strip_prefix("Visual:")) {
            visual = v.trim().to_string();
        } else if let Some(r) = trimmed.strip_prefix("Identity & role:").or_else(|| trimmed.strip_prefix("Role:")) {
            role = r.trim().to_string();
        } else if trimmed.starts_with("Want & need:")
            || trimmed.starts_with("Want:")
            || trimmed.starts_with("Need:")
            || trimmed.starts_with("Flaw & motivation:")
            || trimmed.starts_with("Flaw:")
            || trimmed.starts_with("Motivation:")
            || trimmed.starts_with("Short arc:")
            || trimmed.starts_with("Arc:")
            || trimmed.starts_with("Voice:")
            || trimmed.starts_with("Internal monologue:")
            || trimmed.starts_with("Backstory:")
            || trimmed.starts_with("Biography:")
            || trimmed.starts_with("Profile:")
        {
            continue;
        } else {
            fallback_lines.push(trimmed);
        }
    }

    let mut parts = Vec::new();
    if !name.is_empty() {
        if !role.is_empty() {
            parts.push(format!("{name} ({role})"));
        } else {
            parts.push(name);
        }
    } else if !role.is_empty() {
        parts.push(role);
    }

    if !visual.is_empty() {
        parts.push(visual);
    } else if !fallback_lines.is_empty() {
        parts.push(fallback_lines.join(", "));
    }

    if parts.is_empty() {
        raw.lines().next().unwrap_or("").trim().to_string()
    } else {
        parts.join(": ")
    }
}

impl Flow {
    fn continuity_for(&self, index: usize) -> String {
        let plan = self.settings.plans.get(index);
        let labels = self.settings.character_labels();
        let characters = self
            .settings
            .characters
            .iter()
            .enumerate()
            .filter(|(i, _)| plan.is_none_or(|p| !p.cast_configured || p.cast.contains(i)))
            .map(|(i, s)| {
                let is_flow_linked = self
                    .settings
                    .character_flow_origins
                    .get(i)
                    .is_some_and(|o| !o.id.is_empty());
                let visual = if is_flow_linked {
                    String::new()
                } else {
                    clean_visual_identity(s)
                };
                let action = plan.and_then(|p| p.character_actions.get(&self.settings.character_ids[i]))
                    .map(String::as_str)
                    .unwrap_or("")
                    .trim();
                if action.is_empty() {
                    if visual.is_empty() {
                        format!("- {}", labels[i])
                    } else {
                        format!("- {}: {}", labels[i], visual)
                    }
                } else if visual.is_empty() {
                    format!("- {}: action: {}", labels[i], action)
                } else {
                    format!("- {}: {}, action: {}", labels[i], visual, action)
                }
            })
            .collect::<Vec<_>>()
            .join("\n");
        let background = plan
            .and_then(|p| p.background)
            .and_then(|i| self.settings.backgrounds.get(i))
            .map(String::as_str)
            .unwrap_or("")
            .trim();
        let mut parts = Vec::new();
        if !self.settings.continuity.trim().is_empty() {
            parts.push(self.settings.continuity.trim().to_string());
        }
        if !background.is_empty() {
            parts.push(format!("Location: {}", background));
        }
        if !characters.is_empty() {
            parts.push(characters);
        }
        parts.join("\n")
    }
    /// Native Flow identities follow the same per-scene cast selection as continuity.
    fn native_character_refs(
        &self,
        scene: usize,
        account_id: &str,
        url: &str,
    ) -> Result<Vec<Value>, String> {
        self.settings.validate_cast(scene)?;
        let project = if let Some(r) = url.trim().strip_prefix("https://flow.google.com/project/") {
            r.split(['?', '#', '/']).next().unwrap_or("")
        } else if let Some((_, after)) = url.trim().split_once("project/") {
            after.split(['?', '#', '/']).next().unwrap_or("")
        } else {
            ""
        };
        let plan = self.settings.plans.get(scene);
        let mut result = Vec::new();
        for (i, origin) in self.settings.character_flow_origins.iter().enumerate() {
            if i >= self.settings.characters.len()
                || origin.id.is_empty()
                || plan.is_some_and(|p| p.cast_configured && !p.cast.contains(&i))
            {
                continue;
            }
            if !project.is_empty() && !origin.project_id.is_empty() && origin.project_id != project {
                return Err(format!(
                    "Nhân vật {} thuộc dự án Flow khác. Chọn đúng dự án nguồn hoặc bỏ nhân vật khỏi cảnh.",
                    i + 1
                ));
            }
            if (!origin.account_id.is_empty() && origin.account_id != account_id)
                || (origin.account_id.is_empty() && account_id != "primary")
            {
                return Err("Nhân vật Flow phải chạy bằng tài khoản nguồn đã nhập.".into());
            }
            if self.settings.model != 0 {
                return Err("Dùng nhân vật gốc kèm giọng Flow cần chọn Omni Flash 1.2.".into());
            }
            let char_name = flow_cast::name(&self.settings.characters[i]);
            result.push(json!({
                "project_id": origin.project_id,
                "id": origin.id,
                "voice": origin.voice,
                "local_name": char_name.chars().take(500).collect::<String>()
            }));
        }
        if result.len() > 8 {
            return Err(
                "Mỗi cảnh hỗ trợ tối đa 8 nhân vật gốc Flow; giới hạn thực tế tùy model/tài khoản."
                    .into(),
            );
        }
        Ok(result)
    }
    fn scene_references(&self, scene: usize, manual: &str) -> Result<Vec<String>, String> {
        let plan = self.settings.plans.get(scene);
        if let Some(plan) = plan.filter(|p| !p.scene_image.as_os_str().is_empty()) {
            if plan.image_stale {
                return Err("Ảnh cảnh đã cũ sau khi sửa nội dung/nhân vật. Tạo ảnh mới hoặc duyệt lại ảnh trước khi tạo video.".into());
            }
            return scene_state::references(&plan.scene_image.to_string_lossy());
        }
        let mut paths =
            manual
                .lines()
                .map(|line| line.trim().trim_matches('"'))
                .filter(|line| !line.is_empty())
                .filter(|line| {
                    !self.settings.character_images.iter().any(|image| {
                        !image.as_os_str().is_empty() && image == &PathBuf::from(*line)
                    })
                })
                .map(str::to_owned)
                .collect::<Vec<_>>();
        for (i, image) in self.settings.character_images.iter().enumerate() {
            if i >= self.settings.characters.len()
                || image.as_os_str().is_empty()
                || plan.is_some_and(|p| p.cast_configured && !p.cast.contains(&i))
                || self
                    .settings
                    .character_flow_origins
                    .get(i)
                    .is_some_and(|o| !o.id.is_empty())
            {
                continue;
            }
            paths.push(image.to_string_lossy().into_owned());
        }
        scene_state::references(&paths.join("\n"))
    }
    fn plan_mut(&mut self) -> Option<&mut ScenePlan> {
        self.settings
            .plans
            .resize_with(self.settings.scene_items.len(), ScenePlan::default);
        self.settings.plans.get_mut(self.settings.scene)
    }
    fn gallery(&self, index: usize) -> &[ReferenceImage] {
        self.settings
            .character_ids
            .get(index)
            .and_then(|id| self.settings.character_galleries.get(id))
            .map(Vec::as_slice)
            .unwrap_or(&[])
    }
    fn visual_rows(
        &self,
        gallery: &[ReferenceImage],
        anchor: Option<&PathBuf>,
    ) -> slint::ModelRc<crate::ui::FlowReferenceImage> {
        slint::ModelRc::new(slint::VecModel::from(
            gallery
                .iter()
                .enumerate()
                .map(|(i, item)| crate::ui::FlowReferenceImage {
                    label: format!(
                        "{} {}",
                        if item.kind == "sheet" {
                            "Bộ 3 góc"
                        } else if item.kind == "scene" {
                            "Ảnh cảnh"
                        } else {
                            "Ảnh nhân vật"
                        },
                        i + 1
                    )
                    .into(),
                    path: item.path.to_string_lossy().as_ref().into(),
                    preview: self.asset(Some(&item.path)).preview,
                    kind: item.kind.as_str().into(),
                    approved: item.approved,
                    selected: anchor.is_some_and(|path| path == &item.path),
                })
                .collect::<Vec<_>>(),
        ))
    }
    fn store_character_image(
        &mut self,
        index: usize,
        path: PathBuf,
        kind: &str,
        replace: bool,
    ) -> Result<(), String> {
        if slint::Image::load_from_path(&path).is_err() {
            return Err("Ảnh không đọc được; ảnh chuẩn cũ được giữ nguyên.".into());
        }
        let id = self
            .settings
            .character_ids
            .get(index)
            .ok_or("Nhân vật đã thay đổi")?
            .clone();
        self.settings
            .character_images
            .resize(self.settings.characters.len(), PathBuf::new());
        let old = self.settings.character_images[index].clone();
        let gallery = self.settings.character_galleries.entry(id).or_default();
        visual_state::remember(gallery, &old, "portrait");
        visual_state::remember(gallery, &path, kind);
        if kind != "sheet" && (replace || old.as_os_str().is_empty()) {
            self.settings.character_images[index] = path;
            self.invalidate_prompts();
        }
        Ok(())
    }
    fn start_visual(&mut self, target: ImageTarget, studio: &Studio) -> Result<(), String> {
        let paid = !matches!(target, ImageTarget::Preview(_));
        let root = studio.host.dirs.data.join("google-flow");
        let config = &studio.settings.flow_accounts.config;
        let account = if let ImageTarget::Scene(index) | ImageTarget::SceneAngles(index) = target {
            self.scene_candidates(index, config)?
                .into_iter()
                .find(|a| {
                    !studio.settings.flow_accounts.pending.contains(&a.id)
                        && !crate::flow_worker::in_use(&a.state_dir(&root))
                })
                .ok_or("Tài khoản của cảnh đang bận hoặc không sẵn sàng")?
        } else {
            config.get(&self.settings.source_account_id)?.clone()
        };
        if studio.settings.flow_accounts.pending.contains(&account.id)
            || crate::flow_worker::in_use(&account.state_dir(&root))
        {
            return Err("Tài khoản đang bận. Chờ tác vụ hiện tại hoàn tất.".into());
        }
        let url = if let ImageTarget::Scene(index) | ImageTarget::SceneAngles(index) = target {
            self.scene_project_url(index, &account)?
        } else if !account.project_url.trim().is_empty() {
            account.project_url.trim().into()
        } else {
            self.settings.url.trim().into()
        };
        if !valid_project_url(&url) {
            return Err("Nhập URL dự án Flow trong Cài đặt tài khoản hoặc mục Nhân vật.".into());
        }
        let mut refs = Vec::<String>::new();
        let mut ratio = IMAGE_RATIOS[self.settings.flow_image_ratio.min(4)];
        let prompt = match target {
            ImageTarget::Character(index) | ImageTarget::Preview(index) => {
                let profile = self
                    .settings
                    .characters
                    .get(index)
                    .ok_or("Chọn một nhân vật trước")?;
                if profile.trim().is_empty() {
                    return Err("Nhập hồ sơ nhân vật trước khi tạo ảnh".into());
                }
                if let Some(path) = self
                    .settings
                    .character_images
                    .get(index)
                    .filter(|p| p.is_file())
                {
                    refs.push(path.to_string_lossy().into_owned());
                }
                let visual_desc = clean_character_visual(profile);
                let style = self.settings.style.trim();
                if style.is_empty() {
                    format!(
                        "Full-body reference portrait of ONE character, neutral standing pose, plain studio background. Character: {visual_desc}"
                    )
                } else {
                    format!(
                        "Full-body reference portrait of ONE character, neutral standing pose, plain studio background. Character: {visual_desc}. Visual style: {style}"
                    )
                }
            }
            ImageTarget::Background(index) | ImageTarget::BackgroundSheet(index) => {
                let profile = self
                    .settings
                    .backgrounds
                    .get(index)
                    .ok_or("Chọn bối cảnh trước")?;
                if profile.trim().is_empty() {
                    return Err("Nhập mô tả bối cảnh trước khi tạo ảnh".into());
                }
                if let Some(path) = self
                    .settings
                    .background_images
                    .get(index)
                    .filter(|p| p.is_file())
                {
                    refs.push(path.to_string_lossy().into_owned());
                }
                if matches!(target, ImageTarget::BackgroundSheet(_)) {
                    ratio = "16:9";
                    visual_state::background_sheet_prompt(profile, &self.settings.style)
                } else {
                    format!(
                        "Create one environment reference image without people or text, set in an authentic Vietnamese location with realistic Vietnamese architecture, lighting and cultural details unless the location description explicitly states another country. Location: {profile}\nStyle: {}",
                        self.settings.style
                    )
                }
            }
            ImageTarget::Sheet(index) => {
                let profile = self
                    .settings
                    .characters
                    .get(index)
                    .ok_or("Chọn nhân vật trước")?;
                let anchor = self
                    .settings
                    .character_images
                    .get(index)
                    .filter(|p| p.is_file())
                    .ok_or("Chọn hoặc tạo ảnh chuẩn của nhân vật trước khi tạo bộ 3 góc.")?;
                refs.push(anchor.to_string_lossy().into_owned());
                ratio = "16:9";
                visual_state::sheet_prompt(profile, &self.settings.style)
            }
            ImageTarget::Scene(index) | ImageTarget::SceneAngles(index) => {
                self.settings.validate_cast(index)?;
                let plan = self
                    .settings
                    .plans
                    .get(index)
                    .ok_or("Chọn một cảnh trước")?;
                let scene = self
                    .settings
                    .scene_items
                    .get(index)
                    .filter(|s| !s.trim().is_empty())
                    .ok_or("Nhập nội dung cảnh trước")?;
                let labels = self.settings.character_labels();
                let mut cast = Vec::new();
                let mut bindings = Vec::new();
                for (i, profile) in self
                    .settings
                    .characters
                    .iter()
                    .enumerate()
                    .filter(|(i, _)| !plan.cast_configured || plan.cast.contains(i))
                {
                    let anchor = self
                        .settings
                        .character_images
                        .get(i)
                        .map(PathBuf::as_path)
                        .unwrap_or(std::path::Path::new(""));
                    let reference = visual_state::identity_reference(self.gallery(i), anchor);
                    if !reference.is_file() {
                        return Err(format!(
                            "{}: cần ảnh chuẩn hoặc bộ ảnh đã duyệt để tạo ảnh cảnh.",
                            labels[i]
                        ));
                    }
                    refs.push(reference.to_string_lossy().into_owned());
                    bindings.push(format!("Reference {} belongs to {} (one person, even if the reference contains several views).",refs.len(),labels[i]));
                    cast.push((labels[i].clone(), profile.clone()));
                }
                let background = plan
                    .background
                    .and_then(|i| self.settings.backgrounds.get(i))
                    .unwrap_or(&self.settings.continuity);
                if let Some(path) = plan
                    .background
                    .and_then(|i| self.settings.background_images.get(i))
                    .filter(|p| p.is_file())
                {
                    refs.push(path.to_string_lossy().into_owned());
                    bindings.push(format!(
                        "Reference {} is the location, not a person.",
                        refs.len()
                    ));
                }
                let directions = format!(
                    "Shot: {}; angle: {}; light: {}",
                    plan.shot, plan.camera, plan.lighting
                );
                let base = if matches!(target, ImageTarget::SceneAngles(_)) {
                    ratio = "16:9";
                    visual_state::scene_angles_prompt(
                        scene,
                        &cast,
                        background,
                        &directions,
                        &self.settings.style,
                    )
                } else {
                    ratio = if self.settings.ratio == 0 {
                        "9:16"
                    } else {
                        "16:9"
                    };
                    visual_state::scene_prompt(
                        scene,
                        &cast,
                        background,
                        &directions,
                        &self.settings.style,
                    )
                };
                base + "\n" + &bindings.join("\n")
            }
        };
        if refs.len() > 8 {
            return Err(
                "Tối đa 8 ảnh tham chiếu cho một ảnh cảnh. Giảm số nhân vật/ảnh bối cảnh.".into(),
            );
        }
        let args = json!({"project_url":url,"prompt":prompt,"model":IMAGE_MODELS[self.settings.flow_image_model.min(2)],
            "ratio":ratio,"count":1,"reference_images":refs,"auto_confirm":paid,"resume":false});
        if let ImageTarget::Scene(index) | ImageTarget::SceneAngles(index) = target {
            self.settings.plans[index].image_status = "running".into();
            if let Err(error) = self.save() {
                self.settings.plans[index].image_status.clear();
                return Err(error);
            }
        }
        self.busy = true;
        self.credit = false;
        self.active_account = account.id.clone();
        self.generation += 1;
        let g = self.generation;
        self.message = if paid {
            format!("{} đang tạo ảnh trên Flow…", account.name)
        } else {
            "Đang kiểm tra model, kết nối và chuẩn bị ảnh; chưa gửi tạo.".into()
        };
        spawn_in_project(
            move || {
                let value = crate::flow_worker::call_account(
                    &root,
                    &account,
                    "flow_generate_image",
                    args,
                    paid,
                )?;
                if !paid {
                    return if value["status"] == "ready_for_confirmation" {
                        Ok(None)
                    } else {
                        Err("Flow chưa xác nhận chuẩn bị ảnh; không gửi tạo.".into())
                    };
                }
                if value["status"] != "completed" {
                    return Err(
                        "Flow chưa xác nhận tạo ảnh xong. Kiểm tra Flow trước khi tạo lại.".into(),
                    );
                }
                crate::flow_worker::generated_images(&account.state_dir(&root), &value)
                    .into_iter()
                    .next()
                    .map(Some)
                    .ok_or_else(|| {
                        "Flow chưa tải về ảnh hợp lệ. Kiểm tra kết quả Flow trước khi tạo lại."
                            .into()
                    })
            },
            move |studio, _, _, result| {
                studio.handle(Msg::Flow(FlowMsg::VisualFinished(g, target, result)));
            },
        );
        Ok(())
    }
    fn remember_prompt(&mut self) {
        let prompt = self.settings.prompt.clone();
        let reference = self.settings.reference.clone();
        if let Some(plan) = self.plan_mut() {
            plan.prompt = prompt;
            plan.reference = reference;
        }
    }
    fn select_scene(&mut self, index: usize, studio: &Studio) {
        if self.settings.scene_items.is_empty() {
            self.settings.scene = 0;
            return;
        }
        self.settings.scene = index.min(self.settings.scene_items.len().saturating_sub(1));
        let plan = self
            .settings
            .plans
            .get(self.settings.scene)
            .cloned()
            .unwrap_or_default();
        self.settings.prompt = plan.prompt;
        self.settings.reference = plan.reference;
        self.files = crate::flow_worker::result_files(
            &studio.host.dirs.data.join("google-flow"),
            &json!({"files":plan.files}),
        );
        self.credit = false;
    }
    fn stop_queue(&mut self) {
        self.queue.clear();
        self.queue_running = false;
        for plan in &mut self.settings.plans {
            if plan.status == "waiting" {
                plan.status.clear();
            }
        }
    }
    fn invalidate_prompts(&mut self) {
        for plan in &mut self.settings.plans {
            plan.invalidate();
        }
    }
    pub(crate) fn account_in_use(&self, id: &str) -> bool {
        self.active_jobs.values().any(|account| account == id)
            || (self.busy && self.active_account == id)
    }
    pub(crate) fn accounts_locked(&self) -> bool {
        self.queue_running
            || !self.active_jobs.is_empty()
            || (!self.active_account.is_empty() && self.busy)
    }
    fn origins_for_scene(&self, index: usize) -> Vec<&FlowCharacterOrigin> {
        let plan = self.settings.plans.get(index);
        self.settings
            .character_flow_origins
            .iter()
            .enumerate()
            .filter(|(i, o)| {
                !o.id.is_empty()
                    && *i < self.settings.characters.len()
                    && plan.is_none_or(|p| !p.cast_configured || p.cast.contains(i))
            })
            .map(|(_, o)| o)
            .collect()
    }
    fn scene_candidates(
        &self,
        index: usize,
        config: &crate::flow_accounts::Config,
    ) -> Result<Vec<crate::flow_accounts::Account>, String> {
        let mut pinned = None;
        for origin in self.origins_for_scene(index) {
            let id = if origin.account_id.is_empty() {
                "primary"
            } else {
                origin.account_id.as_str()
            };
            if pinned.is_some_and(|old| old != id) {
                return Err("Một cảnh có nhân vật gốc từ nhiều tài khoản Flow. Chọn lại nhân vật cùng tài khoản nguồn.".into());
            }
            pinned = Some(id);
        }
        config.candidates(
            self.settings
                .plans
                .get(index)
                .map(|p| p.account_id.as_str())
                .unwrap_or(""),
            pinned,
        )
    }
    fn scene_project_url(
        &self,
        index: usize,
        account: &crate::flow_accounts::Account,
    ) -> Result<String, String> {
        if let Some(url) = self
            .settings
            .plans
            .get(index)
            .map(|p| p.project_url.trim())
            .filter(|s| !s.is_empty())
        {
            return Ok(url.into());
        }
        let origins = self.origins_for_scene(index);
        let project = origins.first().map(|o| o.project_id.as_str());
        if origins
            .iter()
            .any(|o| Some(o.project_id.as_str()) != project)
        {
            return Err("Nhân vật gốc trong cảnh thuộc nhiều dự án Flow khác nhau. Chọn nhân vật từ cùng dự án.".into());
        }
        Ok(if let Some(project) = project {
            format!("https://flow.google.com/project/{project}")
        } else if !account.project_url.trim().is_empty() {
            account.project_url.trim().into()
        } else {
            self.settings.url.trim().into()
        })
    }
    fn effective_project_url(&self, studio: &Studio) -> String {
        if valid_project_url(&self.settings.url) {
            return self.settings.url.trim().to_string();
        }
        if let Ok(account) = studio
            .settings
            .flow_accounts
            .config
            .get(&self.settings.source_account_id)
        {
            if valid_project_url(&account.project_url) {
                return account.project_url.trim().to_string();
            }
        }
        if let Ok(account) = studio
            .settings
            .flow_accounts
            .config
            .get("primary")
        {
            if valid_project_url(&account.project_url) {
                return account.project_url.trim().to_string();
            }
        }
        for plan in &self.settings.plans {
            if valid_project_url(&plan.project_url) {
                return plan.project_url.trim().to_string();
            }
        }
        String::new()
    }
    fn request_for(
        &self,
        index: usize,
        account: &crate::flow_accounts::Account,
        paid: bool,
    ) -> Result<Value, String> {
        self.settings.validate_cast(index)?;
        let plan = self
            .settings
            .plans
            .get(index)
            .ok_or("Chọn một cảnh trước khi tạo video.")?;
        if plan.needs_prompt || plan.prompt.trim().is_empty() {
            return Err("Cảnh chưa có prompt hoặc cần ghép/cập nhật prompt.".into());
        }
        let url = self.scene_project_url(index, account)?;
        if !valid_project_url(&url) {
            return Err(format!(
                "{}: nhập URL dự án Flow cho cảnh hoặc dự án mặc định trong Cài đặt.",
                account.name
            ));
        }
        let seconds = self.settings.duration + 3;
        if self.settings.model == 1 && ![4, 6, 8].contains(&seconds) {
            return Err("Veo Lite hỗ trợ 4/6/8 giây. Kiểm tra thời lượng video.".into());
        }
        let native = self.native_character_refs(index, &account.id, &url)?;
        let mut refs = self.scene_references(index, &plan.reference)?;
        if plan.visual_mode == 1 {
            refs.clear();
            if !plan.scene_image.as_os_str().is_empty() {
                refs.push(crate::flow_worker::clean_path(plan.scene_image.clone()).to_string_lossy().to_string());
            }
            if !plan.end_image.as_os_str().is_empty() {
                refs.push(crate::flow_worker::clean_path(plan.end_image.clone()).to_string_lossy().to_string());
            }
        } else if refs.len() > 3 {
            refs.truncate(3);
        }
        let labels = self.settings.character_labels();
        let bindings = refs
            .iter()
            .enumerate()
            .filter_map(|(number, path)| {
                let names = self
                    .settings
                    .character_images
                    .iter()
                    .enumerate()
                    .filter(|(i, image)| {
                        *image == &PathBuf::from(path)
                            && (!plan.cast_configured || plan.cast.contains(i))
                            && self
                                .settings
                                .character_flow_origins
                                .get(*i)
                                .is_none_or(|o| o.id.is_empty())
                    })
                    .filter_map(|(i, _)| labels.get(i).cloned())
                    .collect::<Vec<_>>();
                if names.is_empty() {
                    None
                } else {
                    Some(format!(
                        "Attached image {} is reference for {}.",
                        number + 1,
                        names.join(", ")
                    ))
                }
            })
            .collect::<Vec<_>>()
            .join("\n");
        let mut prompt = if bindings.is_empty() {
            plan.prompt.clone()
        } else {
            format!("{}\n\nReference images:\n{}", plan.prompt, bindings)
        };
        if plan.visual_mode == 1 {
            if !plan.scene_image.as_os_str().is_empty() && !plan.end_image.as_os_str().is_empty() {
                prompt.push_str("\n\n[TRANSITION] Start frame: attached image 1. End frame: attached image 2. Generate a smooth cinematic transition connecting start to end while preserving identities and environment.");
            } else if !plan.scene_image.as_os_str().is_empty() {
                prompt.push_str("\n\nFirst frame: attached image 1. Animate the described action while preserving composition and characters.");
            }
        } else if !plan.scene_image.as_os_str().is_empty() {
            prompt.push_str("\n\nFirst frame: attached image 1. Animate the described action while preserving composition and characters.");
        }

        // Clean up any speaker prefixes in prompt (e.g. "Name — Role:" -> "Name:")
        for (i, profile) in self.settings.characters.iter().enumerate() {
            let char_name = flow_cast::name(profile);
            if let Some(lbl) = labels.get(i) {
                if lbl != char_name {
                    let old_prefix = format!("{lbl}:");
                    let new_prefix = format!("{char_name}:");
                    prompt = prompt.replace(&old_prefix, &new_prefix);
                }
            }
        }

        // Add explicit Voice & Audio Directives for characters in this scene
        let mut voice_directives = Vec::new();
        for (i, origin) in self.settings.character_flow_origins.iter().enumerate() {
            if i < self.settings.characters.len() && (!plan.cast_configured || plan.cast.contains(&i)) {
                let char_name = flow_cast::name(&self.settings.characters[i]);
                let mut v_parts = Vec::new();
                if !origin.voice.is_empty() {
                    v_parts.push(format!("Flow Voice ID: {}", origin.voice));
                }
                if let Some(role) = self.settings.character_roles.get(i).filter(|r| !r.trim().is_empty()) {
                    v_parts.push(format!("Role/Voice notes: {role}"));
                }
                for line in self.settings.characters[i].lines() {
                    let l = line.trim();
                    if l.to_lowercase().starts_with("giọng") || l.to_lowercase().starts_with("voice") {
                        v_parts.push(l.to_string());
                    }
                }
                let desc = if v_parts.is_empty() {
                    "assigned Flow character voice".to_string()
                } else {
                    v_parts.join(", ")
                };
                voice_directives.push(format!(
                    "- Character '{char_name}': Must speak strictly using their {desc}. Preserve stable vocal pitch, gender, tone, and character identity."
                ));
            }
        }
        if !voice_directives.is_empty() {
            prompt.push_str("\n\nVoice & Audio Directives (Preserve Flow Character Voices):\n");
            prompt.push_str(&voice_directives.join("\n"));
        }
        Ok(
            json!({"project_url":url,"prompt":prompt,"model":MODELS[self.settings.model.min(MODELS.len() - 1)],"duration":seconds,"ratio":if self.settings.ratio==0{"9:16"}else{"16:9"},"reference_images":refs,"character_references":native,"auto_confirm":paid,"resume":false,"edit_mode":false,"provider":if self.settings.video_provider==1{"vids"}else{"flow"}}),
        )
    }
    fn next_queued(&mut self, studio: &mut Studio) {
        if !self.queue_running {
            return;
        }
        let config = studio.settings.flow_accounts.config.clone();
        let root = studio.host.dirs.data.join("google-flow");
        while self.active_jobs.len() < config.limit() {
            let external = crate::flow_worker::rendering_accounts()
                .iter()
                .filter(|id| !self.active_jobs.values().any(|own| own == *id))
                .count();
            if external + self.active_jobs.len() >= config.limit() {
                break;
            }
            let jobs = self
                .queue
                .iter()
                .map(|&i| (i, self.scene_candidates(i, &config).unwrap_or_default()))
                .collect::<Vec<_>>();
            let unavailable = config
                .accounts
                .iter()
                .filter(|a| {
                    studio.settings.flow_accounts.pending.contains(&a.id)
                        || crate::flow_worker::in_use(&a.state_dir(&root))
                })
                .map(|a| a.id.clone())
                .collect();
            let chosen = config.pick(
                &jobs,
                &self.active_jobs,
                &self.blocked_accounts,
                &unavailable,
                self.account_cursor,
            );
            let Some((position, index, account)) = chosen else {
                break;
            };
            self.queue.remove(position);
            let args = match self.request_for(index, &account, true) {
                Ok(args) => args,
                Err(e) => {
                    self.settings.plans[index].status = "edited".into();
                    self.queue_failures += 1;
                    self.message = format!("Cảnh {} chưa gửi: {e}", index + 1);
                    continue;
                }
            };
            self.settings.plans[index].status = "running".into();
            self.settings.plans[index].last_account_id = account.id.clone();
            self.active_jobs.insert(index, account.id.clone());
            self.account_cursor += 1;
            self.busy = true;
            if let Err(e) = self.save() {
                self.active_jobs.remove(&index);
                self.settings.plans[index].status = "edited".into();
                self.stop_queue();
                self.busy = !self.active_jobs.is_empty();
                self.message = e;
                return;
            }
            let dir = root.clone();
            let id = account.id.clone();
            let generation = self.generation;
            let tool = if self.settings.video_provider == 1 {
                "vids_generate_video"
            } else {
                "flow_generate_video"
            };
            spawn_in_project(
                move || {
                    crate::flow_worker::call_account(
                        &dir,
                        &account,
                        tool,
                        args,
                        true,
                    )
                },
                move |studio, _, _, result| {
                    studio.handle(Msg::Flow(FlowMsg::QueuedFinished(
                        generation, index, id, result,
                    )))
                },
            );
        }
        if self.active_jobs.is_empty() {
            let waiting = self.queue.len();
            self.stop_queue();
            self.busy = false;
            self.message = if waiting > 0 {
                format!(
                    "Hàng đợi đã dừng: {waiting} cảnh chưa gửi vì tài khoản không sẵn sàng. Kiểm tra Cài đặt và kết quả Flow trước khi chạy lại."
                )
            } else if self.queue_failures > 0 {
                format!(
                    "Hàng đợi kết thúc; {} cảnh cần kiểm tra. Không tự gửi lại tác vụ chưa rõ kết quả.",
                    self.queue_failures
                )
            } else {
                "Hàng đợi đã hoàn tất. Chọn cảnh để xem video.".into()
            };
        } else {
            self.busy = true;
            self.message = format!(
                "Đang chạy {} tài khoản / {} luồng; còn {} cảnh chờ.",
                self.active_jobs.len(),
                self.active_jobs.len(),
                self.queue.len()
            );
        }
    }
    /// Handles one UI action or worker completion on the event-loop thread.
    pub fn update(&mut self, msg: FlowMsg, studio: &mut Studio) {
        self.ensure_loaded(studio);
        self.settings.sync_character_ids();
        match msg {
            FlowMsg::VisualFinished(g, target, result) if g == self.generation => {
                self.busy = false;
                self.active_account.clear();
                let stored = (|| -> Result<(), String> {
                    let path = match result {
                        Ok(Some(path)) => path,
                        Ok(None) => {
                            self.message = "Đã chuẩn bị model và prompt tạo ảnh thành công. Chưa gửi tạo, chưa dùng credit. Bật credit rồi bấm Tạo ảnh khi sẵn sàng.".into();
                            return Ok(());
                        }
                        Err(error) => return Err(error),
                    };
                    if slint::Image::load_from_path(&path).is_err() {
                        return Err("Ảnh không đọc được; giữ nguyên ảnh cũ.".into());
                    }
                    match target {
                        ImageTarget::Character(index) => {
                            self.store_character_image(index, path, "portrait", false)?
                        }
                        ImageTarget::Sheet(index) => {
                            self.store_character_image(index, path, "sheet", false)?
                        }
                        ImageTarget::Background(index) | ImageTarget::BackgroundSheet(index) => {
                            self.settings
                                .background_images
                                .resize(self.settings.backgrounds.len(), PathBuf::new());
                            if let Some(image) = self.settings.background_images.get_mut(index) {
                                *image = path;
                            }
                            self.invalidate_prompts();
                        }
                        ImageTarget::Scene(index) | ImageTarget::SceneAngles(index) => {
                            let plan = self
                                .settings
                                .plans
                                .get_mut(index)
                                .ok_or("Cảnh đã thay đổi")?;
                            visual_state::remember(
                                &mut plan.image_history,
                                &plan.scene_image,
                                "scene",
                            );
                            visual_state::remember(
                                &mut plan.image_history,
                                &path,
                                if matches!(target, ImageTarget::SceneAngles(_)) {
                                    "sheet"
                                } else {
                                    "scene"
                                },
                            );
                            plan.image_status = "Đã tạo · chờ duyệt làm khung đầu".into();
                        }
                        ImageTarget::Preview(_) => {}
                    }
                    self.message=if matches!(target,ImageTarget::Sheet(_)) { "Đã lưu bộ 3 góc nhân vật. Kiểm tra khuôn mặt, tóc, quần áo rồi bấm Duyệt để dùng cho ảnh cảnh. Ảnh chuẩn được giữ nguyên." }
                        else if matches!(target,ImageTarget::BackgroundSheet(_)) { "Đã lưu bộ bối cảnh 3 góc. Bấm Dùng ảnh cho cảnh để gắn vào cảnh đang chọn." }
                        else if matches!(target,ImageTarget::Scene(_)|ImageTarget::SceneAngles(_)) { "Đã lưu ảnh cảnh. Bấm Duyệt làm khung đầu để dùng khi tạo video; chưa tự gửi video." }
                        else { "Đã lưu ảnh vào thư viện nhân vật; ảnh chuẩn cũ được giữ nguyên. Chọn Dùng làm ảnh chuẩn nếu muốn đổi." }.into();
                    Ok(())
                })();
                if let Err(error) = stored {
                    if let ImageTarget::Scene(index) | ImageTarget::SceneAngles(index) = target {
                        if let Some(plan) = self.settings.plans.get_mut(index) {
                            plan.image_status = "uncertain".into();
                        }
                    }
                    self.message = error;
                }
            }
            FlowMsg::QueueTick => self.next_queued(studio),
            FlowMsg::QueuedFinished(g, index, account, result)
                if g == self.generation && self.active_jobs.get(&index) == Some(&account) =>
            {
                self.active_jobs.remove(&index);
                let root = studio.host.dirs.data.join("google-flow");
                let files = result
                    .as_ref()
                    .map(|value| crate::flow_worker::result_files(&root, value))
                    .unwrap_or_default();
                let completed = result
                    .as_ref()
                    .is_ok_and(|value| value["status"] == "completed")
                    && !files.is_empty();
                if let Some(plan) = self.settings.plans.get_mut(index) {
                    plan.status = if completed { "completed" } else { "uncertain" }.into();
                    if completed {
                        plan.files = files.clone();
                    }
                }
                if self.settings.scene == index && completed {
                    self.files = files.clone();
                }
                if completed {
                    studio
                        .settings
                        .flow_accounts
                        .states
                        .insert(account.clone(), "Rảnh · tác vụ đã hoàn tất".into());
                    if let Some(video_file) = files.first() {
                        let current_video = video_file.clone();

                        // Extract thumbnail for card preview
                        let _ = post_process::extract_thumbnail(&current_video);

                        // 4. Update active view files if current scene
                        if self.settings.scene == index {
                            if let Some(plan) = self.settings.plans.get(index) {
                                self.files = plan.files.clone();
                            }
                        }

                        // 5. Auto Scene Flow continuity (scene N frame -> scene N+1 start frame)
                        let next_idx = index + 1;
                        if let Some(next_plan) = self.settings.plans.get_mut(next_idx) {
                            if next_plan.scene_flow_auto && next_plan.scene_image.as_os_str().is_empty() {
                                let frame_dst = root.join(format!("output/last-frame-scene-{}.png", next_idx + 1));
                                if let Ok(extracted) = post_process::extract_last_frame(&current_video, &frame_dst) {
                                    next_plan.scene_image = extracted;
                                    next_plan.image_status = "Tự động nối từ cảnh trước".into();
                                }
                            }
                        }

                        // 6. Auto Import to Timeline if enabled
                        if self.settings.auto_import_timeline && studio.session.is_some() {
                            studio.handle(crate::Msg::Media(crate::MediaMsg::Import(vec![current_video.clone()])));
                        }
                    }
                } else {
                    self.queue_failures += 1;
                    let error = result
                        .err()
                        .unwrap_or_else(|| "Tác vụ chưa trả video hoàn tất.".into());
                    let fatal_account_error = error.contains("BROWSER_NOT_CONNECTED")
                        || error.contains("NOT_LOGGED_IN")
                        || error.contains("CHROME_LAUNCH_FAILED")
                        || error.contains("QUOTA_EXCEEDED")
                        || error.contains("POLICY_BLOCKED")
                        || error.contains("chưa đăng nhập");
                    let mut rotated = false;
                    if fatal_account_error {
                        self.blocked_accounts.insert(account.clone());
                        let config = studio.settings.flow_accounts.config.clone();
                        let remaining_candidates = self
                            .scene_candidates(index, &config)
                            .unwrap_or_default()
                            .into_iter()
                            .filter(|a| !self.blocked_accounts.contains(&a.id))
                            .count();
                        if remaining_candidates > 0 {
                            if let Some(plan) = self.settings.plans.get_mut(index) {
                                plan.status = "waiting".into();
                            }
                            self.queue.push_front(index);
                            self.message = format!("Cảnh {} chuyển sang tài khoản dự phòng tiếp theo…", index + 1);
                            rotated = true;
                        }
                    }
                    studio
                        .settings
                        .flow_accounts
                        .states
                        .insert(account.clone(), format!("Cảnh {} lỗi: {error}", index + 1));
                    if !rotated {
                        self.message = format!("Cảnh {} cần kiểm tra Flow: {error}", index + 1);
                    }
                }
                self.busy = !self.active_jobs.is_empty();
                if let Err(e) = self.save() {
                    self.stop_queue();
                    self.message = e;
                } else {
                    self.next_queued(studio);
                }
            }
            FlowMsg::CharactersListed(g, url, result) if g == self.generation => {
                self.busy = false;
                self.active_account.clear();
                match result {
                    Ok(value) => {
                        let expected = url
                            .trim()
                            .strip_prefix("https://flow.google.com/project/")
                            .unwrap_or("")
                            .split(['?', '#', '/'])
                            .next()
                            .unwrap_or("");
                        let parsed = serde_json::from_value::<Vec<FlowLibraryCharacter>>(
                            value["characters"].clone(),
                        );
                        match parsed {
                            Ok(items) if value["project_id"].as_str()==Some(expected) && items.len()<=500 && items.iter().all(|item|item.project_id==expected && !item.id.is_empty() && item.name.len()<=1200 && item.personality.len()<=80000)=>{
                                self.remote_account_id=value["_account_id"].as_str().unwrap_or("primary").into();self.remote_characters=items;self.remote_project_url=url;
                                self.message=format!("Đã đọc {} nhân vật từ Flow. Chọn Nhập để tải hồ sơ và ảnh; không tạo lại nhân vật.",self.remote_characters.len());
                            }
                            _=>self.message="Dữ liệu nhân vật Flow không hợp lệ hoặc không thuộc dự án đã chọn.".into(),
                        }
                    }
                    Err(e) => self.message = e,
                }
            }
            FlowMsg::CharacterImported(g, project_id, character_id, result)
                if g == self.generation =>
            {
                self.busy = false;
                self.active_account.clear();
                let imported = (|| -> Result<(), String> {
                    let value = result?;
                    let account_id = value["_account_id"]
                        .as_str()
                        .ok_or("Thiếu tài khoản nguồn của nhân vật Flow.")?
                        .to_owned();
                    if account_id != self.settings.source_account_id {
                        return Err("Tài khoản nguồn đã đổi. Đọc lại thư viện Flow.".into());
                    }
                    let account = studio.settings.flow_accounts.config.get(&account_id)?;
                    let item: FlowLibraryCharacter =
                        serde_json::from_value(value["character"].clone())
                            .map_err(|_| "Hồ sơ nhân vật Flow không hợp lệ.".to_owned())?;
                    if item.project_id != project_id
                        || item.id != character_id
                        || item.name.len() > 1200
                        || item.personality.len() > 80000
                    {
                        return Err("Flow trả sai nhân vật/dự án. Chưa cập nhật hồ sơ.".into());
                    }
                    let image = crate::flow_worker::imported_image(
                        &account.state_dir(&studio.host.dirs.data.join("google-flow")),
                        &value,
                    )?;
                    if item.has_portrait && image.is_none() {
                        return Err(
                            "Flow chưa trả ảnh của nhân vật đã chọn. Chưa cập nhật hồ sơ.".into(),
                        );
                    }
                    if let Some(path) = image.as_ref() {
                        slint::Image::load_from_path(path).map_err(|_| {
                            "Không đọc được ảnh chân dung Flow. Hồ sơ cũ được giữ nguyên."
                                .to_owned()
                        })?;
                    }
                    self.write_sibling(
                        "flow-before-character-import.json",
                        serde_json::to_vec_pretty(&self.settings).map_err(|e| e.to_string())?,
                    )?;
                    self.settings
                        .character_flow_origins
                        .resize_with(self.settings.characters.len(), FlowCharacterOrigin::default);
                    let existing = self
                        .settings
                        .character_flow_origins
                        .iter()
                        .position(|origin| {
                            origin.project_id == item.project_id
                                && origin.id == item.id
                                && (origin.account_id == account_id
                                    || (origin.account_id.is_empty() && account_id == "primary"))
                        });
                    let target = self
                        .settings
                        .character_ids
                        .iter()
                        .position(|id| id == &self.flow_link_target);
                    if !self.flow_link_target.is_empty() && target.is_none() {
                        return Err(
                            "Hồ sơ cần liên kết không còn tồn tại. Chọn lại nhân vật.".into()
                        );
                    }
                    if existing.is_some() && target.is_some() && existing != target {
                        return Err(
                            "Nhân vật Flow này đã gắn với hồ sơ khác. Hãy chọn đúng hồ sơ.".into(),
                        );
                    }
                    let index = existing
                        .or(target)
                        .unwrap_or(self.settings.characters.len());
                    let profile = format!(
                        "{}\nMô tả từ Flow: {}{}",
                        item.name,
                        item.personality,
                        if item.voice.is_empty() {
                            String::new()
                        } else {
                            format!("\nGiọng Flow (mã tham chiếu): {}", item.voice)
                        }
                    );
                    if index == self.settings.characters.len() {
                        self.settings.characters.push(profile);
                    } else {
                        let role = self.settings.character_roles.get(index).cloned().unwrap_or_default();
                        let clean_role = if !role.trim().is_empty() {
                            format!("Role: {}\n", role.trim())
                        } else {
                            String::new()
                        };
                        self.settings.characters[index] = format!(
                            "{}\n{}Mô tả từ Flow: {}{}",
                            item.name,
                            clean_role,
                            item.personality,
                            if item.voice.is_empty() {
                                String::new()
                            } else {
                                format!("\nGiọng Flow (mã tham chiếu): {}", item.voice)
                            }
                        );
                    }
                    self.settings
                        .character_flow_origins
                        .resize_with(self.settings.characters.len(), FlowCharacterOrigin::default);
                    self.settings
                        .character_images
                        .resize(self.settings.characters.len(), PathBuf::new());
                    if let Some(path) = image {
                        self.settings.character_images[index] = path;
                    }
                    self.settings.character_flow_origins[index] = FlowCharacterOrigin {
                        account_id,
                        project_id: item.project_id,
                        id: item.id,
                        voice: item.voice,
                        portrait_media_id: item.portrait_media_id,
                    };
                    self.invalidate_prompts();
                    self.credit = false;
                    self.message="Đã nhập/cập nhật hồ sơ và ảnh có sẵn từ Flow. Mã giọng chỉ là thông tin tham chiếu, chưa tải tệp giọng nói.".into();
                    if let Some(warning) = value["warning"].as_str().filter(|s| !s.is_empty()) {
                        self.message.push(' ');
                        self.message.push_str(warning);
                    }
                    Ok(())
                })();
                if let Err(e) = imported {
                    self.message = e;
                }
            }
            FlowMsg::ReferencePicked(g, index, profiles, source)
                if g == self.generation
                    && !self.busy
                    && profiles == self.settings.character_ids =>
            {
                if index >= self.settings.characters.len() {
                    return;
                }
                self.busy = true;
                self.credit = false;
                self.generation += 1;
                let generation = self.generation;
                let directory = studio.host.dirs.data.join("google-flow/assets");
                self.message = "Đang lưu ảnh tham chiếu nhân vật…".into();
                spawn_in_project(
                    move || {
                        let size = std::fs::metadata(&source).map_err(|e| e.to_string())?.len();
                        if size == 0 || size > 32 * 1024 * 1024 {
                            return Err("Chọn ảnh có dung lượng từ 1 byte đến 32 MB.".into());
                        }
                        let extension = source
                            .extension()
                            .and_then(|value| value.to_str())
                            .unwrap_or("")
                            .to_ascii_lowercase();
                        if !["png", "jpg", "jpeg", "webp"].contains(&extension.as_str()) {
                            return Err("Ảnh tham chiếu cần là PNG, JPG, JPEG hoặc WebP.".into());
                        }
                        std::fs::create_dir_all(&directory).map_err(|e| e.to_string())?;
                        let id = std::time::SystemTime::now()
                            .duration_since(std::time::UNIX_EPOCH)
                            .map_err(|e| e.to_string())?
                            .as_nanos();
                        let destination = directory.join(format!(
                            "character-reference-{}-{generation}-{id}.{extension}",
                            std::process::id()
                        ));
                        std::fs::copy(&source, &destination).map_err(|e| e.to_string())?;
                        Ok(destination)
                    },
                    move |studio, _, _, result| {
                        studio.handle(Msg::Flow(FlowMsg::ImageFinished(
                            generation, true, index, result,
                        )));
                    },
                );
            }
            FlowMsg::ImageFinished(g, character, index, result) if g == self.generation => {
                self.busy = false;
                self.active_account.clear();
                match result {
                    Ok(path) => {
                        if slint::Image::load_from_path(&path).is_err() {
                            self.message =
                                "Ảnh tham chiếu không đọc được. Ảnh cũ được giữ nguyên.".into();
                            return;
                        }
                        if character {
                            if let Err(error) =
                                self.store_character_image(index, path, "portrait", true)
                            {
                                self.message = error;
                                return;
                            }
                        } else {
                            self.settings
                                .background_images
                                .resize(self.settings.backgrounds.len(), PathBuf::new());
                            if let Some(image) = self.settings.background_images.get_mut(index) {
                                *image = path;
                            }
                            self.invalidate_prompts();
                        }
                        self.message="Đã lưu ảnh tham chiếu. Bấm Dùng cho cảnh để gắn ảnh vào cảnh đang chọn.".into();
                    }
                    Err(e) => self.message = e,
                }
            }
            FlowMsg::AiFinished(g, kind, result) if g == self.generation => {
                self.busy = false;
                match result {
                    Ok(value) => {
                        match kind {
                            41 | 42 | 46 => {
                                let mut draft = self.settings.clone();
                                let start = if kind == 41 {
                                    draft.scene_items.len()
                                } else {
                                    0
                                };
                                if kind == 41 {
                                    draft.logline = value["logline"]
                                        .as_str()
                                        .filter(|s| s.len() <= 2000)
                                        .unwrap_or("")
                                        .to_owned();
                                    draft.scene_items.extend(
                                        value["scenes"]
                                            .as_array()
                                            .into_iter()
                                            .flatten()
                                            .filter_map(Value::as_str)
                                            .map(str::to_owned),
                                    );
                                    draft.screenplay = draft.scene_items.join("\n\n");
                                }
                                let count = if kind == 42 {
                                    0
                                } else {
                                    draft.scene_items.len() - start
                                };
                                if let Err(e) =
                                    cast_state::apply_draft(&mut draft, &value, start, count)
                                {
                                    self.message = e;
                                    return;
                                }
                                if let Err(e) = serde_json::to_vec_pretty(&self.settings)
                                    .map_err(|e| e.to_string())
                                    .and_then(|bytes| {
                                        self.write_sibling("flow-before-cast-sync.json", bytes)
                                            .map(|_| ())
                                    })
                                {
                                    self.message = e;
                                    return;
                                }
                                self.settings = draft;
                                self.invalidate_prompts();
                                let selected = if kind == 41 {
                                    start
                                } else {
                                    self.settings.scene
                                };
                                self.select_scene(selected, studio);
                                self.message="Đã đồng bộ hồ sơ nhân vật, vai trong cảnh và người nói theo ID. Ảnh và liên kết Flow cũ được giữ nguyên.".into();
                            }
                            43 => {
                                self.settings.backgrounds.push(
                                    value["background"].as_str().unwrap_or_default().to_owned(),
                                );
                                self.background_edit = self.settings.backgrounds.len() - 1;
                                let index = self.background_edit;
                                if let Some(plan) = self.plan_mut() {
                                    plan.background = Some(index);
                                }
                                self.invalidate_prompts();
                            }
                            44 => {
                                self.settings.prompt =
                                    value["prompt"].as_str().unwrap_or_default().to_owned();
                                self.remember_prompt();
                                if let Some(plan) = self.plan_mut() {
                                    plan.needs_prompt = false;
                                    plan.edited();
                                }
                            }
                            45 => {
                                if let Some(plan) = self.plan_mut() {
                                    if !plan.dialogue_lines.is_empty() {
                                        let lines = serde_json::from_value::<
                                            Vec<scene_state::DialogueLine>,
                                        >(
                                            value["dialogue_lines"].clone()
                                        );
                                        match lines {
                                            Ok(lines)
                                                if lines.len() == plan.dialogue_lines.len()
                                                    && lines
                                                        .iter()
                                                        .zip(&plan.dialogue_lines)
                                                        .all(|(a, b)| {
                                                            a.character_id == b.character_id
                                                        }) =>
                                            {
                                                plan.dialogue_lines = lines
                                            }
                                            _ => {
                                                self.message="AI đổi cấu trúc người nói. Lời thoại cũ được giữ nguyên.".into();
                                                return;
                                            }
                                        }
                                    } else {
                                        plan.dialogue = value["dialogue"]
                                            .as_str()
                                            .unwrap_or_default()
                                            .to_owned();
                                    }
                                    plan.invalidate();
                                }
                            }
                            49 => {
                                if let Some(items) = value["instructions"].as_array() {
                                    let mut sync_items = Vec::new();
                                    for item in items {
                                        let title = item["title"].as_str().unwrap_or("").trim();
                                        let text = item["text"].as_str().unwrap_or("").trim();
                                        if title.is_empty() || text.is_empty() {
                                            continue;
                                        }
                                        let char_id = item["character_id"].as_str().unwrap_or("").trim();
                                        let mut image_name: Option<String> = None;
                                        if !char_id.is_empty() {
                                            if let Some(pos) = self.settings.character_ids.iter().position(|id| id == char_id) {
                                                if let Some(img_path) = self.settings.character_images.get(pos) {
                                                    if let Some(name) = img_path.file_name().and_then(|n| n.to_str()) {
                                                        image_name = Some(name.to_string());
                                                    }
                                                }
                                            }
                                        }
                                        if image_name.is_none() {
                                            for (pos, name) in self.settings.characters.iter().enumerate() {
                                                if !name.trim().is_empty() && title.to_lowercase().contains(&name.to_lowercase()) {
                                                    if let Some(img_path) = self.settings.character_images.get(pos) {
                                                        if let Some(name) = img_path.file_name().and_then(|n| n.to_str()) {
                                                            image_name = Some(name.to_string());
                                                            break;
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                        let mut obj = json!({
                                            "title": title,
                                            "text": text,
                                        });
                                        if let Some(img) = image_name {
                                            obj["image_name"] = json!(img);
                                        }
                                        sync_items.push(obj);
                                    }
                                    if sync_items.is_empty() {
                                        self.message = "Không có chỉ dẫn nào được tạo từ AI.".into();
                                        return;
                                    }
                                    let dir = studio.host.dirs.data.join("google-flow");
                                    let account = match studio
                                        .settings
                                        .flow_accounts
                                        .config
                                        .get(&self.settings.source_account_id)
                                    {
                                        Ok(a) => a.clone(),
                                        Err(e) => {
                                            self.message = e;
                                            return;
                                        }
                                    };
                                    let project_url = self.effective_project_url(studio);
                                    self.active_account = account.id.clone();
                                    self.busy = true;
                                    self.generation += 1;
                                    let g = self.generation;
                                    self.message = format!("Đang đồng bộ {} chỉ dẫn vào Agent instructions của Flow...", sync_items.len());
                                    let mut args = json!({
                                        "instructions": sync_items,
                                    });
                                    if !project_url.is_empty() {
                                        args["project_url"] = json!(project_url);
                                    }
                                    spawn_in_project(
                                        move || {
                                            crate::flow_worker::call_account(&dir, &account, "flow_sync_agent_instructions", args, false)
                                        },
                                        move |studio, _, _, result| {
                                            studio.handle(Msg::Flow(FlowMsg::Finished(g, result)))
                                        },
                                    );
                                    return;
                                } else {
                                    self.message = "AI trả về danh sách chỉ dẫn không hợp lệ.".into();
                                    return;
                                }
                            }
                            50 => {
                                let mut changes = Vec::new();
                                if let Some(sp) = value["screenplay"].as_str().filter(|s| !s.trim().is_empty()) {
                                    self.settings.screenplay = sp.to_string();
                                    self.settings.scene_items = scene_blocks(&self.settings.screenplay);
                                    self.settings.plans.resize_with(self.settings.scene_items.len(), ScenePlan::default);
                                    self.settings.scene = self.settings.scene.min(self.settings.scene_items.len().saturating_sub(1));
                                    changes.push("Kịch bản");
                                } else if let Some(scenes) = value["scenes"].as_array().filter(|a| !a.is_empty()) {
                                    let items: Vec<String> = scenes.iter().filter_map(|v| v.as_str().map(|s| s.to_string())).collect();
                                    if !items.is_empty() {
                                        self.settings.screenplay = items.join("\n\n");
                                        self.settings.scene_items = items;
                                        self.settings.plans.resize_with(self.settings.scene_items.len(), ScenePlan::default);
                                        self.settings.scene = self.settings.scene.min(self.settings.scene_items.len().saturating_sub(1));
                                        changes.push("Phân cảnh");
                                    }
                                }
                                if let Some(chars) = value["characters"].as_array().filter(|a| !a.is_empty()) {
                                    for c in chars {
                                        let name = c["name"].as_str().unwrap_or("").trim();
                                        let profile = c["profile"].as_str().unwrap_or("").trim();
                                        let role = c["role"].as_str().unwrap_or("").trim();
                                        let id = c["id"].as_str().unwrap_or("").trim();
                                        if name.is_empty() { continue; }
                                        
                                        let pos = if !id.is_empty() {
                                            self.settings.character_ids.iter().position(|cid| cid == id)
                                        } else {
                                            None
                                        }.or_else(|| {
                                            self.settings.characters.iter().position(|cname| cname.eq_ignore_ascii_case(name))
                                        });

                                        if let Some(i) = pos {
                                            if !profile.is_empty() {
                                                self.settings.characters[i] = profile.to_string();
                                            }
                                            if !role.is_empty() && i < self.settings.character_roles.len() {
                                                self.settings.character_roles[i] = role.to_string();
                                            }
                                        } else {
                                            self.settings.characters.push(if profile.is_empty() { name.to_string() } else { profile.to_string() });
                                            self.settings.character_roles.push(role.to_string());
                                            self.settings.character_images.push(PathBuf::new());
                                            self.settings.character_ids.push(if !id.is_empty() && !id.starts_with("new-") { id.to_string() } else { format!("char-{}", self.settings.characters.len()) });
                                        }
                                    }
                                    self.settings.sync_character_ids();
                                    changes.push("Nhân vật");
                                }
                                if let Some(bg) = value["background"].as_str().filter(|s| !s.trim().is_empty()) {
                                    self.settings.continuity = bg.to_string();
                                    changes.push("Bối cảnh");
                                }
                                if let Some(st) = value["style"].as_str().filter(|s| !s.trim().is_empty()) {
                                    self.settings.style = st.to_string();
                                    changes.push("Phong cách");
                                }
                                if let Some(patch) = value["selected_scene_patch"].as_object() {
                                    if let Some(plan) = self.settings.plans.get_mut(self.settings.scene) {
                                        if let Some(shot) = patch.get("shot").and_then(|v| v.as_str()).filter(|s| !s.trim().is_empty()) {
                                            plan.shot = shot.to_string();
                                        }
                                        if let Some(camera) = patch.get("camera").and_then(|v| v.as_str()).filter(|s| !s.trim().is_empty()) {
                                            plan.camera = camera.to_string();
                                        }
                                        if let Some(movement) = patch.get("movement").and_then(|v| v.as_str()).filter(|s| !s.trim().is_empty()) {
                                            plan.movement = movement.to_string();
                                        }
                                        if let Some(lighting) = patch.get("lighting").and_then(|v| v.as_str()).filter(|s| !s.trim().is_empty()) {
                                            plan.lighting = lighting.to_string();
                                        }
                                        if let Some(sound) = patch.get("sound").and_then(|v| v.as_str()).filter(|s| !s.trim().is_empty()) {
                                            plan.sound = sound.to_string();
                                        }
                                        if let Some(dialogue) = patch.get("dialogue").and_then(|v| v.as_str()).filter(|s| !s.trim().is_empty()) {
                                            plan.dialogue = dialogue.to_string();
                                        }
                                        if let Some(action) = patch.get("action").and_then(|v| v.as_str()).filter(|s| !s.trim().is_empty()) {
                                            if let Some(first_char_id) = self.settings.character_ids.first() {
                                                plan.character_actions.insert(first_char_id.clone(), action.to_string());
                                            }
                                        }
                                        plan.invalidate();
                                        changes.push("Chi tiết cảnh");
                                    }
                                }
                                self.invalidate_prompts();
                                let exp = value["explanation"].as_str().unwrap_or("Đã cập nhật theo yêu cầu.");
                                self.message = format!("✨ Trợ lý AI: {exp}");
                                self.copilot_prompt.clear();
                            }
                            51 => {
                                if let Some(revised) = value["revised_text"].as_str() {
                                    let text = revised.trim().to_string();
                                    match self.ai_target_kind {
                                        40 => self.settings.brief = text,
                                        93 => self.settings.writing_audience = text,
                                        94 => self.settings.writing_goal = text,
                                        20 => {
                                            self.settings.screenplay = text;
                                            self.settings.scene_items = scene_blocks(&self.settings.screenplay);
                                            self.settings.plans.resize_with(self.settings.scene_items.len(), ScenePlan::default);
                                            self.settings.scene = self.settings.scene.min(self.settings.scene_items.len().saturating_sub(1));
                                        }
                                        29 => {
                                            if let Some(c) = self.settings.characters.get_mut(self.ai_sub_idx) {
                                                *c = text;
                                            }
                                        }
                                        113 => {
                                            if let Some(r) = self.settings.character_roles.get_mut(self.ai_sub_idx) {
                                                *r = text;
                                            }
                                        }
                                        67 => {
                                            if let Some(b) = self.settings.backgrounds.get_mut(self.background_edit) {
                                                *b = text;
                                            }
                                        }
                                        21 => self.settings.continuity = text,
                                        22 => self.settings.style = text,
                                        27 => {
                                            if let Some(s) = self.settings.scene_items.get_mut(self.settings.scene) {
                                                *s = text.clone();
                                            }
                                            self.settings.screenplay = self.settings.scene_items.join("\n\n");
                                            if let Some(plan) = self.settings.plans.get_mut(self.settings.scene) {
                                                plan.edited();
                                            }
                                        }
                                        105 => {
                                            if let Some(plan) = self.settings.plans.get_mut(self.settings.scene) {
                                                if let Some(char_id) = self.settings.character_ids.get(self.ai_sub_idx) {
                                                    plan.character_actions.insert(char_id.clone(), text);
                                                    plan.edited();
                                                }
                                            }
                                        }
                                        60 => {
                                            if let Some(plan) = self.settings.plans.get_mut(self.settings.scene) {
                                                plan.shot = text;
                                                plan.edited();
                                            }
                                        }
                                        55 => {
                                            if let Some(plan) = self.settings.plans.get_mut(self.settings.scene) {
                                                plan.camera = text;
                                                plan.edited();
                                            }
                                        }
                                        56 => {
                                            if let Some(plan) = self.settings.plans.get_mut(self.settings.scene) {
                                                plan.movement = text;
                                                plan.edited();
                                            }
                                        }
                                        62 => {
                                            if let Some(plan) = self.settings.plans.get_mut(self.settings.scene) {
                                                plan.lighting = text;
                                                plan.edited();
                                            }
                                        }
                                        57 => {
                                            if let Some(plan) = self.settings.plans.get_mut(self.settings.scene) {
                                                plan.sound = text;
                                                plan.edited();
                                            }
                                        }
                                        108 => {
                                            if let Some(plan) = self.settings.plans.get_mut(self.settings.scene) {
                                                if let Some(line) = plan.dialogue_lines.get_mut(self.ai_sub_idx) {
                                                    line.text = text;
                                                    plan.edited();
                                                }
                                            }
                                        }
                                        58 => {
                                            if let Some(plan) = self.settings.plans.get_mut(self.settings.scene) {
                                                plan.dialogue = text;
                                                plan.edited();
                                            }
                                        }
                                        3 => {
                                            self.settings.prompt = text;
                                            self.remember_prompt();
                                            if let Some(plan) = self.settings.plans.get_mut(self.settings.scene) {
                                                plan.needs_prompt = false;
                                                plan.edited();
                                            }
                                        }
                                        _ => {}
                                    }
                                    self.invalidate_prompts();
                                    self.message = "✨ Đã cập nhật nội dung từ AI.".into();
                                }
                            }
                            _ => {}
                        }
                        self.credit = false;
                        if !matches!(kind, 41 | 42 | 46 | 50 | 51) {
                            self.message = "Đã nhận bản nháp từ Gemini. Bạn có thể chỉnh sửa; chưa gửi tạo video Flow.".into();
                        }
                        self.settings.sync_character_ids();
                        if let Err(e) = self.save() {
                            self.message = e;
                        }
                    }
                    Err(e) => self.message = e,
                }
            }
            FlowMsg::Finished(g, result) if g == self.generation => {
                self.busy = false;
                self.active_account.clear();
                let active = self.active_scene.take();
                let mut continue_queue = false;
                if let Some((index, paid)) = active {
                    let files = result
                        .as_ref()
                        .ok()
                        .map(|v| {
                            crate::flow_worker::result_files(
                                &studio.host.dirs.data.join("google-flow"),
                                v,
                            )
                        })
                        .unwrap_or_default();
                    let completed = result.as_ref().is_ok_and(|v| v["status"] == "completed")
                        && !files.is_empty();
                    let prepared = result
                        .as_ref()
                        .is_ok_and(|v| v["status"] == "ready_for_confirmation");
                    if let Some(plan) = self.settings.plans.get_mut(index) {
                        plan.status = if completed {
                            "completed"
                        } else if !paid && prepared {
                            "prepared"
                        } else {
                            "uncertain"
                        }
                        .into();
                        if completed {
                            plan.files = files.clone();
                            if let Some(video_file) = files.first() {
                                let current_video = video_file.clone();
                                let _ = post_process::extract_thumbnail(&current_video);
                                if self.settings.auto_import_timeline && studio.session.is_some() {
                                    studio.handle(Msg::Media(MediaMsg::Import(vec![current_video])));
                                }
                            }
                        }
                    }
                    continue_queue = completed && paid;
                    if let Err(e) = self.save() {
                        self.message = e;
                        self.stop_queue();
                        return;
                    }
                }
                match result {
                    Ok(value) => {
                        let dir = studio.host.dirs.data.join("google-flow");
                        let files = crate::flow_worker::result_files(&dir, &value);
                        if !files.is_empty() {
                            if let Some(plan) = self.settings.plans.get(self.settings.scene) {
                                self.files = plan.files.clone();
                            } else {
                                self.files = files;
                            }
                        }
                        if let Some(new_url) = value.get("project_url").and_then(|v| v.as_str()) {
                            self.settings.url = new_url.to_string();
                            self.remote_characters.clear();
                            self.remote_project_url.clear();
                            if let Some(account) = studio
                                .settings
                                .flow_accounts
                                .draft
                                .accounts
                                .iter_mut()
                                .find(|a| a.id == self.settings.source_account_id)
                            {
                                account.project_url = new_url.to_string();
                                let _ = studio
                                    .settings
                                    .flow_accounts
                                    .save(&studio.host.dirs.data.join("google-flow"));
                            }
                            if let Some(plan) = self.plan_mut() {
                                plan.project_url = new_url.to_string();
                                plan.edited();
                            }
                            let _ = self.save();
                        }
                        self.message = if let Some(new_url) = value.get("project_url").and_then(|v| v.as_str()) {
                            format!("Đã tạo thành công dự án Flow mới: {}", new_url)
                        } else if value.get("synced_count").is_some() {
                            value["message"].as_str().unwrap_or("Đã đồng bộ Agent instructions thành công.").into()
                        } else if value["status"] == "ready_for_confirmation" {
                            format!(
                                "Đã chọn model, chuẩn bị prompt và gắn {} nhân vật gốc Flow kèm giọng đã lưu (nếu có). Chưa gửi, chưa dùng credit.",
                                value["characters_attached"].as_u64().unwrap_or(0)
                            )
                        } else if value["status"] == "completed" {
                            format!(
                                "Đã tải {} clip. Chọn Nhập để thêm vào thư viện dự án.",
                                self.files.len()
                            )
                        } else if value.get("logged_in").is_some() {
                            let connected = value
                                .get("connected")
                                .and_then(|v| v.as_bool())
                                .unwrap_or(true);
                            if !connected {
                                "Chưa mở Chrome Flow. Bấm Mở / kết nối lại Chrome Flow để khởi chạy trình duyệt.".into()
                            } else if value["logged_in"] == true {
                                "Đã kết nối và đăng nhập Google Flow trong Chrome. Sẵn sàng đọc nhân vật hoặc tạo ảnh/video.".into()
                            } else {
                                "Đã mở Chrome nhưng chưa đăng nhập Google Flow. Hãy đăng nhập tài khoản Google trong cửa sổ Chrome vừa mở rồi bấm Kiểm tra lại.".into()
                            }
                        } else if value.get("unresolved").is_some() {
                            format!(
                                "Đã tìm {} clip đã lưu. Kiểm tra Flow trước khi tạo lại tác vụ gián đoạn.",
                                self.files.len()
                            )
                        } else {
                            format!(
                                "Worker sẵn sàng. {}",
                                if value["connected"] == true {
                                    "Chrome đang kết nối."
                                } else {
                                    "Bấm Kết nối Flow để mở Chrome."
                                }
                            )
                        };
                    }
                    Err(e) => self.message = e,
                }
                if self.queue_running {
                    if continue_queue {
                        self.next_queued(studio);
                    } else {
                        self.stop_queue();
                        self.message
                            .push_str(" Hàng đợi đã dừng. Kiểm tra Flow trước khi tạo lại.");
                    }
                }
            }
            FlowMsg::Action(0, _, _) => {
                self.open = true;
                self.ensure_loaded(studio);
            }
            FlowMsg::Action(1, _, _) => {
                self.open = false;
                self.credit = false;
                self.stop_queue();
                if let Err(e) = self.save() {
                    studio.notify(&e, true);
                }
            }
            FlowMsg::Action(53, _, _) => {
                self.stop_queue();
                self.credit = false;
                self.message =
                    "Đã dừng hàng đợi. Cảnh đã gửi vẫn tiếp tục render trên Flow.".into();
            }
            FlowMsg::Action(120, _, _) => {
                studio.handle(Msg::Settings(
                    crate::panes::settings::SettingsMsg::PageChanged(8),
                ));
                studio.handle(Msg::Settings(crate::panes::settings::SettingsMsg::Open));
            }
            FlowMsg::Action(a, index, text) if !self.busy => match a {
                127 | 131 | 134 | 138 | 139 => {
                    let target = match a {
                        131 => ImageTarget::Scene(self.settings.scene),
                        139 => ImageTarget::SceneAngles(self.settings.scene),
                        127 => ImageTarget::Sheet(index.max(0) as usize),
                        138 => ImageTarget::BackgroundSheet(index.max(0) as usize),
                        _ => ImageTarget::Preview(index.max(0) as usize),
                    };
                    if let Err(error) = self.start_visual(target, studio) {
                        self.message = error;
                    }
                }
                128 | 129 | 132 => {
                    let index = index.max(0) as usize;
                    let selected = text.parse::<usize>().unwrap_or(usize::MAX);
                    let result = (|| -> Result<(), String> {
                        let item = self
                            .gallery(index)
                            .get(selected)
                            .ok_or("Không tìm thấy ảnh trong thư viện")?
                            .clone();
                        if a == 128 {
                            if item.kind == "sheet" {
                                return Err(
                                    "Bộ nhiều góc dùng cho ảnh cảnh. Chọn ảnh đơn làm ảnh chuẩn."
                                        .into(),
                                );
                            }
                            self.store_character_image(index, item.path, "portrait", true)?;
                        } else if a == 129 {
                            let id = self
                                .settings
                                .character_ids
                                .get(index)
                                .ok_or("Không tìm thấy nhân vật")?
                                .clone();
                            visual_state::approve_sheet(
                                self.settings
                                    .character_galleries
                                    .get_mut(&id)
                                    .ok_or("Chưa có thư viện ảnh")?,
                                selected,
                            )?;
                            self.invalidate_prompts();
                        } else {
                            #[cfg(not(any(target_os = "android", target_os = "ios")))]
                            opener::open(&item.path).map_err(|e| e.to_string())?;
                        }
                        Ok(())
                    })();
                    self.message = match result {
                        Ok(()) => {
                            "Đã cập nhật lựa chọn ảnh tham chiếu. Các file cũ vẫn được giữ.".into()
                        }
                        Err(e) => e,
                    };
                }
                133 | 136 | 137 => {
                    let result = (|| -> Result<(), String> {
                        let plan = self.plan_mut().ok_or("Chọn một cảnh trước")?;
                        if a == 137 {
                            plan.scene_image = PathBuf::new();
                            plan.image_stale = false;
                            return Ok(());
                        }
                        let selected = text.parse::<usize>().unwrap_or(usize::MAX);
                        let item = plan
                            .image_history
                            .get(selected)
                            .ok_or("Không tìm thấy ảnh cảnh")?
                            .clone();
                        if !item.path.is_file() {
                            return Err("File ảnh đã bị di chuyển".into());
                        }
                        if a == 136 {
                            #[cfg(not(any(target_os = "android", target_os = "ios")))]
                            opener::open(&item.path).map_err(|e| e.to_string())?;
                        } else {
                            for item in &mut plan.image_history {
                                item.approved = false;
                            }
                            plan.image_history[selected].approved = true;
                            plan.scene_image = item.path;
                            plan.image_stale = false;
                            plan.image_status = "Đã duyệt làm khung đầu video".into();
                            plan.edited();
                        }
                        Ok(())
                    })();
                    self.message = match result {
                        Ok(()) => "Đã cập nhật khung đầu; chưa gửi tạo video.".into(),
                        Err(e) => e,
                    };
                }
                121 => {
                    if let Some(account) = studio
                        .settings
                        .flow_accounts
                        .config
                        .accounts
                        .get(index.max(0) as usize)
                    {
                        self.settings.source_account_id = account.id.clone();
                        self.remote_characters.clear();
                        self.remote_project_url.clear();
                        self.remote_account_id.clear();
                        self.credit = false;
                    }
                }
                122 => {
                    let id = if index <= 0 {
                        String::new()
                    } else {
                        studio
                            .settings
                            .flow_accounts
                            .config
                            .accounts
                            .get(index as usize - 1)
                            .map(|a| a.id.clone())
                            .unwrap_or_default()
                    };
                    if let Some(plan) = self.plan_mut() {
                        plan.account_id = id;
                        plan.edited();
                    }
                    self.credit = false;
                }
                123 => {
                    if let Some(plan) = self.plan_mut() {
                        plan.project_url = text;
                        plan.edited();
                    }
                    self.credit = false;
                }
                105 => {
                    let id = self
                        .settings
                        .character_ids
                        .get(index.max(0) as usize)
                        .cloned();
                    if let (Some(id), Some(plan)) = (id, self.plan_mut()) {
                        plan.character_actions.insert(id, text);
                        plan.invalidate();
                    }
                }
                106 => {
                    let ids = self.settings.character_ids.clone();
                    if ids.is_empty() {
                        self.message =
                            "Thêm hoặc trích xuất nhân vật trước khi thêm lời thoại.".into();
                        return;
                    }
                    if let Some(plan) = self.plan_mut() {
                        if plan.dialogue_lines.is_empty() && !plan.dialogue.trim().is_empty() {
                            self.message="Trích xuất nhân vật để phân vai lời thoại cũ, hoặc xóa ô lời thoại cũ trước khi thêm từng câu.".into();
                            return;
                        }
                        let id = if plan.cast_configured {
                            plan.cast
                                .first()
                                .and_then(|&i| ids.get(i))
                                .cloned()
                                .unwrap_or_default()
                        } else {
                            ids[0].clone()
                        };
                        plan.dialogue_lines.push(scene_state::DialogueLine {
                            character_id: id,
                            text: String::new(),
                        });
                        plan.invalidate();
                    }
                }
                107 => {
                    let speaker = text
                        .parse::<usize>()
                        .ok()
                        .and_then(|i| i.checked_sub(1))
                        .and_then(|i| self.settings.character_ids.get(i).cloned());
                    let ids = self.settings.character_ids.clone();
                    if let Some(plan) = self.plan_mut() {
                        if let Some(line) = plan.dialogue_lines.get_mut(index.max(0) as usize) {
                            line.character_id = speaker.clone().unwrap_or_default();
                        }
                        if let Some(id) = speaker {
                            if !plan.cast_configured {
                                plan.cast_ids = Some(ids);
                                plan.cast_configured = true;
                            }
                            let cast = plan.cast_ids.get_or_insert_default();
                            if !cast.contains(&id) {
                                cast.push(id);
                            }
                        }
                        plan.invalidate();
                    }
                }
                108 => {
                    if let Some(plan) = self.plan_mut() {
                        if let Some(line) = plan.dialogue_lines.get_mut(index.max(0) as usize) {
                            line.text = text;
                            plan.invalidate();
                        }
                    }
                }
                109 => {
                    if let Some(plan) = self.plan_mut() {
                        let i = index.max(0) as usize;
                        if i < plan.dialogue_lines.len() {
                            plan.dialogue_lines.remove(i);
                            if plan.dialogue_lines.is_empty() {
                                plan.dialogue.clear();
                            }
                            plan.invalidate();
                        }
                    }
                }
                112 => {
                    self.flow_link_target = (index.max(0) as usize)
                        .checked_sub(1)
                        .and_then(|i| self.settings.character_ids.get(i).cloned())
                        .unwrap_or_default();
                }
                114 => {
                    if let Some(file) = self.files.get(index.max(0) as usize) {
                        let clean = crate::flow_worker::clean_path(file.clone());
                        if !clean.is_file() {
                            self.message = "Video chưa được tạo hoàn tất hoặc file đã bị xóa/di chuyển.".into();
                            return;
                        }
                        let name = clean
                            .file_name()
                            .and_then(|s| s.to_str())
                            .unwrap_or("Video Cảnh");
                        studio.preview_file(&clean.to_string_lossy(), name, &clean);
                    } else {
                        self.message = "Chưa có video cho cảnh này.".into();
                    }
                }
                113 => {
                    if let Some(role) = self.settings.character_roles.get_mut(index.max(0) as usize)
                    {
                        *role = text;
                        self.invalidate_prompts();
                    }
                }
                100 => {
                    let account = match studio
                        .settings
                        .flow_accounts
                        .config
                        .get(&self.settings.source_account_id)
                    {
                        Ok(a) => a.clone(),
                        Err(e) => {
                            self.message = e;
                            return;
                        }
                    };
                    if !valid_project_url(&self.settings.url) {
                        self.message = "Nhập URL dự án Flow hợp lệ trước khi đọc nhân vật.".into();
                        return;
                    }
                    self.remote_characters.clear();
                    self.remote_project_url.clear();
                    self.busy = true;
                    self.active_account = account.id.clone();
                    self.credit = false;
                    self.generation += 1;
                    let g = self.generation;
                    let url = self.settings.url.trim().to_owned();
                    let requested = url.clone();
                    let dir = studio.host.dirs.data.join("google-flow");
                    self.message = "Đang đọc thư viện nhân vật Flow…".into();
                    spawn_in_project(
                        move || {
                            crate::flow_worker::call_account(
                                &dir,
                                &account,
                                "flow_characters_list",
                                json!({"project_url":url}),
                                false,
                            )
                        },
                        move |studio, _, _, result| {
                            studio
                                .handle(Msg::Flow(FlowMsg::CharactersListed(g, requested, result)))
                        },
                    );
                }
                101 => {
                    let account = match studio
                        .settings
                        .flow_accounts
                        .config
                        .get(&self.settings.source_account_id)
                    {
                        Ok(a) => a.clone(),
                        Err(e) => {
                            self.message = e;
                            return;
                        }
                    };
                    let Some(item) = self.remote_characters.get(index.max(0) as usize).cloned()
                    else {
                        return;
                    };
                    if self.settings.url.trim() != self.remote_project_url
                        || account.id != self.remote_account_id
                    {
                        self.message = "Dự án Flow đã đổi. Đọc lại thư viện nhân vật.".into();
                        return;
                    }
                    let url = self.remote_project_url.clone();
                    let id = item.id.clone();
                    let project_id = item.project_id.clone();
                    let dir = studio.host.dirs.data.join("google-flow");
                    self.busy = true;
                    self.active_account = account.id.clone();
                    self.credit = false;
                    self.generation += 1;
                    let g = self.generation;
                    self.message = format!("Đang tải hồ sơ và ảnh của {}…", item.name);
                    spawn_in_project(
                        move || {
                            crate::flow_worker::call_account(
                                &dir,
                                &account,
                                "flow_character_download",
                                json!({"project_url":url,"character_id":id}),
                                false,
                            )
                        },
                        move |studio, _, _, result| {
                            studio.handle(Msg::Flow(FlowMsg::CharacterImported(
                                g, project_id, item.id, result,
                            )))
                        },
                    );
                }
                95 => {
                    let index = index.max(0) as usize;
                    if index >= self.settings.characters.len() {
                        return;
                    }
                    let generation = self.generation;
                    let epoch = crate::host::project_epoch();
                    let profiles = self.settings.character_ids.clone();
                    crate::host::pick_files_then(
                        "Chọn một ảnh tham chiếu nhân vật".into(),
                        Some((
                            "Ảnh PNG / JPEG / WebP".into(),
                            &["png", "jpg", "jpeg", "webp"],
                        )),
                        move |studio, _, _, paths| {
                            if crate::host::project_epoch() != epoch {
                                return;
                            }
                            if let Some(path) = paths.into_iter().next() {
                                studio.handle(Msg::Flow(FlowMsg::ReferencePicked(
                                    generation, index, profiles, path,
                                )));
                            }
                        },
                    );
                }
                90 => self.settings.writing_format = (index.max(0) as usize).min(4),
                91 => self.settings.writing_structure = (index.max(0) as usize).min(3),
                92 => self.settings.writing_scene_count = (index.max(1) as usize).min(12),
                93 => self.settings.writing_audience = text,
                94 => self.settings.writing_goal = text,
                60 => {
                    if let Some(plan) = self.plan_mut() {
                        plan.shot = text;
                        plan.invalidate();
                    }
                }
                61 => {
                    let count = self.settings.characters.len();
                    let ids = self.settings.character_ids.clone();
                    if let Some(plan) = self.plan_mut() {
                        if !plan.cast_configured {
                            plan.cast = (0..count).collect();
                            plan.cast_configured = true;
                        }
                        let i = index.max(0) as usize;
                        if i < count {
                            if plan.cast.contains(&i) {
                                plan.cast.retain(|&v| v != i);
                            } else {
                                plan.cast.push(i);
                            }
                            plan.cast_ids = Some(
                                plan.cast
                                    .iter()
                                    .filter_map(|&i| ids.get(i).cloned())
                                    .collect(),
                            );
                            plan.invalidate();
                        }
                    }
                }
                62 => {
                    if let Some(plan) = self.plan_mut() {
                        plan.lighting = text;
                        plan.invalidate();
                    }
                }
                64 => {
                    let count = self.settings.backgrounds.len();
                    if let Some(plan) = self.plan_mut() {
                        plan.background = if index > 0 && index as usize <= count {
                            Some(index as usize - 1)
                        } else {
                            None
                        };
                        plan.invalidate();
                    }
                }
                65 => {
                    self.background_edit = if index <= 0 {
                        usize::MAX
                    } else {
                        (index as usize - 1).min(self.settings.backgrounds.len().saturating_sub(1))
                    };
                }
                66 => {
                    self.settings
                        .backgrounds
                        .push("Bối cảnh mới: địa điểm, thời gian, ánh sáng và đạo cụ".into());
                    self.background_edit = self.settings.backgrounds.len() - 1;
                }
                67 => {
                    if let Some(background) =
                        self.settings.backgrounds.get_mut(self.background_edit)
                    {
                        *background = text;
                        self.invalidate_prompts();
                    }
                }
                68 => {
                    let removed = self.background_edit;
                    if removed < self.settings.backgrounds.len() {
                        self.settings.backgrounds.remove(removed);
                        if removed < self.settings.background_images.len() {
                            self.settings.background_images.remove(removed);
                        }
                        for plan in &mut self.settings.plans {
                            plan.remove_background(removed);
                        }
                        self.background_edit =
                            removed.min(self.settings.backgrounds.len().saturating_sub(1));
                    }
                }
                70 => self.transfer_path = text,
                71 => {
                    self.remember_prompt();
                    let mut export = self.settings.clone();
                    for plan in &mut export.plans {
                        plan.status.clear();
                        plan.files.clear();
                        plan.selected = false;
                    }
                    let path = self.config_path.with_file_name("flow-storyboard.json");
                    self.message = match serde_json::to_vec_pretty(&export)
                        .map_err(|e| e.to_string())
                        .and_then(|bytes| std::fs::write(&path, bytes).map_err(|e| e.to_string()))
                    {
                        Ok(()) => format!(
                            "Đã xuất {}. JSON lưu đường dẫn ảnh, không đóng gói ảnh.",
                            path.display()
                        ),
                        Err(e) => e,
                    };
                }
                72 => {
                    let loaded = (|| -> Result<Settings, String> {
                        let path = std::path::Path::new(self.transfer_path.trim());
                        if !path.is_file() {
                            return Err("File JSON không tồn tại hoặc đường dẫn không đúng.".into());
                        }
                        if std::fs::metadata(path).map_err(|e| format!("Không đọc được file: {e}"))?.len() > 5_000_000 {
                            return Err("Bộ cảnh vượt 5 MB.".into());
                        }
                        let mut settings: Settings = serde_json::from_slice(
                            &std::fs::read(path).map_err(|e| e.to_string())?,
                        )
                        .map_err(|_| "File JSON bộ cảnh không hợp lệ.".to_owned())?;
                        if settings.scene_items.is_empty() {
                            settings.scene_items = scene_blocks(&settings.screenplay);
                        }
                        if settings.scene_items.is_empty() || settings.scene_items.len() > 500 {
                            return Err("Bộ cảnh cần có từ 1 đến 500 cảnh.".into());
                        }
                        settings
                            .plans
                            .resize_with(settings.scene_items.len(), ScenePlan::default);
                        for plan in &mut settings.plans {
                            plan.status.clear();
                            plan.files.clear();
                            plan.selected = false;
                            plan.cast.retain(|&i| i < settings.characters.len());
                            if plan
                                .background
                                .is_some_and(|i| i >= settings.backgrounds.len())
                            {
                                plan.background = None;
                            }
                        }
                        settings.url = self.settings.url.clone();
                        settings.model = normalize_model(settings.model);
                        settings.duration = settings.duration.min(12);
                        settings.ratio = settings.ratio.min(1);
                        settings.scene = 0;
                        self.write_sibling(
                            "flow-before-import.json",
                            serde_json::to_vec_pretty(&self.settings).map_err(|e| e.to_string())?,
                        )?;
                        Ok(settings)
                    })();
                    match loaded {
                        Ok(settings) => {
                            self.settings = settings;
                            self.background_edit = 0;
                            self.credit = false;
                            self.stop_queue();
                            self.select_scene(0, studio);
                            self.message="Đã nhập bộ cảnh. Bản cũ lưu ở flow-before-import.json; URL Flow hiện tại được giữ nguyên.".into();
                        }
                        Err(e) => self.message = e,
                    }
                }
                79 => self.settings.image_model = text,
                124 => self.settings.image_provider = (index.max(0) as usize).min(1),
                125 => self.settings.flow_image_model = (index.max(0) as usize).min(2),
                126 => self.settings.flow_image_ratio = (index.max(0) as usize).min(4),
                80 | 81 => {
                    let character = a == 80;
                    let index = index.max(0) as usize;
                    let profile = if character {
                        self.settings.characters.get(index)
                    } else {
                        self.settings.backgrounds.get(index)
                    }
                    .cloned()
                    .unwrap_or_default();
                    if profile.trim().is_empty() {
                        self.message =
                            "Nhập hồ sơ nhân vật hoặc bối cảnh trước khi tạo ảnh.".into();
                        return;
                    }
                    let desc = if character {
                        clean_character_visual(&profile)
                    } else {
                        profile
                    };
                    let style = self.settings.style.trim();
                    let prompt = if character {
                        if style.is_empty() {
                            format!(
                                "Full-body character reference portrait, neutral standing pose, plain studio background. Character: {desc}"
                            )
                        } else {
                            format!(
                                "Full-body character reference portrait, neutral standing pose, plain studio background. Character: {desc}. Visual style: {style}"
                            )
                        }
                    } else {
                        if style.is_empty() {
                            format!(
                                "Environment and location reference image without characters or text. Location: {desc}"
                            )
                        } else {
                            format!(
                                "Environment and location reference image without characters or text. Location: {desc}. Visual style: {style}"
                            )
                        }
                    };
                    if self.settings.image_provider == 0 {
                        let target = if character {
                            ImageTarget::Character(index)
                        } else {
                            ImageTarget::Background(index)
                        };
                        if let Err(error) = self.start_visual(target, studio) {
                            self.message = error;
                        }
                        return;
                    }
                    let keys = studio.prefs.gemini_api_key.expose().to_owned();
                    if concat_speech::api_keys::parse(&keys).is_empty() {
                        self.message =
                            "Thêm Gemini API key trong Cài đặt hoặc chọn Google Flow để tạo ảnh."
                                .into();
                        return;
                    }
                    let model = self.settings.image_model.clone();
                    let dir = studio.host.dirs.data.join("google-flow/assets");
                    self.busy = true;
                    self.credit = false;
                    self.generation += 1;
                    let g = self.generation;
                    self.message = "Gemini đang tạo ảnh tham chiếu…".into();
                    spawn_in_project(
                        move || {
                            let image =
                                concat_speech::flow_image::generate(&keys, &model, &prompt)?;
                            std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
                            let id = std::time::SystemTime::now()
                                .duration_since(std::time::UNIX_EPOCH)
                                .map_err(|e| e.to_string())?
                                .as_nanos();
                            let path = dir.join(format!(
                                "reference-{}-{id}.{}",
                                std::process::id(),
                                image.extension
                            ));
                            std::fs::write(&path, image.bytes).map_err(|e| e.to_string())?;
                            Ok(path)
                        },
                        move |studio, _, _, result| {
                            studio.handle(Msg::Flow(FlowMsg::VisualFinished(
                                g,
                                if character {
                                    ImageTarget::Character(index)
                                } else {
                                    ImageTarget::Background(index)
                                },
                                result.map(Some),
                            )))
                        },
                    );
                }
                82 | 83 => {
                    if a == 82 {
                        let id = self
                            .settings
                            .character_ids
                            .get(index.max(0) as usize)
                            .cloned();
                        if let (Some(id), Some(plan)) = (id, self.plan_mut()) {
                            if let Some(cast) = &mut plan.cast_ids {
                                if !cast.contains(&id) {
                                    cast.push(id);
                                }
                            }
                            plan.invalidate();
                        }
                    }
                    let path = if a == 82 {
                        self.settings.character_images.get(index.max(0) as usize)
                    } else {
                        self.settings.background_images.get(index.max(0) as usize)
                    }
                    .filter(|p| p.is_file())
                    .cloned();
                    if let Some(path) = path {
                        let path = path.to_string_lossy();
                        if !self.settings.reference.lines().any(|p| p == path) {
                            if !self.settings.reference.is_empty() {
                                self.settings.reference.push('\n');
                            }
                            self.settings.reference.push_str(&path);
                        }
                        self.remember_prompt();
                        if let Some(plan) = self.plan_mut() {
                            plan.edited();
                        }
                        self.message = "Đã thêm ảnh vào tham chiếu cảnh đang chọn.".into();
                    }
                }
                86 => {
                    for plan in &mut self.settings.plans {
                        if index == 1 && plan.status == "uncertain" {
                            plan.status.clear();
                        }
                        plan.selected = index == 1
                            && !matches!(
                                plan.status.as_str(),
                                "completed" | "running"
                            );
                    }
                }
                50 => {
                    self.settings
                        .plans
                        .resize_with(self.settings.scene_items.len(), ScenePlan::default);
                    if let Some(plan) = self.settings.plans.get_mut(index.max(0) as usize) {
                        plan.selected = !plan.selected;
                    }
                }
                51 => {
                    self.remember_prompt();
                    for i in 0..self.settings.scene_items.len() {
                        if let Err(e) = self.settings.validate_cast(i) {
                            self.message = format!("Cảnh {}: {e}", i + 1);
                            return;
                        }
                    }
                    self.settings
                        .plans
                        .resize_with(self.settings.scene_items.len(), ScenePlan::default);
                    let continuity = (0..self.settings.scene_items.len())
                        .map(|i| self.continuity_for(i))
                        .collect::<Vec<_>>();
                    for (i, (scene, plan)) in self
                        .settings
                        .scene_items
                        .iter()
                        .zip(self.settings.plans.iter_mut())
                        .enumerate()
                    {
                        if (plan.prompt.trim().is_empty() || plan.needs_prompt)
                            && !scene.trim().is_empty()
                        {
                            plan.prompt = synchronized_prompt(
                                &format!("{}\n{}", scene, plan.directions()),
                                &continuity[i],
                                &self.settings.style,
                            );
                            plan.needs_prompt = false;
                            plan.edited();
                        }
                    }
                    let index = self.settings.scene;
                    self.select_scene(index, studio);
                    self.message="Đã ghép prompt còn thiếu hoặc cần cập nhật. Kiểm tra từng cảnh trước khi chạy hàng đợi.".into();
                }
                52 => {
                    self.remember_prompt();
                    if self.settings.plans.iter().any(|p| {
                        p.selected
                            && !matches!(p.status.as_str(), "completed" | "uncertain" | "running")
                            && (p.needs_prompt || p.prompt.trim().is_empty())
                    }) {
                        self.message="Có cảnh đã chọn chưa có prompt hoặc cần cập nhật. Bấm Ghép / cập nhật prompt trước.".into();
                        return;
                    }
                    let selected = self
                        .settings
                        .plans
                        .iter()
                        .enumerate()
                        .filter(|(_, p)| p.queueable())
                        .map(|(i, _)| i)
                        .collect::<Vec<_>>();
                    if selected.is_empty() {
                        self.message="Chọn cảnh chưa chạy và có prompt. Cảnh đã xong hoặc chưa rõ kết quả không tự chạy lại.".into();
                        return;
                    }
                    let config = studio.settings.flow_accounts.config.clone();
                    for &i in &selected {
                        let candidates = match self.scene_candidates(i, &config) {
                            Ok(accounts) => accounts,
                            Err(e) => {
                                self.message = format!("Cảnh {}: {e}", i + 1);
                                return;
                            }
                        };
                        // Validate every route before any paid request is sent.
                        for account in &candidates {
                            if let Err(e) = self.request_for(i, account, true) {
                                self.message = format!("Cảnh {} / {}: {e}", i + 1, account.name);
                                return;
                            }
                        }
                    }
                    self.generation += 1;
                    self.queue_failures = 0;
                    self.blocked_accounts.clear();
                    self.account_cursor = 0;
                    self.queue = selected.into();
                    self.queue_running = true;
                    self.credit = false;
                    for &i in &self.queue {
                        self.settings.plans[i].status = "waiting".into();
                    }
                    if let Err(e) = self.save() {
                        self.stop_queue();
                        self.message = e;
                        return;
                    }
                    self.next_queued(studio);
                }
                55..=58 => {
                    if let Some(plan) = self.plan_mut() {
                        if a == 58 {
                            plan.dialogue_lines.clear();
                        }
                        match a {
                            55 => plan.camera = text,
                            56 => plan.movement = text,
                            57 => plan.sound = text,
                            _ => plan.dialogue = text,
                        }
                        plan.invalidate();
                    }
                }
                59 => {
                    self.remember_prompt();
                    let text = self
                        .settings
                        .plans
                        .iter()
                        .enumerate()
                        .filter(|(_, p)| !p.prompt.trim().is_empty())
                        .map(|(i, p)| format!("Cảnh {}\n{}", i + 1, p.prompt))
                        .collect::<Vec<_>>()
                        .join("\n\n---\n\n");
                    if text.is_empty() {
                        self.message = "Chưa có prompt theo cảnh để xuất.".into();
                        return;
                    }
                    self.message = match self.write_sibling("flow-prompts.txt", text) {
                        Ok(path) => format!("Đã xuất {}", path.display()),
                        Err(e) => e,
                    };
                }
                47 => {
                    let _ = serde_json::to_vec_pretty(&self.settings)
                        .map_err(|e| e.to_string())
                        .and_then(|bytes| {
                            self.write_sibling("flow-before-new-script.json", bytes)
                                .map(|_| ())
                        });
                    self.settings.screenplay.clear();
                    self.settings.scene_items.clear();
                    self.settings.plans.clear();
                    self.settings.characters.clear();
                    self.settings.character_ids.clear();
                    self.settings.character_roles.clear();
                    self.settings.character_images.clear();
                    self.settings.character_galleries.clear();
                    self.settings.character_flow_origins.clear();
                    self.settings.backgrounds.clear();
                    self.settings.background_images.clear();
                    self.settings.brief.clear();
                    self.settings.logline.clear();
                    self.settings.prompt.clear();
                    self.settings.reference.clear();
                    self.settings.scene = 0;
                    self.queue.clear();
                    self.invalidate_prompts();
                    let _ = self.save();
                    self.message = "Đã tạo kịch bản mới thành công. Hãy nhập ý tưởng hoặc dán kịch bản để bắt đầu.".into();
                }
                48 => {
                    let keys = studio.prefs.gemini_api_key.expose().to_owned();
                    if concat_speech::api_keys::parse(&keys).is_empty() {
                        self.message = "Thêm Gemini API key trong Cài đặt trước khi dùng AI.".into();
                        return;
                    }
                    if self.settings.brief.trim().is_empty() {
                        self.message = "Nhập ý tưởng/yêu cầu ở ô trên để AI có thông tin viết kịch bản.".into();
                        return;
                    }
                    let _ = serde_json::to_vec_pretty(&self.settings)
                        .map_err(|e| e.to_string())
                        .and_then(|bytes| {
                            self.write_sibling("flow-before-ai-new-script.json", bytes)
                                .map(|_| ())
                        });
                    self.settings.screenplay.clear();
                    self.settings.scene_items.clear();
                    self.settings.plans.clear();
                    self.settings.characters.clear();
                    self.settings.character_ids.clear();
                    self.settings.character_roles.clear();
                    self.settings.character_images.clear();
                    self.settings.character_galleries.clear();
                    self.settings.character_flow_origins.clear();
                    self.settings.backgrounds.clear();
                    self.settings.background_images.clear();
                    self.settings.logline.clear();
                    self.settings.prompt.clear();
                    self.settings.reference.clear();
                    self.settings.scene = 0;
                    self.queue.clear();
                    self.invalidate_prompts();
                    let _ = self.save();

                    let formats = [
                        "short fictional film",
                        "product advertisement",
                        "factual documentary",
                        "step-by-step tutorial",
                        "children's story",
                    ];
                    let structures = [
                        "opening / development / payoff",
                        "goal / obstacle / attempt / result",
                        "problem / demonstration / solution",
                        "follow the brief's requested structure",
                    ];
                    let context = json!({
                        "brief": self.settings.brief,
                        "screenplay": "",
                        "characters": [],
                        "scene_texts": [],
                        "scene_bindings": [],
                        "continuity_for_selected_scene": "",
                        "background": "",
                        "style": self.settings.style,
                        "selected_scene": 0,
                        "selected_dialogue": "",
                        "dialogue_lines": null,
                        "scene_directions": "",
                        "has_scene_image": false,
                        "scene_seconds": self.settings.duration + 3,
                        "target_scene_count": self.settings.writing_scene_count.clamp(1, 12),
                        "video_format": formats[self.settings.writing_format.min(4)],
                        "story_structure": structures[self.settings.writing_structure.min(3)],
                        "audience": self.settings.writing_audience,
                        "viewer_goal": self.settings.writing_goal
                    });
                    self.busy = true;
                    self.credit = false;
                    self.generation += 1;
                    let g = self.generation;
                    self.message = "Gemini đang viết kịch bản mới…".into();
                    spawn_in_project(
                        move || concat_speech::flow_content::generate(&keys, 41, &context),
                        move |studio, _, _, result| {
                            studio.handle(Msg::Flow(FlowMsg::AiFinished(g, 41, result)))
                        },
                    );
                }
                40 => self.settings.brief = text,
                41..=46 => {
                    let keys = studio.prefs.gemini_api_key.expose().to_owned();
                    if concat_speech::api_keys::parse(&keys).is_empty() {
                        self.message =
                            "Thêm Gemini API key trong Cài đặt trước khi dùng AI.".into();
                        return;
                    }
                    if a == 46 && self.settings.scene_items.is_empty() {
                        self.message =
                            "Nhập kịch bản hoặc thêm cảnh trước khi trích xuất nhân vật.".into();
                        return;
                    }
                    let scene = self
                        .settings
                        .scene_items
                        .get(self.settings.scene)
                        .cloned()
                        .unwrap_or_default();
                    if matches!(a, 44 | 45) && scene.trim().is_empty() {
                        self.message = "Chọn một cảnh có nội dung trước khi viết prompt AI.".into();
                        return;
                    }
                    if matches!(a, 44 | 45) {
                        if let Err(e) = self.settings.validate_cast(self.settings.scene) {
                            self.message = e;
                            return;
                        }
                    }
                    let dialogue = self
                        .settings
                        .plans
                        .get(self.settings.scene)
                        .map(|p| p.dialogue.as_str())
                        .unwrap_or("")
                        .to_owned();
                    if a == 45 && dialogue.trim().is_empty() {
                        self.message =
                            "Nhập lời thoại gốc vào ô Lời thoại của cảnh trước khi trau chuốt."
                                .into();
                        return;
                    }
                    if self.settings.brief.trim().is_empty()
                        && self.settings.screenplay.trim().is_empty()
                    {
                        self.message =
                            "Nhập ý tưởng/yêu cầu hoặc kịch bản để AI có dữ liệu.".into();
                        return;
                    }
                    if let Err(e) = self.save() {
                        self.message = e;
                        return;
                    }
                    if a == 45 {
                        if let Err(e) = serde_json::to_vec_pretty(&self.settings)
                            .map_err(|e| e.to_string())
                            .and_then(|bytes| {
                                self.write_sibling("flow-before-dialogue-ai.json", bytes)
                                    .map(|_| ())
                            })
                        {
                            self.message = e;
                            return;
                        }
                    }
                    let directions = self
                        .settings
                        .plans
                        .get(self.settings.scene)
                        .map(ScenePlan::directions)
                        .unwrap_or_default();
                    let formats = [
                        "short fictional film",
                        "product advertisement",
                        "factual documentary",
                        "step-by-step tutorial",
                        "children's story",
                    ];
                    let structures = [
                        "opening / development / payoff",
                        "goal / obstacle / attempt / result",
                        "problem / demonstration / solution",
                        "follow the brief's requested structure",
                    ];
                    let mut registry = self.settings.character_context();
                    if matches!(a, 44 | 45) {
                        if let (Some(items), Some(plan)) = (
                            registry.as_array_mut(),
                            self.settings.plans.get(self.settings.scene),
                        ) {
                            if plan.cast_configured {
                                items.retain(|item| {
                                    plan.cast_ids
                                        .iter()
                                        .flatten()
                                        .any(|id| item["id"].as_str() == Some(id.as_str()))
                                });
                            }
                        }
                    }
                    let has_scene_image = self
                        .settings
                        .plans
                        .get(self.settings.scene)
                        .is_some_and(|p| !p.scene_image.as_os_str().is_empty() && !p.image_stale);
                    let bindings=self.settings.plans.iter().enumerate().map(|(i,p)|json!({"scene_index":i,"cast":p.cast_ids,"dialogue":p.dialogue_lines,"legacy_dialogue":p.dialogue,"actions":p.character_actions})).collect::<Vec<_>>();
                    let context = json!({"brief":self.settings.brief,"screenplay":self.settings.screenplay,"characters":registry,"scene_texts":self.settings.scene_items,"scene_bindings":bindings,"continuity_for_selected_scene":self.continuity_for(self.settings.scene),"background":self.settings.continuity,"style":self.settings.style,"selected_scene":scene,"selected_dialogue":dialogue,"dialogue_lines":self.settings.plans.get(self.settings.scene).map(|p|&p.dialogue_lines),"scene_directions":directions,"has_scene_image":has_scene_image,"scene_seconds":self.settings.duration+3,"target_scene_count":self.settings.writing_scene_count.clamp(1,12),"video_format":formats[self.settings.writing_format.min(4)],"story_structure":structures[self.settings.writing_structure.min(3)],"audience":self.settings.writing_audience,"viewer_goal":self.settings.writing_goal});
                    self.busy = true;
                    self.credit = false;
                    self.generation += 1;
                    let g = self.generation;
                    self.message = "Gemini đang soạn nội dung…".into();
                    spawn_in_project(
                        move || concat_speech::flow_content::generate(&keys, a, &context),
                        move |studio, _, _, result| {
                            studio.handle(Msg::Flow(FlowMsg::AiFinished(g, a, result)))
                        },
                    );
                }
                20 => {
                    self.remember_prompt();
                    let scenes = scene_blocks(&text);
                    self.settings.plans = scene_state::reconcile(
                        &self.settings.scene_items,
                        &scenes,
                        &self.settings.plans,
                    );
                    self.settings.scene_items = scenes;
                    self.settings.screenplay = text;
                    self.select_scene(0, studio);
                }
                21 => {
                    self.settings.continuity = text;
                    self.invalidate_prompts();
                }
                22 => {
                    self.settings.style = text;
                    self.invalidate_prompts();
                }
                23 => {
                    self.remember_prompt();
                    self.select_scene(index.max(0) as usize, studio);
                }
                24 => {
                    if let Err(e) = self.settings.validate_cast(self.settings.scene) {
                        self.message = e;
                        return;
                    }
                    if let Some(scene) = self
                        .settings
                        .scene_items
                        .get(self.settings.scene)
                        .filter(|s| !s.trim().is_empty())
                    {
                        let continuity = self.continuity_for(self.settings.scene);
                        let directions = self
                            .settings
                            .plans
                            .get(self.settings.scene)
                            .map(ScenePlan::directions)
                            .unwrap_or_default();
                        self.settings.prompt = synchronized_prompt(
                            &format!("{scene}\n{directions}"),
                            &continuity,
                            &self.settings.style,
                        );
                        self.remember_prompt();
                        if let Some(plan) = self.plan_mut() {
                            plan.needs_prompt = false;
                            plan.edited();
                        }
                        self.credit = false;
                        self.message = format!(
                            "Đã ghép prompt cảnh {}. Kiểm tra nội dung rồi bấm Chuẩn bị.",
                            self.settings.scene + 1
                        );
                        if let Err(e) = self.save() {
                            self.message = e;
                        }
                    } else {
                        self.message = "Nhập kịch bản, mỗi cảnh cách nhau một dòng trống.".into();
                    }
                }
                25 => {
                    self.remember_prompt();
                    let mut scenes = self.settings.scene_items.clone();
                    scenes.push(format!(
                        "Cảnh {}: Nhập hành động và lời thoại",
                        scenes.len() + 1
                    ));
                    self.settings.scene = scenes.len() - 1;
                    self.settings.screenplay = scenes.join("\n\n");
                    self.settings.scene_items = scenes;
                    let index = self.settings.scene;
                    self.select_scene(index, studio);
                }
                26 => {
                    let removed = self.settings.scene;
                    let mut scenes = self.settings.scene_items.clone();
                    if self.settings.scene < scenes.len() {
                        scenes.remove(self.settings.scene);
                    }
                    self.settings.scene = self.settings.scene.min(scenes.len().saturating_sub(1));
                    self.settings.screenplay = scenes.join("\n\n");
                    self.settings.scene_items = scenes;
                    if removed < self.settings.plans.len() {
                        self.settings.plans.remove(removed);
                    }
                    let index = self.settings.scene;
                    self.select_scene(index, studio);
                    if self.settings.scene_items.is_empty() {
                        self.settings.prompt.clear();
                        self.settings.reference.clear();
                        self.files.clear();
                    }
                }
                27 => {
                    let mut scenes = self.settings.scene_items.clone();
                    if let Some(scene) = scenes.get_mut(self.settings.scene) {
                        *scene = text;
                    }
                    self.settings.screenplay = scenes.join("\n\n");
                    self.settings.scene_items = scenes;
                    if let Some(plan) = self.plan_mut() {
                        plan.invalidate();
                    }
                }
                28 => self
                    .settings
                    .characters
                    .push("Nhân vật mới: tên, ngoại hình, trang phục, giọng nói".into()),
                29 => {
                    if let Some(character) = self.settings.characters.get_mut(index.max(0) as usize)
                    {
                        *character = text;
                        self.invalidate_prompts();
                    }
                }
                30 => {
                    if (index as usize) < self.settings.characters.len() {
                        let removed_id = self
                            .settings
                            .character_ids
                            .get(index as usize)
                            .cloned()
                            .unwrap_or_default();
                        if self.flow_link_target == removed_id {
                            self.flow_link_target.clear();
                        }
                        self.settings.character_galleries.remove(&removed_id);
                        self.settings.characters.remove(index as usize);
                        let removed = index as usize;
                        if removed < self.settings.character_ids.len() {
                            self.settings.character_ids.remove(removed);
                        }
                        if removed < self.settings.character_roles.len() {
                            self.settings.character_roles.remove(removed);
                        }
                        if removed < self.settings.character_images.len() {
                            self.settings.character_images.remove(removed);
                        }
                        if removed < self.settings.character_flow_origins.len() {
                            self.settings.character_flow_origins.remove(removed);
                        }
                        for plan in &mut self.settings.plans {
                            if let Some(ids) = &mut plan.cast_ids {
                                ids.retain(|id| id != &removed_id);
                            }
                            plan.character_actions.remove(&removed_id);
                            plan.remove_character(removed);
                        }
                        self.invalidate_prompts();
                    }
                }
                2 => {
                    if self.settings.url != text {
                        self.remote_characters.clear();
                        self.remote_project_url.clear();
                    }
                    self.settings.url = text;
                }
                3 => {
                    self.settings.prompt = text;
                    self.remember_prompt();
                    if let Some(plan) = self.plan_mut() {
                        plan.needs_prompt = false;
                        plan.edited();
                    }
                }
                4 => self.settings.model = (index.max(0) as usize).min(1),
                5 => self.settings.duration = (index.max(0) as usize).min(12),
                6 => self.settings.ratio = (index.max(0) as usize).min(1),
                7 => {
                    self.settings.reference = text;
                    self.remember_prompt();
                    if let Some(plan) = self.plan_mut() {
                        plan.edited();
                    }
                }
                8 => self.credit = index == 1,
                10 | 11 | 12 | 13 | 16 | 190 => {
                    if cfg!(any(target_os = "android", target_os = "ios")) {
                        self.message = "Google Flow hiện chỉ hỗ trợ desktop.".into();
                        return;
                    }
                    if a == 190 {
                        self.settings.scene = (index.max(0) as usize).min(self.settings.plans.len().saturating_sub(1));
                    }
                    let tool = match a {
                        10 => "flow_connect",
                        11 => "flow_status",
                        16 => "recent",
                        _ => {
                            if self.settings.video_provider == 1 {
                                "vids_generate_video"
                            } else {
                                "flow_generate_video"
                            }
                        }
                    };
                    let video = matches!(a, 12 | 13 | 190);
                    if video {
                        self.remember_prompt();
                    }
                    let config = studio.settings.flow_accounts.config.clone();
                    if matches!(a, 13 | 190) && crate::flow_worker::rendering_accounts().len() >= config.limit() {
                        self.message="Đã đủ luồng Flow đang chạy. Chờ tác vụ hiện tại hoàn tất; chưa gửi yêu cầu mới.".into();
                        return;
                    }
                    let candidates = if video {
                        self.scene_candidates(self.settings.scene, &config)
                    } else {
                        let id = if a == 16 {
                            self.settings
                                .plans
                                .get(self.settings.scene)
                                .map(|p| p.last_account_id.as_str())
                                .filter(|id| !id.is_empty())
                                .unwrap_or(&self.settings.source_account_id)
                        } else {
                            &self.settings.source_account_id
                        };
                        config.get(id).map(|account| vec![account.clone()])
                    };
                    let candidates = match candidates {
                        Ok(accounts) => accounts,
                        Err(e) => {
                            self.message = e;
                            return;
                        }
                    };
                    let dir = studio.host.dirs.data.join("google-flow");
                    let Some(account) = candidates.into_iter().find(|account| {
                        !studio.settings.flow_accounts.pending.contains(&account.id)
                            && !crate::flow_worker::in_use(&account.state_dir(&dir))
                    }) else {
                        self.message="Tài khoản đang bận hoặc chưa sẵn sàng. Kiểm tra Cài đặt → Google Flow.".into();
                        return;
                    };
                    let args = if video {
                        match self.request_for(self.settings.scene, &account, matches!(a, 13 | 190)) {
                            Ok(args) => args,
                            Err(e) => {
                                self.message = e;
                                return;
                            }
                        }
                    } else if a == 10 {
                        let scene_url = self
                            .settings
                            .plans
                            .get(self.settings.scene)
                            .map(|p| p.project_url.trim())
                            .unwrap_or("");
                        let open_url = [
                            scene_url,
                            self.settings.url.trim(),
                            account.project_url.trim(),
                        ]
                        .into_iter()
                        .find(|u| valid_project_url(u));
                        match open_url {
                            Some(u) => json!({ "open_url": u }),
                            None => json!({}),
                        }
                    } else {
                        json!({})
                    };
                    let old_status = self
                        .settings
                        .plans
                        .get(self.settings.scene)
                        .map(|p| p.status.clone());
                    if video {
                        if let Some(plan) = self.plan_mut() {
                            plan.status = "running".into();
                            plan.last_account_id = account.id.clone();
                            self.active_scene = Some((self.settings.scene, matches!(a, 13 | 190)));
                        }
                    }
                    if let Err(e) = self.save() {
                        if let (Some(status), Some(plan)) = (old_status, self.plan_mut()) {
                            plan.status = status;
                        }
                        self.active_scene = None;
                        self.message = e;
                        return;
                    }
                    self.active_account = account.id.clone();
                    self.busy = true;
                    self.generation += 1;
                    let g = self.generation;
                    self.credit = false;
                    self.message = if a == 190 {
                        format!("Đang tạo lại video cho Cảnh {} trong Google Flow… Có thể mất nhiều phút.", self.settings.scene + 1)
                    } else if a == 13 {
                        "Đang tạo video trong Google Flow… Có thể mất nhiều phút. Không gửi lại nếu chưa rõ kết quả.".into()
                    } else {
                        "Đang xử lý trong Chrome…".into()
                    };
                    spawn_in_project(
                        move || {
                            crate::flow_worker::call_account(&dir, &account, tool, args, matches!(a, 13 | 190))
                        },
                        move |studio, _, _, result| {
                            studio.handle(Msg::Flow(FlowMsg::Finished(g, result)))
                        },
                    );
                }
                14 => {
                    if studio.session.is_none() {
                        self.message = "Mở dự án Concat để nhập clip vào thư viện.".into();
                        return;
                    }
                    if let Some(file) = self.files.get(index.max(0) as usize).cloned() {
                        studio.handle(Msg::Media(MediaMsg::Import(vec![file])));
                        self.message = "Đang nhập clip vào thư viện dự án…".into();
                    }
                }
                15 => {
                    if let Some(file) = self.files.get(index.max(0) as usize) {
                        let clean = crate::flow_worker::clean_path(file.clone());
                        if !clean.is_file() {
                            self.message = "File video chưa tồn tại hoặc đã bị xóa/di chuyển.".into();
                            return;
                        }
                        #[cfg(not(any(target_os = "android", target_os = "ios")))]
                        if let Err(e) = opener::reveal(&clean) {
                            self.message = format!("Không mở được thư mục chứa video: {e}");
                        }
                    } else {
                        self.message = "Chưa có video cho cảnh này.".into();
                    }
                }
                140 => {
                    let file = self.files.get(index.max(0) as usize).cloned();
                    let scene_idx = self.settings.scene;
                    let account_id = self.settings.plans.get(scene_idx)
                        .map(|p| p.last_account_id.as_str())
                        .filter(|id| !id.is_empty())
                        .unwrap_or(&self.settings.source_account_id);
                    let account = match studio.settings.flow_accounts.config.get(account_id) {
                        Ok(acc) => acc.clone(),
                        Err(e) => {
                            self.message = format!("Không tìm thấy tài khoản Flow: {e}");
                            return;
                        }
                    };
                    let project_url = self.scene_project_url(scene_idx, &account).unwrap_or_else(|_| self.settings.url.clone());
                    if !valid_project_url(&project_url) {
                        self.message = "URL dự án Flow không hợp lệ để upscale 1080p.".into();
                        return;
                    }
                    let media_id = file.as_ref().and_then(|p| {
                        let stem = p.file_stem()?.to_str()?;
                        stem.strip_prefix("flow_").and_then(|s| s.split('_').next()).map(|s| s.to_string())
                    }).unwrap_or_default();

                    let dir = studio.host.dirs.data.join("google-flow");
                    self.busy = true;
                    self.message = "Đang gửi yêu cầu Upscale 1080p lên Google Flow Cloud...".into();
                    spawn_in_project(
                        move || {
                            let args = serde_json::json!({
                                "project_url": project_url,
                                "media_id": media_id,
                                "auto_confirm": true,
                            });
                            crate::flow_worker::call_account(&dir, &account, "flow_upscale_video", args, true)
                        },
                        move |studio, _, _, result| {
                            studio.handle(Msg::Flow(FlowMsg::Action(
                                149,
                                0,
                                match result {
                                    Ok(val) => {
                                        if let Some(arr) = val.get("files").and_then(|v| v.as_array()) {
                                            if let Some(first) = arr.first().and_then(|f| f.as_str()) {
                                                format!("OK:{first}")
                                            } else {
                                                "ERR:Không nhận được file 1080p sau khi upscale.".into()
                                            }
                                        } else {
                                            "ERR:Lỗi định dạng phản hồi từ Flow.".into()
                                        }
                                    }
                                    Err(e) => format!("ERR:{e}"),
                                },
                            )));
                        },
                    );
                }
                149 => {
                    self.busy = false;
                    if let Some(rest) = text.strip_prefix("OK:") {
                        let path = PathBuf::from(rest);
                        let _ = post_process::extract_thumbnail(&path);
                        if let Some(plan) = self.plan_mut() {
                            if !plan.files.contains(&path) {
                                plan.files.insert(0, path.clone());
                            }
                        }
                        if !self.files.contains(&path) {
                            self.files.insert(0, path);
                        }
                        self.message = "Đã hoàn tất nâng nét 1080p từ Google Flow!".into();
                    } else if let Some(err) = text.strip_prefix("ERR:") {
                        self.message = err.to_owned();
                    }
                }
                143 => {
                    if let Some(plan) = self.plan_mut() {
                        plan.scene_flow_auto = !plan.scene_flow_auto;
                        let state = if plan.scene_flow_auto { "Bật" } else { "Tắt" };
                        self.message = format!("{state} Scene Flow tự động nối cảnh.");
                    }
                }
                144 => {
                    if let Some(plan) = self.plan_mut() {
                        plan.visual_mode = (index.max(0) as usize).min(1);
                        plan.invalidate();
                    }
                }
                145 => {
                    if text == "clear" {
                        if let Some(plan) = self.plan_mut() {
                            plan.end_image = PathBuf::new();
                            plan.invalidate();
                            self.message = "Đã bỏ chọn End Frame.".into();
                        }
                        return;
                    }
                    let scene = self.settings.scene;
                    let epoch = crate::host::project_epoch();
                    crate::host::pick_files_then(
                        "Chọn ảnh kết thúc (End Frame)".into(),
                        Some(("Ảnh PNG / JPEG / WebP".into(), &["png", "jpg", "jpeg", "webp"])),
                        move |studio, _, _, paths| {
                            if crate::host::project_epoch() != epoch {
                                return;
                            }
                            if let Some(path) = paths.into_iter().next() {
                                studio.handle(Msg::Flow(FlowMsg::Action(
                                    148,
                                    scene as i32,
                                    path.to_string_lossy().to_string(),
                                )));
                            }
                        },
                    );
                }
                148 => {
                    let scene_idx = index.max(0) as usize;
                    if let Some(plan) = self.settings.plans.get_mut(scene_idx) {
                        plan.end_image = PathBuf::from(text);
                        plan.invalidate();
                        self.message = "Đã chọn ảnh kết thúc (End Frame).".into();
                    }
                }
                146 => {
                    self.settings.video_provider = (index.max(0) as usize).min(1);
                    self.invalidate_prompts();
                }
                147 => {
                    if self.settings.scene == 0 {
                        self.message = "Cảnh 1 không có cảnh trước đó để lấy frame.".into();
                        return;
                    }
                    let prev_idx = self.settings.scene - 1;
                    let prev_file = self
                        .settings
                        .plans
                        .get(prev_idx)
                        .and_then(|p| p.files.first())
                        .cloned();
                    let Some(prev_file) = prev_file else {
                        self.message = format!(
                            "Cảnh {} chưa có video hoàn tất để trích xuất frame.",
                            prev_idx + 1
                        );
                        return;
                    };
                    let root = studio.host.dirs.data.join("google-flow");
                    let out_frame = root.join(format!(
                        "output/last-frame-scene-{}.png",
                        self.settings.scene + 1
                    ));
                    match post_process::extract_last_frame(&prev_file, &out_frame) {
                        Ok(frame) => {
                            if let Some(plan) = self.plan_mut() {
                                plan.scene_image = frame;
                                plan.image_status = "Đã lấy frame cuối cảnh trước".into();
                                self.message = format!(
                                    "Đã lấy frame cuối từ Cảnh {} làm khung đầu Cảnh {}.",
                                    prev_idx + 1,
                                    self.settings.scene + 1
                                );
                            }
                        }
                        Err(e) => {
                            self.message = e;
                        }
                    }
                }
                150 => {
                    let continuity = (0..self.settings.plans.len())
                        .map(|i| self.continuity_for(i))
                        .collect::<Vec<_>>();
                    for (i, plan) in self.settings.plans.iter_mut().enumerate() {
                        if plan.needs_prompt || plan.prompt.trim().is_empty() {
                            let scene_text = plan.directions();
                            plan.prompt = synchronized_prompt(
                                &scene_text,
                                continuity.get(i).map(String::as_str).unwrap_or(""),
                                &self.settings.style,
                            );
                            plan.needs_prompt = false;
                        }
                        if plan.status != "completed" {
                            plan.selected = true;
                            if plan.status == "uncertain" {
                                plan.status.clear();
                            }
                        }
                    }
                    self.remember_prompt();
                    let selected = self
                        .settings
                        .plans
                        .iter()
                        .enumerate()
                        .filter(|(_, p)| p.queueable())
                        .map(|(i, _)| i)
                        .collect::<Vec<_>>();
                    if selected.is_empty() {
                        self.message = "Tất cả các cảnh đã hoàn tất! Xem ở tab Kết quả.".into();
                        return;
                    }
                    self.queue = selected.into();
                    self.queue_running = true;
                    self.next_queued(studio);
                    self.message = "⚡ Đang tự động tạo toàn bộ video theo kịch bản...".into();
                }
                153 => {
                    self.settings.auto_import_timeline = !self.settings.auto_import_timeline;
                    let state = if self.settings.auto_import_timeline { "Bật" } else { "Tắt" };
                    self.message = format!("{state} tự động đưa clip vào Timeline sau khi hoàn tất.");
                }
                160 => {
                    let project_url = self.effective_project_url(studio);
                    if self.settings.url.trim().is_empty() && !project_url.is_empty() {
                        self.settings.url = project_url;
                    }
                    let keys = studio.prefs.gemini_api_key.expose().to_owned();
                    if concat_speech::api_keys::parse(&keys).is_empty() {
                        self.message = "Thêm Gemini API key trong Cài đặt trước khi dùng AI.".into();
                        return;
                    }
                    if self.settings.brief.trim().is_empty()
                        && self.settings.screenplay.trim().is_empty()
                        && self.settings.characters.is_empty()
                    {
                        self.message = "Nhập kịch bản, ý tưởng hoặc thêm nhân vật để AI có dữ liệu tạo Agent instructions.".into();
                        return;
                    }
                    let registry = self.settings.character_context();
                    let context = json!({
                        "brief": self.settings.brief,
                        "screenplay": self.settings.screenplay,
                        "characters": registry,
                        "style": self.settings.style,
                        "continuity": self.settings.continuity,
                    });
                    self.busy = true;
                    self.credit = false;
                    self.generation += 1;
                    let g = self.generation;
                    self.message = "Gemini đang thiết kế Agent instructions (quy tắc nhân vật & góc máy)...".into();
                    spawn_in_project(
                        move || concat_speech::flow_content::generate(&keys, 49, &context),
                        move |studio, _, _, result| {
                            studio.handle(Msg::Flow(FlowMsg::AiFinished(g, 49, result)))
                        },
                    );
                }
                161 => {
                    if cfg!(any(target_os = "android", target_os = "ios")) {
                        self.message = "Google Flow hiện chỉ hỗ trợ desktop.".into();
                        return;
                    }
                    let dir = studio.host.dirs.data.join("google-flow");
                    let account = if let Ok(a) = studio.settings.flow_accounts.config.get(&self.settings.source_account_id) {
                        a.clone()
                    } else if let Some(a) = studio.settings.flow_accounts.config.accounts.iter().find(|a| a.enabled) {
                        self.settings.source_account_id = a.id.clone();
                        a.clone()
                    } else {
                        self.message = "Chưa có tài khoản Flow nào được bật. Hãy cấu hình trong Cài đặt.".into();
                        return;
                    };
                    self.active_account = account.id.clone();
                    self.busy = true;
                    self.generation += 1;
                    let g = self.generation;
                    self.message = "Đang tạo dự án Flow mới trong Chrome...".into();
                    spawn_in_project(
                        move || {
                            crate::flow_worker::call_account(&dir, &account, "flow_project_create", json!({}), false)
                        },
                        move |studio, _, _, result| {
                            studio.handle(Msg::Flow(FlowMsg::Finished(g, result)))
                        },
                    );
                }
                170 => {
                    let user_prompt = text.trim();
                    if user_prompt.is_empty() {
                        self.message = "Vui lòng nhập yêu cầu sửa đổi cho Trợ lý AI.".into();
                        return;
                    }
                    let keys = studio.prefs.gemini_api_key.expose().to_owned();
                    if concat_speech::api_keys::parse(&keys).is_empty() {
                        self.message = "Thêm Gemini API key trong Cài đặt trước khi dùng AI.".into();
                        return;
                    }
                    if let Ok(bytes) = serde_json::to_vec_pretty(&self.settings) {
                        let _ = self.write_sibling("flow-before-copilot.json", bytes);
                    }
                    let active_plan = self.settings.plans.get(self.settings.scene);
                    let selected_scene_patch = active_plan.map(|p| json!({
                        "shot": p.shot,
                        "camera": p.camera,
                        "movement": p.movement,
                        "lighting": p.lighting,
                        "sound": p.sound,
                        "dialogue": p.dialogue,
                    })).unwrap_or(json!({}));

                    let scenes_texts: Vec<String> = self.settings.scene_items.clone();
                    let registry = self.settings.character_context();
                    let context = json!({
                        "instruction": user_prompt,
                        "active_tab": index,
                        "screenplay": self.settings.screenplay,
                        "scenes": scenes_texts,
                        "characters": registry,
                        "selected_scene": self.settings.scene,
                        "selected_scene_patch": selected_scene_patch,
                        "background": self.settings.continuity,
                        "style": self.settings.style,
                        "target_scene_count": self.settings.scene_items.len().max(1),
                    });
                    self.busy = true;
                    self.credit = false;
                    self.generation += 1;
                    let g = self.generation;
                    self.message = "Trợ lý AI đang phân tích và thực hiện sửa đổi theo yêu cầu...".into();
                    spawn_in_project(
                        move || concat_speech::flow_content::generate(&keys, 50, &context),
                        move |studio, _, _, result| {
                            studio.handle(Msg::Flow(FlowMsg::AiFinished(g, 50, result)))
                        },
                    );
                }
                171 => {
                    let path = self.config_path.with_file_name("flow-before-copilot.json");
                    match std::fs::read(&path) {
                        Ok(bytes) => {
                            match serde_json::from_slice::<Settings>(&bytes) {
                                Ok(restored) => {
                                    self.settings = restored;
                                    self.settings.sync_character_ids();
                                    let _ = self.save();
                                    self.message = "Đã hoàn tác thay đổi gần nhất của Trợ lý AI.".into();
                                }
                                Err(e) => self.message = format!("Không khôi phục được bản sao lưu: {e}"),
                            }
                        }
                        Err(e) => self.message = format!("Chưa có bản sao lưu trước đó để hoàn tác: {e}"),
                    }
                }
                172 => {
                    self.copilot_prompt = text;
                }
                173 => {
                    self.copilot_collapsed = !self.copilot_collapsed;
                }
                175 => {
                    let text = text.trim();
                    if text.is_empty() {
                        self.message = "Vui lòng nhập yêu cầu chỉnh sửa cho AI.".into();
                        return;
                    }
                    let keys = studio.prefs.gemini_api_key.expose().to_owned();
                    if concat_speech::api_keys::parse(&keys).is_empty() {
                        self.message = "Thêm Gemini API key trong Cài đặt trước khi dùng AI.".into();
                        return;
                    }

                    let (sub_idx, instruction) = if let Some((idx_str, instr)) = text.split_once(":::") {
                        (idx_str.parse::<usize>().unwrap_or(0), instr.trim())
                    } else if let Some((idx_str, instr)) = text.split_once('\t') {
                        (idx_str.parse::<usize>().unwrap_or(0), instr.trim())
                    } else {
                        (0, text)
                    };

                    if instruction.is_empty() {
                        self.message = "Vui lòng nhập yêu cầu chỉnh sửa cho AI.".into();
                        return;
                    }

                    if let Ok(bytes) = serde_json::to_vec_pretty(&self.settings) {
                        let _ = self.write_sibling("flow-before-copilot.json", bytes);
                    }

                    self.ai_target_kind = index;
                    self.ai_sub_idx = sub_idx;

                    let (field_label, current_value, extra_context) = match index {
                        40 => (
                            "Ý tưởng / tóm tắt kịch bản",
                            self.settings.brief.clone(),
                            json!({
                                "writing_format": self.settings.writing_format,
                                "writing_audience": self.settings.writing_audience,
                                "writing_goal": self.settings.writing_goal,
                            }),
                        ),
                        93 => (
                            "Đối tượng người xem",
                            self.settings.writing_audience.clone(),
                            json!({
                                "brief": self.settings.brief,
                                "goal": self.settings.writing_goal,
                            }),
                        ),
                        94 => (
                            "Mục tiêu / thông điệp",
                            self.settings.writing_goal.clone(),
                            json!({
                                "brief": self.settings.brief,
                                "audience": self.settings.writing_audience,
                            }),
                        ),
                        20 => (
                            "Kịch bản tổng thể các cảnh",
                            self.settings.screenplay.clone(),
                            json!({
                                "brief": self.settings.brief,
                                "scene_count": self.settings.scene_items.len(),
                                "style": self.settings.style,
                            }),
                        ),
                        29 => {
                            let profile = self.settings.characters.get(sub_idx).cloned().unwrap_or_default();
                            let role = self.settings.character_roles.get(sub_idx).cloned().unwrap_or_default();
                            (
                                "Hồ sơ nhân vật",
                                profile,
                                json!({
                                    "role": role,
                                    "brief": self.settings.brief,
                                }),
                            )
                        }
                        113 => {
                            let role = self.settings.character_roles.get(sub_idx).cloned().unwrap_or_default();
                            let profile = self.settings.characters.get(sub_idx).cloned().unwrap_or_default();
                            (
                                "Vai trò nhân vật",
                                role,
                                json!({
                                    "character_profile": profile,
                                    "brief": self.settings.brief,
                                }),
                            )
                        }
                        67 => {
                            let bg = self.settings.backgrounds.get(self.background_edit).cloned().unwrap_or_default();
                            (
                                "Mô tả bối cảnh / không gian",
                                bg,
                                json!({
                                    "continuity": self.settings.continuity,
                                    "style": self.settings.style,
                                    "brief": self.settings.brief,
                                }),
                            )
                        }
                        21 => (
                            "Bối cảnh & tính liên tục",
                            self.settings.continuity.clone(),
                            json!({
                                "brief": self.settings.brief,
                                "style": self.settings.style,
                            }),
                        ),
                        22 => (
                            "Phong cách hình ảnh, ánh sáng và âm thanh chung",
                            self.settings.style.clone(),
                            json!({
                                "brief": self.settings.brief,
                            }),
                        ),
                        27 => {
                            let scene_txt = self.settings.scene_items.get(self.settings.scene).cloned().unwrap_or_default();
                            (
                                "Nội dung phân cảnh",
                                scene_txt,
                                json!({
                                    "scene_index": self.settings.scene + 1,
                                    "brief": self.settings.brief,
                                    "screenplay": self.settings.screenplay,
                                }),
                            )
                        }
                        105 => {
                            let char_name = self.settings.characters.get(sub_idx).cloned().unwrap_or_default();
                            let char_id = self.settings.character_ids.get(sub_idx).cloned().unwrap_or_default();
                            let plan = self.settings.plans.get(self.settings.scene);
                            let action = plan.and_then(|p| p.character_actions.get(&char_id)).cloned().unwrap_or_default();
                            (
                                "Hành động / cảm xúc nhân vật trong cảnh",
                                action,
                                json!({
                                    "character_name": char_name,
                                    "scene_index": self.settings.scene + 1,
                                    "scene_text": self.settings.scene_items.get(self.settings.scene),
                                }),
                            )
                        }
                        60 => {
                            let plan = self.settings.plans.get(self.settings.scene);
                            (
                                "Cỡ cảnh (Shot)",
                                plan.map(|p| p.shot.clone()).unwrap_or_default(),
                                json!({
                                    "scene_index": self.settings.scene + 1,
                                    "scene_text": self.settings.scene_items.get(self.settings.scene),
                                }),
                            )
                        }
                        55 => {
                            let plan = self.settings.plans.get(self.settings.scene);
                            (
                                "Góc máy (Camera angle)",
                                plan.map(|p| p.camera.clone()).unwrap_or_default(),
                                json!({
                                    "scene_index": self.settings.scene + 1,
                                    "scene_text": self.settings.scene_items.get(self.settings.scene),
                                }),
                            )
                        }
                        56 => {
                            let plan = self.settings.plans.get(self.settings.scene);
                            (
                                "Chuyển động máy (Camera movement)",
                                plan.map(|p| p.movement.clone()).unwrap_or_default(),
                                json!({
                                    "scene_index": self.settings.scene + 1,
                                    "scene_text": self.settings.scene_items.get(self.settings.scene),
                                }),
                            )
                        }
                        62 => {
                            let plan = self.settings.plans.get(self.settings.scene);
                            (
                                "Ánh sáng (Lighting)",
                                plan.map(|p| p.lighting.clone()).unwrap_or_default(),
                                json!({
                                    "scene_index": self.settings.scene + 1,
                                    "scene_text": self.settings.scene_items.get(self.settings.scene),
                                }),
                            )
                        }
                        57 => {
                            let plan = self.settings.plans.get(self.settings.scene);
                            (
                                "Âm thanh / SFX / Nhạc nền",
                                plan.map(|p| p.sound.clone()).unwrap_or_default(),
                                json!({
                                    "scene_index": self.settings.scene + 1,
                                    "scene_text": self.settings.scene_items.get(self.settings.scene),
                                }),
                            )
                        }
                        108 => {
                            let plan = self.settings.plans.get(self.settings.scene);
                            let line = plan.and_then(|p| p.dialogue_lines.get(sub_idx));
                            let speaker_id = line.map(|l| l.character_id.clone()).unwrap_or_default();
                            let speaker_name = self.settings.character_ids.iter().position(|id| id == &speaker_id).and_then(|pos| self.settings.characters.get(pos)).cloned().unwrap_or_else(|| "Nhân vật".to_string());
                            let text = line.map(|l| l.text.clone()).unwrap_or_default();
                            (
                                "Câu thoại nhân vật",
                                text,
                                json!({
                                    "speaker": speaker_name,
                                    "scene_index": self.settings.scene + 1,
                                    "scene_text": self.settings.scene_items.get(self.settings.scene),
                                }),
                            )
                        }
                        58 => {
                            let plan = self.settings.plans.get(self.settings.scene);
                            (
                                "Lời thoại",
                                plan.map(|p| p.dialogue.clone()).unwrap_or_default(),
                                json!({
                                    "scene_index": self.settings.scene + 1,
                                    "scene_text": self.settings.scene_items.get(self.settings.scene),
                                }),
                            )
                        }
                        3 => (
                            "Prompt tạo video",
                            self.settings.prompt.clone(),
                            json!({
                                "scene_index": self.settings.scene + 1,
                                "scene_text": self.settings.scene_items.get(self.settings.scene),
                                "style": self.settings.style,
                            }),
                        ),
                        _ => {
                            self.message = "Mục chỉnh sửa không hợp lệ.".into();
                            return;
                        }
                    };

                    let context = json!({
                        "field_label": field_label,
                        "current_value": current_value,
                        "instruction": instruction,
                        "context": extra_context,
                    });

                    self.busy = true;
                    self.credit = false;
                    self.generation += 1;
                    let g = self.generation;
                    self.message = format!("AI đang chỉnh sửa {field_label}...").into();
                    spawn_in_project(
                        move || concat_speech::flow_content::generate(&keys, 51, &context),
                        move |studio, _, _, result| {
                            studio.handle(Msg::Flow(FlowMsg::AiFinished(g, 51, result)))
                        },
                    );
                }
                180 => {
                    let index = index.max(0) as usize;
                    if let Some(profile) = self.settings.characters.get(index) {
                        let role = self.settings.character_roles.get(index).cloned().unwrap_or_default();
                        let labels = self.settings.character_labels();
                        let display_name = labels.get(index).cloned().unwrap_or_else(|| format!("Nhân vật {}", index + 1));
                        let root = studio.host.dirs.data.join("google-flow");
                        let mut presets = CharacterPreset::list(&root);

                        // Copy character image permanently if it exists
                        let mut perm_image_path = String::new();
                        if let Some(src_img) = self.settings.character_images.get(index).filter(|p| p.is_file()) {
                            let presets_img_dir = root.join("presets").join("characters");
                            let _ = std::fs::create_dir_all(&presets_img_dir);
                            let ext = src_img.extension().and_then(|s| s.to_str()).unwrap_or("png");
                            let safe_name: String = display_name
                                .chars()
                                .map(|c| if c.is_alphanumeric() { c } else { '_' })
                                .collect();
                            let timestamp = std::time::SystemTime::now()
                                .duration_since(std::time::UNIX_EPOCH)
                                .unwrap_or_default()
                                .as_millis();
                            let file_name = format!("char_{safe_name}_{timestamp}.{ext}");
                            let target_path = presets_img_dir.join(&file_name);
                            if std::fs::copy(src_img, &target_path).is_ok() {
                                perm_image_path = target_path.to_string_lossy().to_string();
                            } else {
                                perm_image_path = src_img.to_string_lossy().to_string();
                            }
                        }

                        let flow_id = self.settings.character_ids.get(index).cloned().unwrap_or_default();
                        let flow_origin = self.settings.character_flow_origins.get(index).cloned();
                        let id = format!(
                            "preset_{}",
                            std::time::SystemTime::now()
                                .duration_since(std::time::UNIX_EPOCH)
                                .unwrap_or_default()
                                .as_millis()
                        );

                        if let Some(existing) = presets.iter_mut().find(|p| p.name.eq_ignore_ascii_case(&display_name)) {
                            existing.role = role;
                            existing.profile = profile.clone();
                            if !perm_image_path.is_empty() {
                                existing.image_path = perm_image_path;
                            }
                            existing.flow_id = flow_id;
                            existing.flow_origin = flow_origin;
                            self.message = format!("Đã cập nhật nhân vật '{}' trong Kho Preset!", display_name);
                        } else {
                            presets.insert(0, CharacterPreset {
                                id,
                                name: display_name.clone(),
                                role,
                                profile: profile.clone(),
                                image_path: perm_image_path,
                                flow_id,
                                flow_origin,
                            });
                            self.message = format!("Đã lưu nhân vật '{}' vào Kho Preset dùng chung!", display_name);
                        }

                        if let Err(e) = CharacterPreset::save_all(&root, &presets) {
                            self.message = format!("Lỗi lưu Preset: {e}");
                        }
                    }
                }
                181 => {
                    let index = index.max(0) as usize;
                    let root = studio.host.dirs.data.join("google-flow");
                    let presets = CharacterPreset::list(&root);
                    if let Some(preset) = presets.get(index) {
                        let new_idx = self.settings.characters.len();
                        self.settings.characters.push(preset.profile.clone());
                        self.settings.character_roles.push(preset.role.clone());
                        let img = if !preset.image_path.is_empty() {
                            PathBuf::from(&preset.image_path)
                        } else {
                            PathBuf::new()
                        };
                        self.settings.character_images.push(img);
                        let char_id = if !preset.flow_id.is_empty() && !preset.flow_id.starts_with("preset_") {
                            preset.flow_id.clone()
                        } else {
                            format!("char-{}", new_idx + 1)
                        };
                        self.settings.character_ids.push(char_id);
                        while self.settings.character_flow_origins.len() < self.settings.characters.len() - 1 {
                            self.settings.character_flow_origins.push(FlowCharacterOrigin::default());
                        }
                        self.settings.character_flow_origins.push(preset.flow_origin.clone().unwrap_or_default());
                        self.settings.sync_character_ids();

                        self.invalidate_prompts();
                        if let Err(e) = self.save() {
                            log::warn!("flow: save failed: {e}");
                        }
                        self.message = format!("Đã thêm nhân vật '{}' từ Kho Preset vào kịch bản!", preset.name);
                    }
                }
                182 => {
                    let index = index.max(0) as usize;
                    let root = studio.host.dirs.data.join("google-flow");
                    let mut presets = CharacterPreset::list(&root);
                    if index < presets.len() {
                        let removed = presets.remove(index);
                        if !removed.image_path.is_empty() {
                            let p = PathBuf::from(&removed.image_path);
                            if p.is_file() && p.starts_with(root.join("presets")) {
                                let _ = std::fs::remove_file(p);
                            }
                        }
                        if let Err(e) = CharacterPreset::save_all(&root, &presets) {
                            self.message = format!("Lỗi lưu Preset: {e}");
                        } else {
                            self.message = format!("Đã xóa nhân vật '{}' khỏi Kho Preset.", removed.name);
                        }
                    }
                }
                185 => {
                    let index = index.max(0) as usize;
                    if let Some(desc) = self.settings.backgrounds.get(index) {
                        let root = studio.host.dirs.data.join("google-flow");
                        let mut presets = ScenePreset::list(&root);

                        // Generate display name from first line or snippet
                        let first_line = desc.lines().next().unwrap_or("").trim();
                        let display_name = if !first_line.is_empty() {
                            if first_line.chars().count() > 30 {
                                format!("{}...", first_line.chars().take(28).collect::<String>())
                            } else {
                                first_line.to_string()
                            }
                        } else {
                            format!("Bối cảnh {}", index + 1)
                        };

                        // Copy background image permanently if it exists
                        let mut perm_image_path = String::new();
                        if let Some(src_img) = self.settings.background_images.get(index).filter(|p| p.is_file()) {
                            let presets_img_dir = root.join("presets").join("scenes");
                            let _ = std::fs::create_dir_all(&presets_img_dir);
                            let ext = src_img.extension().and_then(|s| s.to_str()).unwrap_or("png");
                            let safe_name: String = display_name
                                .chars()
                                .map(|c| if c.is_alphanumeric() { c } else { '_' })
                                .collect();
                            let timestamp = std::time::SystemTime::now()
                                .duration_since(std::time::UNIX_EPOCH)
                                .unwrap_or_default()
                                .as_millis();
                            let file_name = format!("scene_{safe_name}_{timestamp}.{ext}");
                            let target_path = presets_img_dir.join(&file_name);
                            if std::fs::copy(src_img, &target_path).is_ok() {
                                perm_image_path = target_path.to_string_lossy().to_string();
                            } else {
                                perm_image_path = src_img.to_string_lossy().to_string();
                            }
                        }

                        let id = format!(
                            "preset_scene_{}",
                            std::time::SystemTime::now()
                                .duration_since(std::time::UNIX_EPOCH)
                                .unwrap_or_default()
                                .as_millis()
                        );
                        let style = self.settings.style.clone();

                        if let Some(existing) = presets.iter_mut().find(|p| p.name.eq_ignore_ascii_case(&display_name)) {
                            existing.description = desc.clone();
                            existing.style = style;
                            if !perm_image_path.is_empty() {
                                existing.image_path = perm_image_path;
                            }
                            self.message = format!("Đã cập nhật bối cảnh '{}' trong Kho Preset!", display_name);
                        } else {
                            presets.insert(0, ScenePreset {
                                id,
                                name: display_name.clone(),
                                description: desc.clone(),
                                style,
                                image_path: perm_image_path,
                            });
                            self.message = format!("Đã lưu bối cảnh '{}' vào Kho Preset dùng chung!", display_name);
                        }

                        if let Err(e) = ScenePreset::save_all(&root, &presets) {
                            self.message = format!("Lỗi lưu Preset bối cảnh: {e}");
                        }
                    }
                }
                186 => {
                    let index = index.max(0) as usize;
                    let root = studio.host.dirs.data.join("google-flow");
                    let presets = ScenePreset::list(&root);
                    if let Some(preset) = presets.get(index) {
                        let new_idx = self.settings.backgrounds.len();
                        self.settings.backgrounds.push(preset.description.clone());
                        let img = if !preset.image_path.is_empty() {
                            PathBuf::from(&preset.image_path)
                        } else {
                            PathBuf::new()
                        };
                        self.settings.background_images.push(img);
                        self.background_edit = new_idx;
                        if self.settings.style.is_empty() && !preset.style.is_empty() {
                            self.settings.style = preset.style.clone();
                        }
                        if let Some(plan) = self.plan_mut() {
                            plan.background = Some(new_idx);
                            plan.invalidate();
                        }

                        self.invalidate_prompts();
                        if let Err(e) = self.save() {
                            log::warn!("flow: save failed: {e}");
                        }
                        self.message = format!("Đã thêm bối cảnh '{}' từ Kho Preset vào dự án Flow!", preset.name);
                    }
                }
                187 => {
                    let index = index.max(0) as usize;
                    let root = studio.host.dirs.data.join("google-flow");
                    let mut presets = ScenePreset::list(&root);
                    if index < presets.len() {
                        let removed = presets.remove(index);
                        if !removed.image_path.is_empty() {
                            let p = PathBuf::from(&removed.image_path);
                            if p.is_file() && p.starts_with(root.join("presets")) {
                                let _ = std::fs::remove_file(p);
                            }
                        }
                        if let Err(e) = ScenePreset::save_all(&root, &presets) {
                            self.message = format!("Lỗi lưu Preset bối cảnh: {e}");
                        } else {
                            self.message = format!("Đã xóa bối cảnh '{}' khỏi Kho Preset.", removed.name);
                        }
                    }
                }
                _ => {}
            },
            _ => {}
        }
        self.settings.sync_character_ids();
        if self.loaded && !self.busy {
            if let Err(e) = self.save() {
                self.message = e;
            }
        }
    }
}
fn valid_project_url(url: &str) -> bool {
    crate::flow_accounts::valid_project(url)
}

fn scene_blocks(screenplay: &str) -> Vec<String> {
    screenplay
        .replace("\r\n", "\n")
        .split("\n\n")
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_owned)
        .collect()
}
fn synchronized_prompt(scene: &str, continuity: &str, style: &str) -> String {
    let mut parts = vec![scene.trim().to_string()];
    if !continuity.trim().is_empty() {
        parts.push(format!("Characters & Consistency:\n{}", continuity.trim()));
    }
    if !style.trim().is_empty() {
        parts.push(format!("Style: {}", style.trim()));
    }
    parts.push("Camera & Motion: Slow, calm, and steady pacing. No rapid pans, fast zooms, or sudden erratic camera moves. No on-screen text or subtitles.".into());
    parts.join("\n\n")
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn references_follow_selected_profiles_and_do_not_require_cached_native_portraits() {
        let directory = std::env::temp_dir().join(format!(
            "concat-cast-refs-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&directory).unwrap();
        let paths = [
            directory.join("nam.png"),
            directory.join("lan.png"),
            directory.join("binh.png"),
        ];
        std::fs::write(&paths[1], b"reference fixture").unwrap();
        let mut flow = Flow::default();
        flow.settings.characters = vec!["Nam".into(), "Lan".into(), "Bình".into()];
        flow.settings.character_images = paths.to_vec();
        flow.settings.character_flow_origins = vec![
            FlowCharacterOrigin::default(),
            FlowCharacterOrigin::default(),
            FlowCharacterOrigin {
                id: "native-binh".into(),
                ..Default::default()
            },
        ];
        flow.settings.plans.push(ScenePlan {
            cast: vec![1, 2],
            cast_configured: true,
            ..Default::default()
        });
        flow.settings.sync_character_ids();
        let manual = format!("{}\n{}", paths[0].display(), paths[2].display());
        assert_eq!(
            flow.scene_references(0, &manual).unwrap(),
            vec![paths[1].to_string_lossy().into_owned()]
        );
        std::fs::remove_file(&paths[1]).unwrap();
        std::fs::remove_dir(&directory).unwrap();
    }
    #[test]
    fn project_urls_are_scoped_to_flow() {
        assert!(valid_project_url(
            "https://flow.google.com/project/11111111-1111-1111-1111-111111111111"
        ));
        assert!(!valid_project_url(
            "https://evil.example/project/11111111-1111-1111-1111-111111111111"
        ));
        assert!(!valid_project_url(
            "https://flow.google.com/project/not-a-project"
        ));
    }
    #[test]
    fn legacy_model_selection_migrates_to_omni_flash_1_2() {
        assert_eq!(normalize_model(2), 0);
        assert_eq!(normalize_model(0), 0);
        assert_eq!(normalize_model(1), 1);
        assert_eq!(normalize_model(99), 0);
    }
}
