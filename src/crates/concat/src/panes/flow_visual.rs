// SPDX-License-Identifier: AGPL-3.0-or-later
//! Reference galleries and image prompts. Inspiration: character-sheet-simple
//! (Saeyeon-developer/character-design-sheet) and Character Turnaround Sheets
//! (talvanes/character-turnaround-sheets). No remote skill code is executed.
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub(super) struct ReferenceImage {
    pub path: PathBuf,
    pub kind: String,
    pub approved: bool,
}
pub(super) fn remember(gallery: &mut Vec<ReferenceImage>, path: &Path, kind: &str) {
    if !path.as_os_str().is_empty() && !gallery.iter().any(|item| item.path == path) {
        gallery.push(ReferenceImage {
            path: path.into(),
            kind: kind.into(),
            approved: false,
        });
    }
}
pub(super) fn approve_sheet(gallery: &mut [ReferenceImage], index: usize) -> Result<(), String> {
    let item = gallery.get(index).ok_or("Không tìm thấy bộ ảnh")?;
    if item.kind != "sheet" || !item.path.is_file() {
        return Err("Bộ ảnh không hợp lệ hoặc đã bị di chuyển.".into());
    }
    let approved = !item.approved;
    for item in gallery.iter_mut().filter(|item| item.kind == "sheet") {
        item.approved = false;
    }
    gallery[index].approved = approved;
    Ok(())
}
pub(super) fn identity_reference<'a>(gallery: &'a [ReferenceImage], anchor: &'a Path) -> &'a Path {
    gallery
        .iter()
        .rev()
        .find(|item| item.kind == "sheet" && item.approved)
        .map_or(anchor, |item| item.path.as_path())
}
pub(super) fn sheet_prompt(profile: &str, style: &str) -> String {
    let visual = super::clean_character_visual(profile);
    let style = style.trim();
    if style.is_empty() {
        format!(
            "Landscape 3-angle character reference sheet of the SAME character: left panel, full-body front view; middle panel, full-body back view; right panel, close-up portrait. Plain studio background, neutral lighting. Character: {visual}"
        )
    } else {
        format!(
            "Landscape 3-angle character reference sheet of the SAME character: left panel, full-body front view; middle panel, full-body back view; right panel, close-up portrait. Plain studio background, neutral lighting. Character: {visual}. Rendering style: {style}"
        )
    }
}
pub(super) fn background_sheet_prompt(profile: &str, style: &str) -> String {
    format!(
        "Create one 16:9 landscape environment reference sheet showing three distinct camera angles of the SAME authentic Vietnamese location (unless location description explicitly states another country) without people or text: left panel, wide establishing shot showing the full space and layout; middle panel, reverse-angle or alternate wide perspective of the same room/environment; right panel, medium architectural and prop detail shot with consistent lighting, materials and color palette. Preserve identical furniture, architecture, props, time of day and atmosphere across all three panels. No people, labels, text, logos or watermarks. Location description: {profile}\nRendering style: {style}"
    )
}
pub(super) fn scene_prompt(
    scene: &str,
    cast: &[(String, String)],
    background: &str,
    directions: &str,
    style: &str,
) -> String {
    let people = cast
        .iter()
        .map(|(name, profile)| format!("{name}: {profile}"))
        .collect::<Vec<_>>()
        .join("\n");
    format!(
        "Create ONE cinematic still opening frame for this scene, not a video, collage or storyboard. Use the attached character references as identity guides only: render each selected person ONCE, never reproduce a reference sheet's multiple figures or background. Preserve their exact Vietnamese facial features, appearance and wardrobe (unless profile states another nationality), and render an authentic Vietnamese environment/setting (unless location states otherwise). Only these characters may appear:\n{people}\nScene (choose one filmable instant; dialogue is acting context, never visible lettering): {scene}\nLocation: {background}\nFraming and light: {directions}\nStyle: {style}\nNo subtitles, captions, logos or watermarks."
    )
}
pub(super) fn scene_angles_prompt(
    scene: &str,
    cast: &[(String, String)],
    background: &str,
    directions: &str,
    style: &str,
) -> String {
    let people = cast
        .iter()
        .map(|(name, profile)| format!("{name}: {profile}"))
        .collect::<Vec<_>>()
        .join("\n");
    format!(
        "Create ONE 16:9 landscape multi-angle cinematic reference sheet for this scene with three synchronized camera angles of the SAME moment: left panel, wide establishing shot of the characters in the environment; middle panel, medium two-shot / action angle; right panel, expressive close-up / over-the-shoulder angle. Preserve each character's exact Vietnamese facial features, appearance and wardrobe from the attached identity references, and keep the authentic Vietnamese setting, lighting and continuity identical across all three panels. Only these characters may appear:\n{people}\nScene (dialogue is acting context, never visible lettering): {scene}\nLocation: {background}\nFraming and light: {directions}\nStyle: {style}\nNo subtitles, captions, labels, logos or watermarks."
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn galleries_keep_anchors_and_only_approved_sheets_are_used() {
        let root = std::env::temp_dir().join(format!("concat-visual-{}", std::process::id()));
        std::fs::create_dir_all(&root).unwrap();
        let anchor = root.join("anchor.png");
        let sheet = root.join("sheet.png");
        std::fs::write(&anchor, b"fixture").unwrap();
        std::fs::write(&sheet, b"fixture").unwrap();
        let mut gallery = Vec::new();
        remember(&mut gallery, &anchor, "portrait");
        remember(&mut gallery, &sheet, "sheet");
        remember(&mut gallery, &anchor, "portrait");
        assert_eq!(gallery.len(), 2);
        assert_eq!(identity_reference(&gallery, &anchor), anchor);
        assert!(approve_sheet(&mut gallery, 0).is_err());
        approve_sheet(&mut gallery, 1).unwrap();
        assert_eq!(identity_reference(&gallery, &anchor), sheet);
        approve_sheet(&mut gallery, 1).unwrap();
        assert_eq!(identity_reference(&gallery, &anchor), anchor);
        assert_eq!(gallery[0].path, anchor);
        std::fs::remove_dir_all(root).unwrap();
    }
}
