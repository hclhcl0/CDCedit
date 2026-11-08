// SPDX-License-Identifier: AGPL-3.0-or-later
//! Shared character identities and transactional AI cast drafts. No name-only scene links.
use super::{
    Settings,
    scene_state::{DialogueLine, ScenePlan},
};
use serde::Deserialize;
use serde_json::Value;
use std::collections::{HashMap, HashSet};

fn new_id() -> String {
    static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
    format!(
        "char-{:x}-{:x}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos(),
        NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    )
}
pub(super) fn name(profile: &str) -> &str {
    profile.lines().next().unwrap_or("Nhân vật").trim()
}
fn key(value: &str) -> String {
    value
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase()
}

impl Settings {
    pub(super) fn sync_character_ids(&mut self) {
        self.character_ids.truncate(self.characters.len());
        while self.character_ids.len() < self.characters.len() {
            self.character_ids.push(new_id());
        }
        self.character_roles
            .resize(self.characters.len(), String::new());
        let mut seen = HashSet::new();
        for id in &mut self.character_ids {
            if id.is_empty() || !seen.insert(id.clone()) {
                *id = new_id();
                seen.insert(id.clone());
            }
        }
        let labels = self.character_labels();
        for plan in &mut self.plans {
            if plan.cast_ids.is_none() {
                plan.cast_ids = Some(if plan.cast_configured {
                    plan.cast
                        .iter()
                        .filter_map(|&i| self.character_ids.get(i).cloned())
                        .collect()
                } else {
                    self.character_ids.clone()
                });
            }
            if !plan.cast_configured {
                plan.cast_ids = Some(self.character_ids.clone());
            }
            plan.cast = plan
                .cast_ids
                .iter()
                .flatten()
                .filter_map(|id| self.character_ids.iter().position(|item| item == id))
                .collect();
            if !plan.dialogue_lines.is_empty() {
                plan.dialogue = plan
                    .dialogue_lines
                    .iter()
                    .map(|line| {
                        let speaker = self
                            .character_ids
                            .iter()
                            .position(|id| id == &line.character_id)
                            .and_then(|i| labels.get(i))
                            .map(String::as_str)
                            .unwrap_or("[Chưa chọn / đã xóa người nói]");
                        format!("{speaker}: {}", line.text)
                    })
                    .collect::<Vec<_>>()
                    .join("\n");
            }
        }
    }
    pub(super) fn character_labels(&self) -> Vec<String> {
        self.characters
            .iter()
            .enumerate()
            .map(|(i, profile)| {
                let title = name(profile);
                if self
                    .characters
                    .iter()
                    .filter(|other| key(name(other)) == key(title))
                    .count()
                    > 1
                {
                    format!(
                        "{title} · {} · #{}",
                        self.character_roles
                            .get(i)
                            .map(String::as_str)
                            .filter(|s| !s.trim().is_empty())
                            .unwrap_or("Hồ sơ"),
                        i + 1
                    )
                } else {
                    format!(
                        "{title}{}",
                        self.character_roles
                            .get(i)
                            .filter(|s| !s.trim().is_empty())
                            .map(|role| format!(" — {role}"))
                            .unwrap_or_default()
                    )
                }
            })
            .collect()
    }
    pub(super) fn validate_cast(&self, index: usize) -> Result<(), String> {
        if let Some(plan) = self.plans.get(index) {
            if plan
                .cast_ids
                .iter()
                .flatten()
                .any(|id| !self.character_ids.contains(id))
            {
                return Err(
                    "Cảnh còn liên kết nhân vật đã xóa. Chọn lại nhân vật xuất hiện.".into(),
                );
            }
            for line in &plan.dialogue_lines {
                let speaker = self
                    .character_ids
                    .iter()
                    .position(|id| id == &line.character_id);
                if speaker.is_none()
                    || (plan.cast_configured && !plan.cast.contains(&speaker.unwrap()))
                {
                    return Err("Có lời thoại chưa gắn người nói hoặc người nói không xuất hiện trong cảnh.".into());
                }
                if line.text.trim().is_empty() {
                    return Err("Có câu thoại trống. Điền nội dung hoặc xóa câu thoại.".into());
                }
            }
        }
        Ok(())
    }
    pub(super) fn character_context(&self) -> Value {
        Value::Array(self.characters.iter().enumerate().map(|(i,profile)|serde_json::json!({"id":self.character_ids.get(i),"name":name(profile),"role":self.character_roles.get(i),"profile":profile})).collect())
    }
}

#[derive(Deserialize)]
struct DraftCharacter {
    id: String,
    name: String,
    #[serde(default)]
    role: String,
    profile: String,
}
#[derive(Deserialize)]
struct DraftScene {
    scene_index: usize,
    cast: Vec<String>,
    #[serde(default)]
    dialogue: Vec<DialogueLine>,
    #[serde(default)]
    actions: HashMap<String, String>,
}

/// Apply only to a cloned Settings. Callers replace live state only after all links validate.
pub(super) fn apply_draft(
    settings: &mut Settings,
    value: &Value,
    start: usize,
    count: usize,
) -> Result<(), String> {
    settings.sync_character_ids();
    let characters: Vec<DraftCharacter> = serde_json::from_value(value["characters"].clone())
        .map_err(|_| "AI chưa trả danh sách nhân vật hợp lệ.")?;
    let scenes: Vec<DraftScene> = serde_json::from_value(value["cast_scenes"].clone())
        .map_err(|_| "AI chưa trả liên kết nhân vật theo cảnh.")?;
    if characters.len() > 100 || scenes.len() != count {
        return Err("AI trả sai số hồ sơ hoặc số cảnh liên kết.".into());
    }
    let mut mapping: HashMap<String, String> = settings
        .character_ids
        .iter()
        .map(|id| (id.clone(), id.clone()))
        .collect();
    let mut draft_ids = HashSet::new();
    let mut draft_names = HashSet::new();
    for character in characters {
        if character.id.is_empty()
            || character.id.len() > 160
            || !draft_ids.insert(character.id.clone())
            || character.name.trim().is_empty()
            || character.name.len() > 500
            || character.profile.trim().is_empty()
            || character.profile.len() > 50000
        {
            return Err("AI trả hồ sơ trùng ID hoặc không hợp lệ.".into());
        }
        if let Some(i) = settings
            .character_ids
            .iter()
            .position(|id| id == &character.id)
        {
            if key(name(&settings.characters[i])) != key(&character.name) {
                return Err("AI ghép tên với sai ID nhân vật. Hồ sơ cũ được giữ nguyên.".into());
            }
            if !settings.character_roles[i].trim().is_empty()
                && !character.role.trim().is_empty()
                && key(&settings.character_roles[i]) != key(&character.role)
            {
                return Err(
                    "AI ghép vai khác vào ID nhân vật cũ. Kiểm tra lại hai nhân vật trùng tên."
                        .into(),
                );
            }
            if settings.character_roles[i].trim().is_empty() && character.role.len() <= 1000 {
                settings.character_roles[i] = character.role;
            }
            continue; // Existing profiles, images and Flow links are authoritative.
        }
        if character.role.len() > 1000 {
            return Err("Vai nhân vật quá dài.".into());
        }
        if !draft_names.insert((key(&character.name), key(&character.role))) {
            return Err("AI đề xuất nhiều người cùng tên và vai chưa phân biệt. Bổ sung vai riêng cho từng người.".into());
        }
        let matches = settings
            .characters
            .iter()
            .enumerate()
            .filter(|(i, profile)| {
                key(name(profile)) == key(&character.name)
                    && key(settings
                        .character_roles
                        .get(*i)
                        .map(String::as_str)
                        .unwrap_or(""))
                        == key(&character.role)
            })
            .map(|(i, _)| i)
            .collect::<Vec<_>>();
        if matches.len() > 1 {
            return Err(format!(
                "Có nhiều hồ sơ '{}'. Dùng đúng ID hoặc bổ sung vai để phân biệt.",
                character.name
            ));
        }
        if let Some(&i) = matches.first() {
            mapping.insert(character.id, settings.character_ids[i].clone());
            continue;
        }
        let id = new_id();
        settings.characters.push(format!(
            "{}\n{}",
            character.name.trim(),
            character.profile.trim()
        ));
        settings.character_ids.push(id.clone());
        settings.character_roles.push(character.role);
        mapping.insert(character.id, id);
    }
    settings
        .plans
        .resize_with(settings.scene_items.len(), ScenePlan::default);
    let mut indices = HashSet::new();
    for scene in scenes {
        if scene.scene_index >= count
            || !indices.insert(scene.scene_index)
            || scene.cast.len() > 100
            || scene.dialogue.len() > 200
            || scene.actions.len() > 100
        {
            return Err("AI trả chỉ số cảnh hoặc số nhân vật/lời thoại không hợp lệ.".into());
        }
        let mut cast = Vec::new();
        for alias in scene.cast {
            let id = mapping
                .get(&alias)
                .ok_or("AI dùng nhân vật chưa có hồ sơ.")?
                .clone();
            if cast.contains(&id) {
                return Err("AI lặp nhân vật trong một cảnh.".into());
            }
            cast.push(id);
        }
        let plan = settings
            .plans
            .get_mut(start + scene.scene_index)
            .ok_or("Cảnh đã thay đổi khi đồng bộ.")?;
        plan.cast_ids = Some(cast.clone());
        plan.cast_configured = true;
        plan.dialogue_lines = scene
            .dialogue
            .into_iter()
            .map(|mut line| {
                if !line.character_id.is_empty() {
                    line.character_id = mapping
                        .get(&line.character_id)
                        .ok_or("AI dùng người nói chưa có hồ sơ.")?
                        .clone();
                }
                if (!line.character_id.is_empty() && !cast.contains(&line.character_id))
                    || line.text.trim().is_empty()
                    || line.text.len() > 10000
                {
                    return Err("Người nói phải có trong cảnh và câu thoại phải hợp lệ.");
                }
                Ok(line)
            })
            .collect::<Result<Vec<_>, &str>>()?;
        plan.dialogue.clear();
        plan.character_actions.clear();
        for (alias, action) in scene.actions {
            let id = mapping
                .get(&alias)
                .ok_or("Vai trò cảnh tham chiếu nhân vật không hợp lệ.")?;
            if !cast.contains(id) || action.len() > 10000 {
                return Err("Hành động nhân vật không thuộc cảnh hoặc quá dài.".into());
            }
            plan.character_actions.insert(id.clone(), action);
        }
        plan.invalidate();
    }
    settings.sync_character_ids();
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn ids_survive_rename_and_dialogue_uses_current_speaker_name() {
        let mut s = Settings::default();
        s.characters = vec!["Nam\nÁo xanh".into(), "Lan\nÁo đỏ".into()];
        s.plans.push(ScenePlan::default());
        s.sync_character_ids();
        let id = s.character_ids[0].clone();
        s.plans[0].dialogue_lines.push(DialogueLine {
            character_id: id.clone(),
            text: "Chào Lan".into(),
        });
        s.characters[0] = "Nam mới\nÁo xanh".into();
        s.sync_character_ids();
        assert_eq!(s.character_ids[0], id);
        assert_eq!(s.plans[0].dialogue, "Nam mới: Chào Lan");
        s.characters.remove(0);
        s.character_ids.remove(0);
        s.sync_character_ids();
        assert!(s.validate_cast(0).is_err());
    }
    #[test]
    fn legacy_cast_migrates_once_and_same_names_keep_separate_roles() {
        let mut s = Settings::default();
        s.characters = vec!["Nam\nNgười cha".into(), "Lan\nNgười mẹ".into()];
        s.scene_items = vec!["Nam chào Lan".into()];
        s.plans.push(ScenePlan {
            cast: vec![0],
            cast_configured: true,
            ..Default::default()
        });
        s.sync_character_ids();
        let father = s.character_ids[0].clone();
        assert_eq!(s.plans[0].cast_ids, Some(vec![father.clone()]));
        s.character_roles[0] = "người cha".into();
        s.character_images.push("portrait.png".into());
        let value = serde_json::json!({"characters":[{"id":"new-student","name":"Nam","role":"học sinh","profile":"Áo đồng phục"}],"cast_scenes":[{"scene_index":0,"cast":[father,"new-student"],"dialogue":[{"character_id":"new-student","text":"Chào bố"}]}]});
        apply_draft(&mut s, &value, 0, 1).unwrap();
        assert_eq!(s.characters.len(), 3);
        assert_ne!(s.character_ids[0], s.character_ids[2]);
        assert_eq!(s.plans[0].cast, vec![0, 2]);
        assert_eq!(
            s.plans[0].dialogue_lines[0].character_id,
            s.character_ids[2]
        );
        assert!(s.plans[0].dialogue.contains("học sinh"));
        assert_eq!(
            s.character_images[0],
            std::path::PathBuf::from("portrait.png")
        );
        assert!(s.validate_cast(0).is_ok());
        let before = s.character_ids.clone();
        apply_draft(&mut s, &value, 0, 1).unwrap();
        assert_eq!(s.character_ids, before);
        s.plans[0].cast_ids = Some(vec![s.character_ids[0].clone()]);
        s.sync_character_ids();
        assert!(s.validate_cast(0).is_err());
    }
    #[test]
    fn unknown_speaker_and_duplicate_scene_assignment_are_rejected() {
        let mut s = Settings::default();
        s.scene_items = vec!["Cảnh".into()];
        let invalid = serde_json::json!({"characters":[],"cast_scenes":[{"scene_index":0,"cast":[],"dialogue":[{"character_id":"missing","text":"Chào"}]}]});
        assert!(apply_draft(&mut s, &invalid, 0, 1).is_err());
        let unassigned = serde_json::json!({"characters":[],"cast_scenes":[{"scene_index":0,"cast":[],"dialogue":[{"character_id":"","text":"Ai đó gọi cửa"}]}]});
        apply_draft(&mut s, &unassigned, 0, 1).unwrap();
        assert_eq!(s.plans[0].dialogue_lines.len(), 1);
        assert!(s.validate_cast(0).is_err());
        let invalid =
            serde_json::json!({"characters":[],"cast_scenes":[{"scene_index":1,"cast":[]}]});
        assert!(apply_draft(&mut s, &invalid, 0, 1).is_err());
    }
}
