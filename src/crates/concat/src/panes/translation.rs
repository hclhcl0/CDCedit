//! Reviewable translation of foreign-language SRT/media into Vietnamese.
use crate::{
    host::{on_ui_in_project, project_epoch, spawn_in_project},
    panes::Msg,
    studio::Studio,
    ui::{TranslationData, TranslationRowData},
};
use concat_project::srt;
use concat_speech::{
    transcribe::{Segment, TranscribeRequest},
    translation::{self as service, Line},
};
use std::{
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};
const LANGUAGES: &[&str] = &[
    "auto", "en", "zh", "ja", "ko", "fr", "de", "es", "ru", "th", "id", "vi",
];
#[derive(Debug)]
pub enum TranslationMsg {
    Action(i32, i32, String, f32),
    Picked(PathBuf),
    Progress(u64, String),
    Finished(u64, Result<Outcome, String>),
    Save(PathBuf, String),
}
#[derive(Debug)]
pub enum Outcome {
    Loaded(PathBuf, Vec<Line>),
    Media(PathBuf),
    Recognized(Vec<Line>),
    Translated(Vec<(usize, String)>),
}
pub struct Translation {
    pub open: bool,
    busy: bool,
    recognizing: bool,
    generation: u64,
    cancel: Arc<AtomicBool>,
    file: Option<PathBuf>,
    media: bool,
    language: usize,
    whisper: usize,
    model: String,
    natural: bool,
    glossary: String,
    rows: Vec<Line>,
    page: usize,
    reviewed: bool,
    offset: f32,
    message: String,
}
impl Default for Translation {
    fn default() -> Self {
        Self {
            open: false,
            busy: false,
            recognizing: false,
            generation: 0,
            cancel: Arc::default(),
            file: None,
            media: false,
            language: 0,
            whisper: 0,
            model: "gemini-3.8-flash".into(),
            natural: false,
            glossary: String::new(),
            rows: Vec::new(),
            page: 0,
            reviewed: false,
            offset: 0.0,
            message: String::new(),
        }
    }
}
impl Drop for Translation {
    fn drop(&mut self) {
        self.cancel.store(true, Ordering::Relaxed);
    }
}
fn from_segments(segments: Vec<Segment>) -> Vec<Line> {
    segments
        .into_iter()
        .filter(|s| !s.text.trim().is_empty())
        .map(|s| Line {
            start: s.start,
            end: s.end,
            source: s.text,
            translated: String::new(),
            speaker: 0,
        })
        .collect()
}
impl Translation {
    fn start(&mut self) -> (u64, Arc<AtomicBool>) {
        self.generation = self.generation.wrapping_add(1);
        self.cancel = Arc::default();
        self.busy = true;
        self.reviewed = false;
        (self.generation, self.cancel.clone())
    }
    fn cues(&self, bilingual: bool, original: bool) -> Result<Vec<srt::Cue>, String> {
        if self.rows.is_empty() {
            return Err("Chưa có phụ đề.".into());
        }
        if !original && (!self.reviewed || self.rows.iter().any(|r| r.translated.trim().is_empty()))
        {
            return Err("Cần dịch đủ câu và xác nhận đã duyệt bản dịch.".into());
        }
        Ok(self
            .rows
            .iter()
            .map(|r| srt::Cue {
                start: r.start,
                end: r.end,
                text: if original {
                    r.source.clone()
                } else if bilingual {
                    format!("{}\n{}", r.source, r.translated)
                } else {
                    r.translated.clone()
                },
            })
            .collect())
    }
    pub fn update(&mut self, msg: TranslationMsg, studio: &mut Studio) {
        match msg {
            TranslationMsg::Progress(g, text) if g == self.generation => self.message = text,
            TranslationMsg::Finished(g, result) if g == self.generation => {
                self.busy = false;
                self.recognizing = false;
                if self.cancel.load(Ordering::Relaxed) {
                    self.message = "Đã dừng; bản gốc và bản dịch trước được giữ nguyên.".into();
                    return;
                }
                match result {
                    Ok(Outcome::Loaded(path, rows)) => {
                        self.file = Some(path);
                        self.media = false;
                        self.rows = rows;
                        self.page = 0;
                        self.reviewed = false;
                        self.message = "Đã nạp SRT. Chọn kiểu dịch và bấm Dịch toàn bộ.".into();
                    }
                    Ok(Outcome::Media(path)) => {
                        self.file = Some(path);
                        self.media = true;
                        self.rows.clear();
                        self.page = 0;
                        self.reviewed = false;
                        self.message =
                            "Đã chọn video/audio. Bấm Nhận dạng tiếng gốc bằng Local.".into();
                    }
                    Ok(Outcome::Recognized(rows)) => {
                        self.rows = rows;
                        self.page = 0;
                        self.reviewed = false;
                        self.message = format!(
                            "Đã nhận dạng {} câu tiếng gốc. Chưa gửi tới Gemini; bấm Dịch toàn bộ để dịch.",
                            self.rows.len()
                        );
                    }
                    Ok(Outcome::Translated(changes)) => {
                        for (id, text) in changes {
                            if let Some(row) = self.rows.get_mut(id) {
                                row.translated = text;
                            }
                        }
                        self.reviewed = false;
                        self.message =
                            "Đã dịch. Đối chiếu, sửa các câu cần thiết rồi xác nhận đã duyệt."
                                .into();
                    }
                    Err(e) => self.message = e,
                }
            }
            TranslationMsg::Picked(path) => {
                if self.busy {
                    return;
                }
                let (g, cancel) = self.start();
                self.message = "Đang đọc file…".into();
                spawn_in_project(
                    move || {
                        if path
                            .extension()
                            .is_some_and(|e| e.eq_ignore_ascii_case("srt"))
                        {
                            if std::fs::metadata(&path).map_err(|e| e.to_string())?.len()
                                > 10_000_000
                            {
                                return Err("SRT vượt 10 MB.".into());
                            }
                            let bytes = std::fs::read(&path).map_err(|e| e.to_string())?;
                            let text = String::from_utf8(bytes)
                                .map_err(|_| "SRT cần mã hoá UTF-8 để giữ đúng chữ gốc.")?;
                            let cues = srt::parse(&text).map_err(|e| e.to_string())?;
                            if cues.is_empty() || cues.len() > 5000 {
                                return Err("Nạp từ 1 đến 5.000 câu phụ đề.".into());
                            }
                            let rows = cues
                                .into_iter()
                                .map(|c| Line {
                                    start: c.start,
                                    end: c.end,
                                    source: c.text,
                                    translated: String::new(),
                                    speaker: 0,
                                })
                                .collect();
                            concat_speech::cloud_tts::check_cancel(&cancel)?;
                            Ok(Outcome::Loaded(path, rows))
                        } else {
                            let info = concat_media::probe(&path).map_err(|e| e.to_string())?;
                            if info.audio.is_none()
                                || info.duration.map(|d| d.as_f64()).unwrap_or(0.0) <= 0.0
                            {
                                return Err("File không có âm thanh hoặc thời lượng hợp lệ.".into());
                            }
                            concat_speech::cloud_tts::check_cancel(&cancel)?;
                            Ok(Outcome::Media(path))
                        }
                    },
                    move |studio, _, _, result| {
                        studio.handle(Msg::Translation(TranslationMsg::Finished(g, result)))
                    },
                );
            }
            TranslationMsg::Save(path, text) => match std::fs::write(&path, text) {
                Ok(_) => self.message = format!("Đã lưu {}", path.display()),
                Err(e) => self.message = format!("Không lưu được phụ đề: {e}"),
            },
            TranslationMsg::Action(action, index, text, value) => {
                if action == 0 {
                    self.open = true;
                    if self.file.is_none() {
                        let installed =
                            crate::panes::settings::installed(&studio.settings.transcribers);
                        self.whisper = installed
                            .iter()
                            .position(|m| m.active)
                            .unwrap_or(installed.len().saturating_sub(1));
                    }
                    return;
                }
                if action == 1 {
                    self.open = false;
                    return;
                }
                if action == 2 {
                    self.cancel.store(true, Ordering::Relaxed);
                    if self.recognizing {
                        studio.host.transcriber.cancel();
                    }
                    self.message = "Đang dừng…".into();
                    return;
                }
                if self.busy {
                    return;
                }
                if matches!(action, 4 | 7 | 8 | 9) {
                    self.reviewed = false;
                }
                match action {
                    3 => crate::host::pick_files_then(
                        "Chọn phụ đề SRT hoặc video/audio".into(),
                        Some((
                            "Subtitles / Media".into(),
                            &[
                                "srt", "mp4", "mkv", "mov", "webm", "mp3", "wav", "m4a", "flac",
                                "ogg",
                            ],
                        )),
                        |studio, _, _, paths| {
                            if let Some(path) = paths.into_iter().next() {
                                studio.handle(Msg::Translation(TranslationMsg::Picked(path)));
                            }
                        },
                    ),
                    4 => self.language = index.clamp(0, LANGUAGES.len() as i32 - 1) as usize,
                    5 => {
                        self.open = false;
                        studio.handle(Msg::Settings(crate::panes::settings::SettingsMsg::Open));
                    }
                    6 => self.whisper = index.max(0) as usize,
                    7 => self.natural = index != 0,
                    8 => self.glossary = text,
                    9 => self.model = text,
                    10 => self.recognize(studio),
                    11 | 12 | 14 => self.translate(
                        studio,
                        if action == 11 {
                            None
                        } else {
                            Some(index.max(0) as usize)
                        },
                        action == 14,
                    ),
                    13 => {
                        if let Some(row) = self.rows.get_mut(index.max(0) as usize) {
                            row.translated = text;
                            self.reviewed = false;
                        }
                    }
                    15 => {
                        if let Some(row) = self.rows.get_mut(index.max(0) as usize) {
                            row.speaker = if text == "B" { 1 } else { 0 };
                            self.reviewed = false;
                        }
                    }
                    16 => self.reviewed = index != 0,
                    17 => self.page = index.max(0) as usize,
                    18 if value.is_finite() => self.offset = value.max(0.0),
                    20 | 21 | 22 => match self.cues(action == 21, action == 22) {
                        Ok(cues) => {
                            let body = srt::write(&cues);
                            let folder = PathBuf::from(crate::studio::home_folder("Documents"));
                            let name = if action == 21 {
                                "phu-de-song-ngu.srt"
                            } else if action == 22 {
                                "phu-de-goc.srt"
                            } else {
                                "phu-de-tieng-viet.srt"
                            };
                            crate::host::after_dialog(
                                move || {
                                    crate::platform::save_file(
                                        "Lưu phụ đề SRT",
                                        &folder,
                                        name,
                                        ("SubRip", &["srt"]),
                                    )
                                    .map(|p| p.with_extension("srt"))
                                },
                                move |studio, _, _, path| {
                                    studio
                                        .handle(Msg::Translation(TranslationMsg::Save(path, body)))
                                },
                            );
                        }
                        Err(e) => self.message = e,
                    },
                    23 => {
                        if studio.session.is_none() {
                            self.message =
                                "Mở dự án video trước khi áp dụng phụ đề xuống timeline.".into();
                            return;
                        }
                        match self.cues(false, false) {
                            Ok(mut cues) => {
                                for cue in &mut cues {
                                    cue.start += self.offset as f64;
                                    cue.end += self.offset as f64;
                                }
                                studio.handle(Msg::Captions(
                                    crate::panes::captions::CaptionsMsg::ImportTranslated(cues),
                                ));
                                self.message =
                                    "Đã thêm bản dịch xuống timeline (có thể Undo).".into();
                            }
                            Err(e) => self.message = e,
                        }
                    }
                    24 => {
                        if studio.speech.running {
                            self.message = "Hãy chờ tác vụ giọng nói hiện tại kết thúc.".into();
                            return;
                        }
                        match self.cues(false, false).and_then(|cues| {
                            let project = studio
                                .standalone_project_dir()
                                .to_string_lossy()
                                .into_owned();
                            let path =
                                concat_speech::cloud_tts::output(&project, "translated-subtitles")?
                                    .with_extension("srt");
                            std::fs::write(&path, srt::write(&cues)).map_err(|e| e.to_string())?;
                            Ok(path)
                        }) {
                            Ok(path) => {
                                use crate::panes::speech::SpeechMsg;
                                studio.handle(Msg::Speech(SpeechMsg::Open));
                                studio.handle(Msg::Speech(SpeechMsg::DubbingAction(
                                    0,
                                    1,
                                    String::new(),
                                    0.0,
                                )));
                                studio.handle(Msg::Speech(SpeechMsg::DubbingAction(
                                    34,
                                    1,
                                    path.to_string_lossy().into_owned(),
                                    0.0,
                                )));
                                self.open = false;
                            }
                            Err(e) => self.message = e,
                        }
                    }
                    25 => {
                        if let Err(e) = self.cues(false, false) {
                            self.message = e;
                            return;
                        }
                        let turns = self
                            .rows
                            .iter()
                            .map(|r| concat_speech::gemini_studio::Turn {
                                speaker: r.speaker,
                                text: r.translated.clone(),
                                style: String::new(),
                            })
                            .collect();
                        let spans = self.rows.iter().map(|r| (r.start, r.end)).collect();
                        match studio.gemini_studio.load_timed(turns, spans) {
                            Ok(()) => self.open = false,
                            Err(e) => self.message = e,
                        }
                    }
                    _ => {}
                }
            }
            _ => {}
        }
    }
    fn recognize(&mut self, studio: &mut Studio) {
        if !self.media {
            self.message = "SRT đã có văn bản; không cần nhận dạng lại.".into();
            return;
        }
        let Some(path) = self.file.clone() else {
            return;
        };
        let installed = crate::panes::settings::installed(&studio.settings.transcribers);
        let Some(model) = installed.get(self.whisper).map(|m| m.id.clone()) else {
            self.message = "Tải model Whisper trong Cài đặt trước.".into();
            return;
        };
        let language = LANGUAGES[self.language].to_owned();
        let dirs = studio.host.dirs.clone();
        let engine = studio.host.transcriber.clone();
        let (g, cancel) = self.start();
        self.recognizing = true;
        self.message = "Đang nhận dạng ngôn ngữ gốc…".into();
        let epoch = project_epoch();
        spawn_in_project(
            move || {
                let info = concat_media::probe(&path).map_err(|e| e.to_string())?;
                let request = TranscribeRequest {
                    path: path.to_string_lossy().into_owned(),
                    audio_stream: None,
                    source_start: 0.0,
                    window: info.duration.ok_or("File không có thời lượng")?.as_f64(),
                    model_id: model,
                };
                let stop = engine.clone();
                let flag = cancel.clone();
                let segments = engine.transcribe_with_language(
                    &dirs,
                    &request,
                    Some(&language),
                    move |stage| {
                        if flag.load(Ordering::Relaxed) {
                            stop.cancel();
                        }
                        let message = match stage {
                            concat_speech::JobStage::Reading => "Đang đọc âm thanh gốc…",
                            concat_speech::JobStage::Loading => "Đang nạp Whisper…",
                            _ => "Whisper đang nhận dạng tiếng gốc…",
                        }
                        .to_owned();
                        on_ui_in_project(epoch, move |studio, _, _| {
                            studio.handle(Msg::Translation(TranslationMsg::Progress(g, message)))
                        });
                    },
                )?;
                concat_speech::cloud_tts::check_cancel(&cancel)?;
                let rows = from_segments(segments);
                if rows.is_empty() {
                    return Err("Không nhận dạng được lời nói.".into());
                }
                if rows.len() > 5000 {
                    return Err("Quá 5.000 câu; hãy chia video thành các phần nhỏ hơn.".into());
                }
                Ok(Outcome::Recognized(rows))
            },
            move |studio, _, _, result| {
                studio.handle(Msg::Translation(TranslationMsg::Finished(g, result)))
            },
        );
    }
    fn translate(&mut self, studio: &mut Studio, row: Option<usize>, shorten: bool) {
        let rows = self.rows.clone();
        let keys = studio.prefs.gemini_api_key.expose().to_owned();
        let model = self.model.clone();
        let language = LANGUAGES[self.language].to_owned();
        let natural = self.natural;
        let glossary = self.glossary.clone();
        let (g, cancel) = self.start();
        self.message = "Đang dịch sang tiếng Việt…".into();
        let epoch = project_epoch();
        spawn_in_project(
            move || {
                service::translate(
                    &rows,
                    row,
                    &keys,
                    &model,
                    &language,
                    natural,
                    &glossary,
                    shorten,
                    &cancel,
                    move |done, total| {
                        on_ui_in_project(epoch, move |studio, _, _| {
                            studio.handle(Msg::Translation(TranslationMsg::Progress(
                                g,
                                format!("Đang dịch: {done}/{total} câu…"),
                            )))
                        });
                    },
                )
                .map(Outcome::Translated)
            },
            move |studio, _, _, result| {
                studio.handle(Msg::Translation(TranslationMsg::Finished(g, result)))
            },
        );
    }
    pub fn data(&self, studio: &Studio) -> TranslationData {
        let pages = self.rows.len().div_ceil(20).max(1);
        let page = self.page.min(pages - 1);
        let rows = self
            .rows
            .iter()
            .enumerate()
            .skip(page * 20)
            .take(20)
            .map(|(id, r)| TranslationRowData {
                id: id as i32,
                source: r.source.as_str().into(),
                translated: r.translated.as_str().into(),
                speaker: r.speaker as i32,
                timing: format!("{:.2}s → {:.2}s", r.start, r.end).into(),
                warning: service::warning(r).into(),
            })
            .collect::<Vec<_>>();
        let models = crate::panes::settings::installed(&studio.settings.transcribers)
            .iter()
            .map(|m| slint::SharedString::from(m.name.as_str()))
            .collect::<Vec<_>>();
        TranslationData {
            open: self.open,
            mobile: cfg!(any(target_os = "android", target_os = "ios")),
            busy: self.busy,
            media: self.media,
            source_name: self
                .file
                .as_ref()
                .and_then(|p| p.file_name())
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default()
                .into(),
            language: self.language as i32,
            whisper: self.whisper as i32,
            models: slint::ModelRc::from(std::rc::Rc::new(slint::VecModel::from(models))),
            model: self.model.as_str().into(),
            natural: self.natural,
            glossary: self.glossary.as_str().into(),
            reviewed: self.reviewed,
            offset: self.offset,
            message: self.message.as_str().into(),
            rows: slint::ModelRc::from(std::rc::Rc::new(slint::VecModel::from(rows))),
            total: self.rows.len() as i32,
            translated_count: self
                .rows
                .iter()
                .filter(|r| !r.translated.trim().is_empty())
                .count() as i32,
            page: page as i32,
            pages: pages as i32,
            can_export: self.reviewed
                && !self.rows.is_empty()
                && self.rows.iter().all(|r| !r.translated.trim().is_empty()),
            ready: !concat_speech::api_keys::parse(studio.prefs.gemini_api_key.expose()).is_empty(),
            has_project: studio.session.is_some(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn export_requires_review_and_keeps_source_and_timing() {
        let mut pane = Translation::default();
        pane.rows = vec![Line {
            start: 1.234,
            end: 3.456,
            source: "Hello".into(),
            translated: "Xin chào".into(),
            speaker: 0,
        }];
        assert!(pane.cues(false, false).is_err());
        assert_eq!(pane.cues(false, true).unwrap()[0].text, "Hello");
        pane.reviewed = true;
        let bilingual = pane.cues(true, false).unwrap();
        assert_eq!(bilingual[0].text, "Hello\nXin chào");
        assert_eq!((bilingual[0].start, bilingual[0].end), (1.234, 3.456));
        assert_eq!(srt::parse(&srt::write(&bilingual)).unwrap(), bilingual);
        pane.rows[0].translated.clear();
        assert!(pane.cues(false, false).is_err());
    }
}
