// SPDX-License-Identifier: AGPL-3.0-or-later
//! Persisted scene editorial data and conservative render recovery.
use super::visual_state::ReferenceImage;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub(super) struct ScenePlan {
    pub account_id: String,
    pub project_url: String,
    pub last_account_id: String,
    pub selected: bool,
    pub prompt: String,
    pub reference: String,
    pub camera: String,
    pub shot: String,
    pub movement: String,
    pub sound: String,
    pub dialogue: String,
    pub lighting: String,
    pub cast: Vec<usize>,
    /// Canonical links; None migrates the older index-based cast once.
    pub cast_ids: Option<Vec<String>>,
    pub dialogue_lines: Vec<DialogueLine>,
    pub character_actions: std::collections::BTreeMap<String, String>,
    pub cast_configured: bool,
    pub background: Option<usize>,
    pub status: String,
    pub needs_prompt: bool,
    pub files: Vec<PathBuf>,
    pub scene_image: PathBuf,
    pub image_history: Vec<ReferenceImage>,
    pub image_status: String,
    pub image_stale: bool,
    /// 0 = Ingredients (tối đa 3 ảnh tham chiếu), 1 = Start / End Frame (khung đầu / khung cuối)
    pub visual_mode: usize,
    /// Khung cuối cho chế độ Start / End Frame
    pub end_image: PathBuf,
    /// Tự động lấy frame cuối của cảnh trước làm khung đầu cho cảnh này
    pub scene_flow_auto: bool,
}
#[derive(Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub(super) struct DialogueLine {
    pub character_id: String,
    pub text: String,
}
impl ScenePlan {
    pub fn remove_character(&mut self, index: usize) {
        self.cast.retain(|&i| i != index);
        for i in &mut self.cast {
            if *i > index {
                *i -= 1;
            }
        }
        self.invalidate();
    }
    pub fn remove_background(&mut self, index: usize) {
        self.background = self.background.and_then(|i| {
            if i == index {
                None
            } else {
                Some(if i > index { i - 1 } else { i })
            }
        });
        self.invalidate();
    }
    pub fn label(&self) -> &str {
        if self.needs_prompt && !matches!(self.status.as_str(), "running" | "uncertain") {
            return "Cần cập nhật prompt";
        }
        match self.status.as_str() {
            "running" => "Đang tạo",
            "completed" => "Xong",
            "prepared" => "Đã chuẩn bị",
            "uncertain" => "Cần kiểm tra Flow",
            "waiting" => "Đang chờ",
            "edited" => "Đã sửa · cần tạo lại",
            _ => "Chưa chạy",
        }
    }
    pub fn recover(&mut self) {
        if self.image_status == "running" {
            self.image_status = "uncertain".into();
        }
        if self.status == "running" {
            self.status = "uncertain".into();
        }
        if self.status == "waiting" {
            self.status.clear();
        }
    }
    pub fn queueable(&self) -> bool {
        self.selected
            && !self.needs_prompt
            && !self.prompt.trim().is_empty()
            && !matches!(self.status.as_str(), "running" | "completed" | "uncertain")
    }
    pub fn edited(&mut self) {
        if !matches!(self.status.as_str(), "running" | "uncertain") {
            self.status = "edited".into();
        }
    }
    pub fn invalidate(&mut self) {
        if !self.scene_image.as_os_str().is_empty() {
            self.image_stale = true;
        }
        self.needs_prompt = true;
        self.edited();
    }
    pub fn directions(&self) -> String {
        [
            ("Shot size", &self.shot),
            ("Shot / camera angle", &self.camera),
            ("Camera movement", &self.movement),
            ("Sound / music / SFX", &self.sound),
            ("Exact dialogue (preserve language)", &self.dialogue),
            ("Lighting", &self.lighting),
        ]
        .into_iter()
        .filter(|(_, value)| !value.trim().is_empty())
        .map(|(name, value)| format!("{name}: {}", value.trim()))
        .collect::<Vec<_>>()
        .join("\n")
    }
}

/// Preserve metadata only for exact, occurrence-matched scenes on bulk import.
pub(super) fn reconcile(old: &[String], new: &[String], plans: &[ScenePlan]) -> Vec<ScenePlan> {
    let mut used = vec![false; old.len()];
    new.iter()
        .map(|text| {
            let index = old
                .iter()
                .enumerate()
                .position(|(i, previous)| !used[i] && previous == text);
            if let Some(i) = index {
                used[i] = true;
                plans.get(i).cloned().unwrap_or_default()
            } else {
                ScenePlan::default()
            }
        })
        .collect()
}

pub(super) fn references(text: &str) -> Result<Vec<String>, String> {
    let mut paths = Vec::new();
    for line in text
        .lines()
        .map(|s| s.trim().trim_matches('"'))
        .filter(|s| !s.is_empty())
    {
        if !std::path::Path::new(line).is_file() {
            return Err(format!("Không tìm thấy ảnh tham chiếu: {line}"));
        }
        if !paths.iter().any(|p| p == line) {
            paths.push(line.to_owned());
        }
    }
    if paths.len() > 8 {
        return Err("Tối đa 8 ảnh tham chiếu cho một cảnh.".into());
    }
    Ok(paths)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn edited_scene_frames_require_review_and_inflight_images_recover_conservatively() {
        let mut plan = ScenePlan {
            scene_image: PathBuf::from("approved.png"),
            image_status: "running".into(),
            ..Default::default()
        };
        plan.invalidate();
        assert!(plan.image_stale);
        plan.recover();
        assert_eq!(plan.image_status, "uncertain");
        let restored: ScenePlan =
            serde_json::from_str(&serde_json::to_string(&plan).unwrap()).unwrap();
        assert!(restored.image_stale);
        assert_eq!(restored.scene_image, PathBuf::from("approved.png"));
    }
    #[test]
    fn interrupted_jobs_never_reenter_a_paid_queue() {
        let mut plan = ScenePlan {
            selected: true,
            prompt: "prompt".into(),
            status: "running".into(),
            ..Default::default()
        };
        plan.recover();
        assert_eq!(plan.status, "uncertain");
        assert!(!plan.queueable());
        plan.status = "completed".into();
        assert!(!plan.queueable());
        plan.status = "waiting".into();
        plan.recover();
        assert!(plan.queueable());
        plan.invalidate();
        assert!(!plan.queueable());
        plan.needs_prompt = false;
        plan.status = "uncertain".into();
        plan.invalidate();
        assert_eq!(plan.status, "uncertain");
        assert!(!plan.queueable());
    }
    #[test]
    fn reordering_and_duplicate_scenes_do_not_swap_results() {
        let old = vec!["A".into(), "B".into(), "A".into()];
        let plans = (0..3)
            .map(|i| ScenePlan {
                prompt: format!("p{i}"),
                ..Default::default()
            })
            .collect::<Vec<_>>();
        let new = vec!["A".into(), "A".into(), "B".into(), "new".into()];
        let output = reconcile(&old, &new, &plans);
        assert_eq!(
            output.iter().map(|p| p.prompt.as_str()).collect::<Vec<_>>(),
            vec!["p0", "p2", "p1", ""]
        );
    }
    #[test]
    fn removing_profiles_keeps_the_same_remaining_cast_and_location() {
        let mut plan = ScenePlan {
            cast_configured: true,
            cast: vec![0, 2],
            background: Some(2),
            ..Default::default()
        };
        plan.remove_character(1);
        assert_eq!(plan.cast, vec![0, 1]);
        plan.remove_character(0);
        assert_eq!(plan.cast, vec![0]);
        plan.remove_background(1);
        assert_eq!(plan.background, Some(1));
        plan.remove_background(1);
        assert_eq!(plan.background, None);
        assert!(plan.needs_prompt);
    }
    #[test]
    fn reference_lists_deduplicate_and_reject_missing_files() {
        let file = std::env::temp_dir().join(format!(
            "concat-reference-{}-{}.png",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::write(&file, b"path validation fixture").unwrap();
        let path = file.to_string_lossy();
        assert_eq!(
            references(&format!("{path}\n\"{path}\"\n")).unwrap().len(),
            1
        );
        std::fs::remove_file(&file).unwrap();
        assert!(references(&path).is_err());
    }
}
