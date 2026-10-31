// SPDX-License-Identifier: AGPL-3.0-or-later
//! Review-first dubbing workspace, owned by the speech sheet.
use super::*;
use crate::ui::{DubbingData, DubbingRowData};
use concat_speech::{
    cloud_tts,
    dubbing::{self, Cue},
    transcribe::TranscribeRequest,
    tts::SpeakRequest,
};
use std::sync::atomic::{AtomicBool, Ordering};

#[derive(Default)]
pub struct Dubbing {
    pub catalog: Vec<concat_speech::voice_catalog::Voice>,
    pub catalog_loading: bool,
    pub gender: i32,
    pub show_paid: bool,
    pub mode: i32,
    pub provider: i32,
    pub preferences_loaded: bool,
    pub options: cloud_tts::VoiceOptions,
    pub gemini_voice: String,
    pub eleven_voice: String,
    pub gemini_model: String,
    pub eleven_model: String,
    pub source: usize,
    pub whisper: usize,
    pub rows: Vec<Cue>,
    pub preview: Option<PathBuf>,
    pub reviewed: bool,
    pub mute_original: bool,
    pub generation: u64,
    pub cancel: Arc<AtomicBool>,
    pub timeline: String,
    pub signature: String,
    pub source_clip: Option<String>,
    pub standalone_srt: Option<(PathBuf, String)>,
    pub standalone_media: Option<(PathBuf, String, f64)>,
    pub mic: Option<concat_host::record::Microphone>,
    pub recording: bool,
    pub vc_method: i32,
    pub vc_preset: i32,
    pub vc_pitch: f32,
    pub vc_speed: f32,
    pub vc_denoise: bool,
}
#[derive(Clone, Debug)]
pub struct DubResult {
    rows: Vec<Cue>,
    preview: Option<PathBuf>,
    listen: bool,
    review_all: bool,
    notice: Option<String>,
}

fn report(epoch: u64) -> impl FnMut(JobStage) + Send + 'static {
    move |stage| {
        on_ui_in_project(epoch, move |studio, _, _| {
            studio.handle(Msg::Speech(SpeechMsg::Progress(stage)))
        });
    }
}
impl Dubbing {
    fn invalidate(&mut self) {
        self.preview = None;
        self.reviewed = false;
    }
    pub fn voice(&self) -> &str {
        if self.provider == 1 {
            if self.gemini_voice.is_empty() {
                "Kore"
            } else {
                &self.gemini_voice
            }
        } else {
            if self.eleven_voice.is_empty() {
                "JBFqnCBsd6RMkjVDRZzb"
            } else {
                &self.eleven_voice
            }
        }
    }
    pub fn model(&self) -> &str {
        if self.mode == 3 && self.vc_method == 1 {
            if self.provider == 1 {
                if self.gemini_model.is_empty() || self.gemini_model.contains("tts") {
                    "gemini-3.5-flash-lite"
                } else {
                    &self.gemini_model
                }
            } else if self.eleven_model.is_empty() || !self.eleven_model.contains("sts") {
                "eleven_multilingual_sts_v2"
            } else {
                &self.eleven_model
            }
        } else if self.provider == 1 {
            if self.gemini_model.is_empty() {
                "gemini-3.5-flash-lite"
            } else {
                &self.gemini_model
            }
        } else if self.eleven_model.is_empty() {
            "eleven_v3"
        } else {
            &self.eleven_model
        }
    }
    pub fn key<'a>(&self, studio: &'a Studio) -> &'a str {
        if self.provider == 1 {
            studio.prefs.gemini_api_key.expose()
        } else {
            studio.prefs.eleven_api_key.expose()
        }
    }
    pub fn restore_preferences(&mut self, studio: &Studio) {
        if self.preferences_loaded {
            return;
        }
        let saved = &studio.prefs.cloud_tts;
        self.gemini_model = saved.gemini_model.clone();
        self.eleven_model = saved.eleven_model.clone();
        self.gemini_voice = saved.gemini_voice.clone();
        self.eleven_voice = saved.eleven_voice.clone();
        self.options = saved.options.clone();
        if self.vc_speed <= 0.0 {
            self.vc_speed = 1.0;
            self.vc_denoise = true;
        }
        self.preferences_loaded = true;
    }
    fn save_preferences(&self, studio: &mut Studio) {
        studio.prefs.cloud_tts = crate::prefs::CloudTtsPrefs {
            gemini_model: self.gemini_model.clone(),
            eleven_model: self.eleven_model.clone(),
            gemini_voice: self.gemini_voice.clone(),
            eleven_voice: self.eleven_voice.clone(),
            options: self.options.clone(),
        };
        studio.prefs.save(&studio.host.dirs);
    }
}
impl SpeechPane {
    pub(crate) fn dub_sources(&self, studio: &Studio) -> Vec<(String, String)> {
        if self.dub.mode == 1 {
            let mut list = Vec::new();
            if let Some((_, ref label)) = self.dub.standalone_srt {
                list.push(("__standalone_srt__".to_owned(), label.clone()));
            }
            list.extend(studio.timeline().tracks.iter().filter_map(|track| {
                let count = studio
                    .timeline()
                    .clips
                    .iter()
                    .filter(|c| c.kind == ClipKind::Text && c.track_id == track.id)
                    .count();
                (count > 0).then(|| (track.id.clone(), format!("{} · {}", track.id, count)))
            }));
            list
        } else {
            let mut list = Vec::new();
            if let Some((_, ref label, _)) = self.dub.standalone_media {
                list.push(("__standalone_media__".to_owned(), label.clone()));
            }
            list.extend(
                studio
                    .timeline()
                    .clips
                    .iter()
                    .filter(|c| c.kind == ClipKind::Video || c.kind == ClipKind::Audio)
                    .filter(|c| {
                        studio
                            .project()
                            .media_by_id(&c.media_id)
                            .is_some_and(|m| m.has_audio)
                    })
                    .map(|c| (c.id.clone(), format!("{} · {:.2}s", c.name, c.start))),
            );
            list
        }
    }
    fn dub_signature(&self, studio: &Studio) -> String {
        let Some((id, _)) = self.dub_sources(studio).get(self.dub.source).cloned() else {
            return String::new();
        };
        if id == "__standalone_srt__" {
            return self
                .dub
                .standalone_srt
                .as_ref()
                .map(|(p, _)| p.to_string_lossy().into_owned())
                .unwrap_or_default();
        }
        if id == "__standalone_media__" {
            return self
                .dub
                .standalone_media
                .as_ref()
                .map(|(p, _, _)| p.to_string_lossy().into_owned())
                .unwrap_or_default();
        }
        let clips: Vec<_> = studio
            .timeline()
            .clips
            .iter()
            .filter(|c| {
                if self.dub.mode == 1 {
                    c.track_id == id && c.kind == ClipKind::Text
                } else {
                    c.id == id
                }
            })
            .collect();
        serde_json::to_string(&clips).unwrap_or_default()
    }
    fn dub_clear(&mut self) {
        self.dub.rows.clear();
        self.dub.invalidate();
        self.message.clear();
    }
    pub fn dub_action(
        &mut self,
        action: i32,
        index: i32,
        text: String,
        value: f32,
        studio: &mut Studio,
    ) {
        if self.running && action != 16 {
            return;
        }
        let previous_options = self.dub.options.clone();
        let previous_model = self.dub.model().to_owned();
        let previous_voice = self.dub.voice().to_owned();
        if matches!(
            action,
            0 | 1 | 3 | 4 | 5 | 6 | 7 | 8 | 9 | 12 | 13 | 14 | 18..=25 | 37 | 40 | 42 | 43 | 44 | 45 | 47 | 48
        ) {
            studio.close_source();
        }
        match action {
            0 => {
                self.dub.mode = index.clamp(0, 3);
                self.dub.mute_original = self.dub.mode >= 2;
                if self.dub.vc_speed <= 0.0 {
                    self.dub.vc_speed = 1.0;
                    self.dub.vc_denoise = true;
                }
                self.dub.source = 0;
                self.dub_clear();
            }
            1 => {
                self.dub.provider = index.clamp(0, 2);
                for row in &mut self.dub.rows {
                    row.audio = None;
                }
                self.dub.invalidate();
            }
            2 => {
                self.open = false;
                studio.settings.tab = 0;
                studio.settings.open = true;
            }
            3 => {
                if text == self.dub.voice() {
                    return;
                }
                if self.dub.provider == 1 {
                    self.dub.gemini_voice = text
                } else {
                    self.dub.eleven_voice = text
                }
                for row in &mut self.dub.rows {
                    row.audio = None;
                }
                self.dub.invalidate();
            }
            4 => {
                if text == self.dub.model() {
                    return;
                }
                if self.dub.provider == 1 {
                    self.dub.gemini_model = text
                } else {
                    self.dub.eleven_model = text
                }
                for row in &mut self.dub.rows {
                    row.audio = None;
                }
                self.dub.invalidate();
            }
            5 => {
                self.dub.source = index.max(0) as usize;
                self.dub_clear();
            }
            6 => self.dub_load_track(studio),
            7 => self.dub_transcribe(studio),
            8 => self.dub_generate(None, studio),
            9 => self.dub_generate(Some(index.max(0) as usize), studio),
            10 => self.dub_listen(index, studio),
            11 => self.dub_apply(studio),
            12 => {
                if let Some(row) = self.dub.rows.get_mut(index.max(0) as usize) {
                    if row.text == text {
                        return;
                    }
                    row.text = text;
                    row.audio = None;
                }
                self.dub.invalidate();
            }
            13 | 14 => {
                if value.is_finite() {
                    if let Some(row) = self.dub.rows.get_mut(index.max(0) as usize) {
                        if action == 13 {
                            row.start = value.max(0.0) as f64
                        } else {
                            row.end = value.max(0.0) as f64
                        }
                    }
                    self.dub.invalidate();
                }
            }
            15 => self.dub.whisper = index.max(0) as usize,
            16 => studio.close_source(),
            17 => self.dub.mute_original = index != 0,
            18 => self.dub.options.delivery = text,
            19 if value.is_finite() => self.dub.options.stability = value.clamp(0.0, 1.0),
            20 if value.is_finite() => self.dub.options.similarity = value.clamp(0.0, 1.0),
            21 if value.is_finite() => self.dub.options.style = value.clamp(0.0, 1.0),
            22 if value.is_finite() => self.dub.options.speed = value.clamp(0.7, 1.2),
            23 => self.dub.options.speaker_boost = index != 0,
            24 => {
                let is_sts = self.dub.mode == 3 && self.dub.vc_method == 1;
                let list = if is_sts {
                    cloud_tts::sts_models(self.dub.provider)
                } else {
                    cloud_tts::models(self.dub.provider)
                };
                if let Some(model) = list.get(index.max(0) as usize)
                {
                    if self.dub.provider == 1 {
                        self.dub.gemini_model = (*model).into();
                    } else {
                        self.dub.eleven_model = (*model).into();
                    }
                } else if self.dub.provider == 1 {
                    self.dub.gemini_model = "custom".into();
                } else {
                    self.dub.eleven_model = "custom".into();
                }
            }
            25 => self.dub.options = cloud_tts::VoiceOptions::default(),
            32 if value.is_finite() => self.dub.options.temperature = value.clamp(0.0, 2.0),
            26 => self.dub.gender = index.clamp(0, 2),
            27 => self.dub.show_paid = index != 0,
            28 => {
                let voices = self.dub_voices();
                if let Some(voice) = voices.get(index.max(0) as usize) {
                    let id = voice.id.clone();
                    self.dub_action(3, 0, id, 0.0, studio);
                }
            }
            29 if !self.dub.catalog_loading => {
                self.dub.catalog_loading = true;
                let key = self.dub.key(studio).to_owned();
                spawn_in_project(
                    move || concat_speech::voice_catalog::eleven(&key),
                    move |studio, _, _, result| {
                        studio.handle(Msg::Speech(SpeechMsg::VoiceCatalog(result)))
                    },
                );
            }
            30 => {
                self.dub.options.accent = index.clamp(0, 3);
                self.dub.invalidate();
            }
            31 => {
                self.dub.options.delivery_preset = index.clamp(0, 6);
                if self.dub.provider == 2 && cloud_tts::has_extended_settings(self.dub.model()) {
                    let (stability, style) = [
                        (0.5, 0.0),
                        (0.5, 0.0),
                        (0.5, 0.35),
                        (1.0, 0.0),
                        (0.0, 0.6),
                        (1.0, 0.1),
                        (0.0, 0.5),
                    ][index.clamp(0, 6) as usize];
                    self.dub.options.stability = stability;
                    self.dub.options.style = style;
                }
                self.dub.invalidate();
            }
            33 => {
                if self.dub.mode == 1 {
                    crate::host::pick_files_then(
                        "Chọn file phụ đề (.SRT)".to_owned(),
                        Some(("SubRip Subtitles".to_owned(), &["srt"])),
                        |studio, _, _, paths| {
                            if let Some(path) = paths.into_iter().next() {
                                studio.handle(Msg::Speech(SpeechMsg::DubbingAction(
                                    34,
                                    1,
                                    path.to_string_lossy().into_owned(),
                                    0.0,
                                )));
                            }
                        },
                    );
                } else if self.dub.mode == 2 || self.dub.mode == 3 {
                    crate::host::pick_files_then(
                        "Chọn file Video / Audio".to_owned(),
                        Some((
                            "Video & Audio".to_owned(),
                            &[
                                "mp4", "mov", "mkv", "webm", "avi", "m4v", "mp3", "wav", "m4a",
                                "flac", "ogg", "aac",
                            ],
                        )),
                        |studio, _, _, paths| {
                            if let Some(path) = paths.into_iter().next() {
                                studio.handle(Msg::Speech(SpeechMsg::DubbingAction(
                                    34,
                                    2,
                                    path.to_string_lossy().into_owned(),
                                    0.0,
                                )));
                            }
                        },
                    );
                }
            }
            34 => {
                let path = PathBuf::from(&text);
                if index == 1 {
                    let label = path
                        .file_name()
                        .map(|n| n.to_string_lossy().into_owned())
                        .unwrap_or_else(|| "subtitles.srt".to_owned());
                    self.dub.standalone_srt = Some((path, label));
                    self.dub.source = 0;
                    self.dub_load_track(studio);
                } else if index == 2 {
                    match media::probe(&path.to_string_lossy()) {
                        Ok(summary) => {
                            let duration = summary.duration.unwrap_or(0.0);
                            if summary.audio.is_none() || duration <= 0.0 {
                                self.message =
                                    "File đã chọn không có luồng âm thanh hợp lệ.".to_owned();
                                return;
                            }
                            let name = path
                                .file_name()
                                .map(|n| n.to_string_lossy().into_owned())
                                .unwrap_or_else(|| summary.path);
                            let label = format!("{name} · {duration:.1}s");
                            self.dub.standalone_media = Some((path, label, duration));
                            self.dub.source = 0;
                            self.dub_clear();
                        }
                        Err(e) => {
                            self.message = crate::host::probe_error(&e);
                        }
                    }
                }
            }
            35 => self.dub_save_wav(studio),
            36 => {
                let dst = PathBuf::from(&text);
                let Some(ref src) = self.dub.preview else {
                    return;
                };
                match std::fs::copy(src, &dst) {
                    Ok(_) => {
                        let _ = crate::platform::reveal(&dst.to_string_lossy());
                        studio.notify(
                            &format!("Đã lưu file âm thanh tại {}", dst.display()),
                            false,
                        );
                    }
                    Err(e) => {
                        self.message = format!("Không thể lưu file âm thanh: {e}");
                    }
                }
            }
            37 => self.dub_resegment_gemini(studio),
            40 => {
                if self.dub.recording {
                    self.dub.recording = false;
                    let Some(mic) = self.dub.mic.take() else {
                        return;
                    };
                    match mic.end() {
                        Ok(take) => {
                            let duration = take.seconds.max(0.1);
                            let dst = take.path;
                            let name = dst
                                .file_stem()
                                .map(|s| s.to_string_lossy().into_owned())
                                .unwrap_or_else(|| "ban-thu-micro".to_owned());
                            let label = format!("{name} · {duration:.1}s");
                            self.dub.standalone_media = Some((dst.clone(), label, duration));
                            self.dub.source = 0;
                            self.dub_clear();
                            if studio.session.is_some()
                                && let Ok(summary) = media::probe(&dst.to_string_lossy())
                            {
                                let mut item = summary.to_new_media();
                                item.name = format!("Bản thu Micro ({duration:.1}s)");
                                item.origin = Some(MediaOrigin::Speech);
                                let _ = studio.apply(Command::AddMedia { item });
                            }
                            studio.notify(
                                &format!("Đã thu âm xong ({duration:.1}s). Sẵn sàng biến đổi giọng nói!"),
                                false,
                            );
                        }
                        Err(e) => {
                            self.message = format!("Không thể hoàn tất bản thu âm Micro: {e}");
                        }
                    }
                } else {
                    let audio_dir = studio.standalone_project_dir().join("audio");
                    let _ = std::fs::create_dir_all(&audio_dir);
                    let stamp = std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .map(|d| d.as_secs())
                        .unwrap_or(0);
                    let dst = audio_dir.join(format!("ban-thu-micro-{stamp}.wav"));
                    match concat_host::record::Microphone::open(None) {
                        Ok(mic) => match mic.begin(dst) {
                            Ok(()) => {
                                self.dub.mic = Some(mic);
                                self.dub.recording = true;
                                self.message.clear();
                                studio.notify(
                                    "Đang thu âm từ Micro… Hãy nói vào Micro rồi bấm Dừng thu âm.",
                                    false,
                                );
                            }
                            Err(e) => {
                                self.message = format!("Không thể bắt đầu thu âm Micro: {e}");
                            }
                        },
                        Err(e) => {
                            self.message = format!("Không thể mở Micro: {e}");
                        }
                    }
                }
            }
            41 => {
                let Some((id, label)) = self.dub_sources(studio).get(self.dub.source).cloned() else {
                    self.message = t("dubbing.chooseSource");
                    return;
                };
                if id == "__standalone_media__" {
                    if let Some((ref path, _, _)) = self.dub.standalone_media {
                        let card = format!("dub-orig:{}", path.display());
                        studio.preview_file(&card, &label, path);
                    }
                } else if let Some(clip) = studio.clip(&id).cloned()
                    && let Some(media) = studio.project().media_by_id(&clip.media_id)
                {
                    let path = PathBuf::from(&media.path);
                    let card = format!("dub-orig:{}", path.display());
                    studio.preview_file(&card, &clip.name, &path);
                }
            }
            42 => {
                self.dub.vc_method = index.clamp(0, 2);
                if self.dub.vc_method == 1 && self.dub.provider == 0 {
                    if !studio.prefs.eleven_api_key.expose().trim().is_empty() {
                        self.dub.provider = 2;
                    } else {
                        self.dub.provider = 1;
                    }
                }
                self.dub.invalidate();
            }
            43 => {
                self.dub.vc_preset = index.clamp(0, 9);
                let (pitch, denoise) = cloud_tts::dsp_preset_defaults(self.dub.vc_preset);
                self.dub.vc_pitch = pitch;
                self.dub.vc_denoise = denoise;
                if self.dub.vc_speed <= 0.0 {
                    self.dub.vc_speed = 1.0;
                }
                self.dub.invalidate();
            }
            44 if value.is_finite() => {
                self.dub.vc_pitch = value.clamp(-12.0, 12.0);
                self.dub.invalidate();
            }
            45 if value.is_finite() => {
                self.dub.vc_speed = value.clamp(0.5, 2.0);
                self.dub.invalidate();
            }
            46 => {
                self.dub.mic = None;
                self.dub.recording = false;
                self.open = false;
                studio.handle(Msg::Gemini(crate::panes::gemini_studio::GeminiMsg::Action(
                    0,
                    0,
                    String::new(),
                    0.0,
                )));
            }
            47 => {
                self.dub.vc_denoise = index != 0;
                self.dub.invalidate();
            }
            48 => self.dub_transform_voice(studio),
            49 => {
                crate::host::pick_files_then(
                    "Chọn tệp âm thanh giọng mẫu".to_owned(),
                    Some((
                        "Audio & Video".to_owned(),
                        &[
                            "wav", "mp3", "m4a", "flac", "ogg", "aac", "mp4", "mov", "mkv", "webm",
                        ],
                    )),
                    |studio, _, _, paths| {
                        if let Some(path) = paths.into_iter().next() {
                            if studio.session.is_some()
                                && let Ok(summary) = media::probe(&path.to_string_lossy())
                                && summary.audio.is_some()
                            {
                                let item = summary.to_new_media();
                                let _ = studio.apply(Command::AddMedia { item });
                                let count = studio.speech.samples(&studio).len();
                                if count > 0 {
                                    studio.speech.sample = count - 1;
                                    studio.speech.use_sample = true;
                                }
                            } else {
                                studio.handle(Msg::Speech(SpeechMsg::ImportVoice(path)));
                            }
                        }
                    },
                );
            }

            _ => {}
        }
        if matches!(action, 3 | 4 | 18..=25 | 30 | 31 | 32)
            && (previous_options != self.dub.options
                || previous_model != self.dub.model()
                || previous_voice != self.dub.voice())
        {
            for row in &mut self.dub.rows {
                row.audio = None;
            }
            self.dub.invalidate();
            self.dub.save_preferences(studio);
        }
    }
    fn dub_resegment_gemini(&mut self, studio: &mut Studio) {
        if self.dub.rows.is_empty() {
            return;
        }
        let api_key = studio.prefs.gemini_api_key.expose().trim().to_string();
        if api_key.is_empty() {
            self.message = t("captions.geminiKeyRequired");
            return;
        }
        let segments: Vec<concat_speech::transcribe::Segment> = self
            .dub
            .rows
            .iter()
            .map(|row| concat_speech::transcribe::Segment {
                start: row.start,
                end: row.end,
                text: row.text.clone(),
            })
            .collect();
        let (generation, cancel) = self.dub_start(studio);
        self.dub.invalidate();
        spawn_in_project(
            move || {
                let resegmented = concat_speech::gemini::resegment_for_dubbing(
                    &segments,
                    &api_key,
                    &cancel,
                )?;
                cloud_tts::check_cancel(&cancel)?;
                let rows = resegmented
                    .into_iter()
                    .map(|s| Cue {
                        text: s.text,
                        start: s.start,
                        end: s.end,
                        ..Cue::default()
                    })
                    .collect();
                Ok(DubResult {
                    rows,
                    preview: None,
                    listen: false,
                    review_all: false,
                    notice: None,
                })
            },
            move |studio, _, _, result| {
                if result.is_ok() {
                    studio.notify("Đã tối ưu ngắt câu & ngữ điệu tự nhiên bằng Gemini AI.", false);
                }
                studio.handle(Msg::Speech(SpeechMsg::DubFinished(
                    generation,
                    Box::new(result),
                )))
            },
        );
    }
    fn dub_save_wav(&mut self, studio: &mut Studio) {
        let Some(ref _preview) = self.dub.preview else {
            self.message = "Chưa có bản âm thanh để lưu. Hãy bấm Tạo trước.".to_owned();
            return;
        };
        let (folder, name) = if self.dub.mode == 1
            && let Some((ref p, _)) = self.dub.standalone_srt
        {
            let folder = p
                .parent()
                .map(PathBuf::from)
                .unwrap_or_else(|| PathBuf::from(crate::studio::home_folder("Documents")));
            let stem = p
                .file_stem()
                .map(|s| s.to_string_lossy().into_owned())
                .unwrap_or_else(|| "thuyet-minh".to_owned());
            (folder, format!("{stem}-thuyet-minh.wav"))
        } else if (self.dub.mode == 2 || self.dub.mode == 3)
            && let Some((ref p, _, _)) = self.dub.standalone_media
        {
            let folder = p
                .parent()
                .map(PathBuf::from)
                .unwrap_or_else(|| PathBuf::from(crate::studio::home_folder("Documents")));
            let stem = p
                .file_stem()
                .map(|s| s.to_string_lossy().into_owned())
                .unwrap_or_else(|| "doi-giong".to_owned());
            (folder, format!("{stem}-doi-giong.wav"))
        } else {
            let folder = studio
                .session
                .as_ref()
                .map(|s| PathBuf::from(s.path()))
                .unwrap_or_else(|| PathBuf::from(crate::studio::home_folder("Documents")));
            (folder, "giong-doc-ai.wav".to_owned())
        };
        crate::host::after_dialog(
            move || {
                let chosen = crate::platform::save_file(
                    "Lưu file âm thanh (.WAV)",
                    &folder,
                    &name,
                    ("WAV Audio", &["wav"]),
                )?;
                Some(
                    if chosen
                        .extension()
                        .is_some_and(|ext| ext.eq_ignore_ascii_case("wav"))
                    {
                        chosen
                    } else {
                        chosen.with_extension("wav")
                    },
                )
            },
            |studio, _, _, path| {
                studio.handle(Msg::Speech(SpeechMsg::DubbingAction(
                    36,
                    0,
                    path.to_string_lossy().into_owned(),
                    0.0,
                )));
            },
        );
    }
    fn dub_load_track(&mut self, studio: &Studio) {
        let Some((track, _)) = self.dub_sources(studio).get(self.dub.source).cloned() else {
            self.message = t("dubbing.chooseSource");
            return;
        };
        let mut rows: Vec<_> = if track == "__standalone_srt__" {
            let Some((ref path, _)) = self.dub.standalone_srt else {
                self.message = t("dubbing.chooseSource");
                return;
            };
            let cues = match std::fs::read(path)
                .map_err(|e| e.to_string())
                .and_then(|bytes| {
                    concat_project::srt::parse(&String::from_utf8_lossy(&bytes))
                        .map_err(|e| e.to_string())
                }) {
                Ok(cues) => cues,
                Err(error) => {
                    self.message = tf("captions.couldNotReadSubtitles", &[&error]);
                    return;
                }
            };
            cues.into_iter()
                .filter(|c| !c.text.trim().is_empty())
                .map(|c| Cue {
                    text: c.text.trim().to_owned(),
                    start: c.start,
                    end: c.end,
                    ..Cue::default()
                })
                .collect()
        } else {
            studio
                .timeline()
                .clips
                .iter()
                .filter(|c| c.kind == ClipKind::Text && c.track_id == track)
                .filter_map(|c| {
                    c.text.as_ref().map(|text| Cue {
                        text: text.content.clone(),
                        start: c.start,
                        end: c.start + c.duration,
                        ..Cue::default()
                    })
                })
                .collect()
        };
        rows.sort_by(|a, b| a.start.total_cmp(&b.start));
        let rows: Vec<Cue> = concat_speech::caption_layout::clean_stumbles_and_orphans(
            rows.into_iter()
                .map(|c| concat_speech::transcribe::Segment {
                    start: c.start,
                    end: c.end,
                    text: c.text,
                })
                .collect(),
        )
        .into_iter()
        .map(|s| Cue {
            text: s.text,
            start: s.start,
            end: s.end,
            ..Cue::default()
        })
        .collect();
        if let Err(error) = dubbing::validate(&rows) {
            self.message = error;
            return;
        }
        self.dub.rows = rows;
        self.dub.timeline = studio.timeline().id.clone();
        self.dub.signature = self.dub_signature(studio);
        self.dub.source_clip = None;
        self.dub.invalidate();
        self.message.clear();
    }
    fn dub_start(&mut self, studio: &Studio) -> (u64, Arc<AtomicBool>) {
        self.dub.generation += 1;
        self.dub.cancel = Arc::default();
        self.dub.timeline = studio.timeline().id.clone();
        self.running = true;
        self.message.clear();
        self.pace = Pace::new(vec![(Stage::Loading, 2.0), (Stage::Working, 20.0)]);
        (self.dub.generation, Arc::clone(&self.dub.cancel))
    }
    fn dub_transcribe(&mut self, studio: &Studio) {
        let Some((id, _)) = self.dub_sources(studio).get(self.dub.source).cloned() else {
            self.message = t("dubbing.chooseSource");
            return;
        };
        let (media_path, audio_stream, source_start, window, clip_start, clip_speed, clip_duration) =
            if id == "__standalone_media__" {
                let Some((ref path, _, duration)) = self.dub.standalone_media else {
                    self.message = t("dubbing.chooseSource");
                    return;
                };
                (
                    path.to_string_lossy().into_owned(),
                    None,
                    0.0,
                    duration,
                    0.0,
                    1.0,
                    duration,
                )
            } else {
                let Some(clip) = studio.clip(&id).cloned() else {
                    return;
                };
                if clip.speed_curve.is_some() {
                    self.message = t("dubbing.constantSpeed");
                    return;
                }
                let Some(media) = studio.project().media_by_id(&clip.media_id) else {
                    return;
                };
                (
                    media.path.clone(),
                    clip.audio_stream,
                    clip.source_start,
                    clip.duration * clip.speed,
                    clip.start,
                    clip.speed,
                    clip.duration,
                )
            };
        let Some(model) = installed(&studio.settings.transcribers)
            .get(self.dub.whisper)
            .map(|m| m.id.clone())
        else {
            self.message = t("captions.downloadTranscriberModelFirst");
            return;
        };
        let request = TranscribeRequest {
            path: media_path,
            audio_stream,
            source_start,
            window,
            model_id: model,
        };
        self.dub.signature = self.dub_signature(studio);
        self.dub.source_clip = (id != "__standalone_media__").then_some(id);
        self.dub.invalidate();
        let (generation, cancel) = self.dub_start(studio);
        let dirs = studio.host.dirs.clone();
        let transcriber = Arc::clone(&studio.host.transcriber);
        let epoch = crate::host::project_epoch();
        spawn_in_project(
            move || {
                let words = transcriber.transcribe_words(&dirs, &request, report(epoch))?;
                let segments = dubbing::sentences(&words);
                cloud_tts::check_cancel(&cancel)?;
                let rows = segments
                    .into_iter()
                    .map(|s| Cue {
                        text: s.text,
                        start: clip_start + s.start / clip_speed,
                        end: (clip_start + s.end / clip_speed).min(clip_start + clip_duration),
                        ..Cue::default()
                    })
                    .collect();
                Ok(DubResult {
                    rows,
                    preview: None,
                    listen: false,
                    review_all: false,
                    notice: None,
                })
            },
            move |studio, _, _, result| {
                studio.handle(Msg::Speech(SpeechMsg::DubFinished(
                    generation,
                    Box::new(result),
                )))
            },
        );
    }
    fn dub_local_request(&self, text: String, studio: &Studio) -> Result<SpeakRequest, String> {
        let model = installed(&studio.settings.voices)
            .get(self.model)
            .map(|m| m.id.clone())
            .ok_or_else(|| t("speech.downloadVoiceModelFirst"))?;
        let saved = self
            .saved_voice
            .filter(|_| !self.use_sample)
            .and_then(|i| self.saved.get(i));
        let (voice, reference) = if let Some(saved) = saved {
            (
                self.sample_voice()
                    .ok_or_else(|| t("speech.needsCloningModel"))?,
                Some(Reference {
                    path: saved.sample().to_string_lossy().into(),
                    start: 0.0,
                }),
            )
        } else if self.use_sample {
            let sample = self
                .sample_media(studio)
                .ok_or_else(|| t("speech.pickSampleVoice"))?;
            (
                self.sample_voice()
                    .ok_or_else(|| t("speech.modelReadsOnlyOwn"))?,
                Some(Reference {
                    path: sample.path.clone(),
                    start: self.reference_offset(sample.duration.unwrap_or(0.0)),
                }),
            )
        } else {
            (
                self.offered()
                    .get(self.voice)
                    .map(|v| v.id)
                    .ok_or_else(|| t("speech.noVoiceRead"))?,
                None,
            )
        };
        let project = studio
            .standalone_project_dir()
            .to_string_lossy()
            .into_owned();
        Ok(SpeakRequest {
            model_id: model,
            voice,
            reference,
            text,
            speed: self.rate(),
            pauses: self.reads_at_a_pace().then_some(self.pauses),
            steps: (self.family == Family::Pocket).then(|| STEPS[self.quality.min(2)]),
            project,
        })
    }
    pub fn dub_generate(&mut self, row: Option<usize>, studio: &mut Studio) {
        if self.dub.provider == 2
            && !self.dub.show_paid
            && !self
                .dub
                .catalog
                .iter()
                .any(|v| v.id == self.dub.voice() && v.free)
        {
            self.message = t("dubbing.selectFreeVoice");
            return;
        }
        if self.running {
            return;
        }
        if self.dub.mode == 0 {
            if self.text.trim().is_empty() {
                self.message = t("speech.nothingToReadYet");
                return;
            }
            let start = self
                .clip
                .as_ref()
                .and_then(|id| studio.clip(id))
                .map_or(studio.playhead as f64, |c| c.start);
            self.dub.rows = vec![Cue {
                text: self.text.clone(),
                start,
                end: start + 1.0,
                ..Cue::default()
            }];
            self.dub.source_clip = None;
            self.dub.signature.clear();
        }
        if let Err(error) = dubbing::validate(&self.dub.rows) {
            self.message = error;
            return;
        }
        let merged = row.is_none() && self.dub.mode != 0;
        let project = studio
            .standalone_project_dir()
            .to_string_lossy()
            .into_owned();
        let mut rows = self.dub.rows.clone();
        let text = if let Some(index) = row {
            let Some(cue) = rows.get(index) else {
                return;
            };
            cue.text.clone()
        } else {
            rows.iter()
                .map(|r| r.text.trim())
                .collect::<Vec<_>>()
                .join("\n")
        };
        if text.chars().count() > 20000 {
            self.message = t("dubbing.textLimit");
            return;
        }
        let local = if self.dub.provider == 0 {
            match self.dub_local_request(text.clone(), studio) {
                Ok(r) => Some(r),
                Err(e) => {
                    self.message = e;
                    return;
                }
            }
        } else {
            if self.dub.key(studio).trim().is_empty() {
                self.message = t("dubbing.keyRequired");
                return;
            }
            None
        };
        if let Some(request) = local.as_ref() {
            if !self.use_sample && self.saved_voice.is_none() {
                studio.prefs.tts_voice = Some(request.voice);
                studio.prefs.tts_model = Some(request.model_id.clone());
                studio.prefs.save(&studio.host.dirs);
            }
        }
        let whisper = if merged {
            match installed(&studio.settings.transcribers).get(self.dub.whisper) {
                Some(m) => Some(m.id.clone()),
                None => {
                    self.message = t("captions.downloadTranscriberModelFirst");
                    return;
                }
            }
        } else {
            None
        };
        let (provider, key, model, voice) = (
            self.dub.provider,
            self.dub.key(studio).to_owned(),
            self.dub.model().to_owned(),
            self.dub.voice().to_owned(),
        );
        let mut options = self.dub.options.clone();
        if self.dub.provider == 1 {
            let accent = [
                "",
                "Speak Vietnamese with a Northern Vietnamese accent.",
                "Speak Vietnamese with a Central Vietnamese accent.",
                "Speak Vietnamese with a Southern Vietnamese accent.",
            ][self.dub.options.accent.clamp(0, 3) as usize];
            let style = [
                "",
                "Natural conversational delivery.",
                "Warm storytelling delivery.",
                "Clear professional newsreader delivery.",
                "Energetic advertising delivery.",
                "Calm gentle delivery.",
                "Cheerful expressive delivery.",
            ][self.dub.options.delivery_preset.clamp(0, 6) as usize];
            options.delivery = format!("{accent} {style} {}", options.delivery)
                .trim()
                .to_owned();
        }
        let speech = Arc::clone(&studio.host.speech);
        let transcriber = Arc::clone(&studio.host.transcriber);
        let dirs = studio.host.dirs.clone();
        let mode = self.dub.mode;
        let style_chain = self
            .style
            .and_then(|i| STYLES.get(i))
            .map(|style| {
                let filters = style
                    .effects
                    .iter()
                    .map(|id| concat_project::model::AppliedFilter::new(*id))
                    .collect::<Vec<_>>();
                concat_effects::Catalogue::builtin().audio_chain(&filters)
            })
            .unwrap_or_default();
        self.dub.invalidate();
        let (generation, cancel) = self.dub_start(studio);
        let epoch = crate::host::project_epoch();
        spawn_in_project(
            move || {
                cloud_tts::check_cancel(&cancel)?;
                let mut path = if let Some(request) = local {
                    PathBuf::from(speech.speak(&dirs, &request, report(epoch))?.path)
                } else {
                    cloud_tts::synthesize(
                        provider, &key, &model, &voice, &text, &project, &cancel, &options,
                    )?
                };
                cloud_tts::check_cancel(&cancel)?;
                if !style_chain.is_empty() {
                    let mut decoder = concat_media::AudioDecoder::open(
                        &path,
                        &concat_media::AudioOptions {
                            filters: vec![style_chain],
                            rate: 24000,
                            channels: 1,
                            format: concat_media::SampleFormat::F32,
                            ..Default::default()
                        },
                    )
                    .map_err(|e| e.to_string())?;
                    let samples = decoder.collect_f32().map_err(|e| e.to_string())?;
                    path = cloud_tts::output(&project, "dub-style")?;
                    cloud_tts::write_wav(&path, &samples, 24000)?;
                }
                let summary = media::probe(&path.to_string_lossy())?;
                let duration = summary.duration.ok_or("Generated audio has no duration")?;
                if merged {
                    let request = TranscribeRequest {
                        path: path.to_string_lossy().into(),
                        audio_stream: None,
                        source_start: 0.0,
                        window: duration,
                        model_id: whisper.unwrap(),
                    };
                    let lines = rows.iter().map(|r| r.text.clone()).collect::<Vec<_>>();
                    let aligned = transcriber
                        .transcribe_words(&dirs, &request, report(epoch))
                        .and_then(|words| dubbing::align(&lines, &words));
                    let mut spans = match aligned {
                        Ok(spans) => spans,
                        Err(error) => {
                            cloud_tts::check_cancel(&cancel)?;
                            // The full recording is valid even when sentence boundaries are not.
                            // Preview it without marking any sentence aligned or allowing Apply.
                            return Ok(DubResult {
                                rows,
                                preview: Some(path),
                                listen: true,
                                review_all: false,
                                notice: Some(error),
                            });
                        }
                    };
                    if let Some(last) = spans.last_mut() {
                        last.1 = duration;
                    }
                    for (cue, (start, mut end)) in rows.iter_mut().zip(spans) {
                        let max_dur = (cue.end - cue.start).max(0.1) * 3.8;
                        if end - start > max_dur {
                            end = start + max_dur;
                        }
                        cue.audio = Some(path.clone());
                        cue.audio_start = start;
                        cue.audio_end = end;
                    }
                } else {
                    let index = row.unwrap_or(0);
                    let cue = &mut rows[index];
                    cue.audio = Some(path);
                    cue.audio_start = 0.0;
                    if mode == 0 {
                        cue.audio_end = duration;
                        cue.end = cue.start + duration;
                    } else {
                        let max_dur = (cue.end - cue.start).max(0.1) * 3.8;
                        cue.audio_end = duration.min(max_dur);
                    }
                }
                cloud_tts::check_cancel(&cancel)?;
                // Speech generation succeeded. Preview/rendering errors must not discard it.
                let (preview, notice) = if dubbing::missing_audio(&rows).is_empty() {
                    match dubbing::render(&rows, &project, &cancel) {
                        Ok(path) => (Some(path), None),
                        Err(error) => (None, Some(error)),
                    }
                } else {
                    (None, None)
                };
                Ok(DubResult {
                    rows,
                    preview,
                    listen: false,
                    review_all: false,
                    notice,
                })
            },
            move |studio, _, _, result| {
                studio.handle(Msg::Speech(SpeechMsg::DubFinished(
                    generation,
                    Box::new(result),
                )))
            },
        );
    }
    fn dub_listen(&mut self, index: i32, studio: &mut Studio) {
        if self.running {
            return;
        }
        if index < 0 {
            if let Some(path) = self.dub.preview.clone() {
                if path.is_file() {
                    let card = format!("dub:{}", path.display());
                    studio.preview_file(&card, &t("dubbing.preview"), &path);
                    self.dub.reviewed = studio
                        .source
                        .as_ref()
                        .is_some_and(|source| source.card == card);
                    return;
                } else {
                    self.dub.preview = None;
                }
            }
        }
        let rows = if index < 0 {
            self.dub.rows.clone()
        } else {
            let Some(cue) = self.dub.rows.get(index as usize).cloned() else {
                return;
            };
            vec![cue]
        };
        let pending = dubbing::missing_audio(&rows);
        if !pending.is_empty() {
            let numbers = if index < 0 {
                pending
            } else {
                vec![index as usize + 1]
            };
            self.message = tf(
                "dubbing.missingAudio",
                &[&numbers
                    .iter()
                    .map(usize::to_string)
                    .collect::<Vec<_>>()
                    .join(", ")],
            );
            studio.notify(&self.message, true);
            return;
        }
        let project = studio
            .standalone_project_dir()
            .to_string_lossy()
            .into_owned();
        let (generation, cancel) = self.dub_start(studio);
        let all = self.dub.rows.clone();
        spawn_in_project(
            move || {
                let preview = dubbing::render(&rows, &project, &cancel)?;
                Ok(DubResult {
                    rows: all,
                    preview: Some(preview),
                    listen: true,
                    review_all: index < 0,
                    notice: None,
                })
            },
            move |studio, _, _, result| {
                studio.handle(Msg::Speech(SpeechMsg::DubFinished(
                    generation,
                    Box::new(result),
                )))
            },
        );
    }
    pub fn dub_finished(
        &mut self,
        generation: u64,
        result: Result<DubResult, String>,
        studio: &mut Studio,
    ) {
        if generation != self.dub.generation {
            return;
        }
        self.running = false;
        if self.dub.cancel.load(Ordering::Relaxed) {
            return;
        }
        if self.dub.timeline != studio.timeline().id {
            self.dub_clear();
            self.message = t("dubbing.timelineChanged");
            return;
        }
        match result {
            Ok(result) => {
                self.message = result
                    .notice
                    .as_ref()
                    .map(|error| tf("dubbing.previewFailedKeptAudio", &[error]))
                    .unwrap_or_default();
                self.dub.rows = result.rows;
                if result.review_all || !result.listen {
                    self.dub.preview = result.preview.clone();
                    self.dub.reviewed = false;
                }
                if result.listen {
                    if let Some(path) = result.preview {
                        let card = format!("dub:{}", path.display());
                        studio.preview_file(&card, &t("dubbing.preview"), &path);
                        if result.review_all {
                            self.dub.reviewed = studio
                                .source
                                .as_ref()
                                .is_some_and(|source| source.card == card);
                        }
                    }
                }
            }
            Err(error) => {
                self.message = error.clone();
                studio.notify(&format!("Lỗi nghe thử: {error}"), true);
            }
        }
    }
    fn dub_apply(&mut self, studio: &mut Studio) {
        if self.running || !self.dub.reviewed {
            return;
        }
        if studio.session.is_none() {
            self.dub_save_wav(studio);
            return;
        }
        if self.dub.timeline != studio.timeline().id
            || (self.dub.mode != 0 && self.dub.signature != self.dub_signature(studio))
        {
            self.message = t("dubbing.sourceChanged");
            return;
        }
        let Some(path) = self.dub.preview.clone() else {
            return;
        };
        if let Err(error) = dubbing::validate(&self.dub.rows) {
            self.message = error;
            return;
        }
        let summary = match media::probe(&path.to_string_lossy()) {
            Ok(s) => s,
            Err(e) => {
                self.message = e;
                return;
            }
        };
        let mut item = summary.to_new_media();
        item.origin = Some(MediaOrigin::Speech);
        let media_id = studio.apply(Command::AddMedia { item }).or_else(|| {
            studio
                .project()
                .media
                .iter()
                .find(|m| m.path == summary.path)
                .map(|m| m.id.clone())
        });
        let Some(media_id) = media_id else {
            self.message = t("dubbing.applyFailed");
            return;
        };
        let mut commands = vec![Command::AddClipAtFirstFree {
            media_id,
            start: self.dub.rows[0].start,
        }];
        if (self.dub.mode == 2 || self.dub.mode == 3) && self.dub.mute_original {
            if let Some(id) = &self.dub.source_clip {
                commands.push(Command::UpdateClip {
                    clip_id: id.clone(),
                    patch: ClipPatch {
                        muted: Some(true),
                        ..ClipPatch::default()
                    },
                });
            }
        }
        studio.close_source();
        studio.apply(Command::Batch { commands });
        self.dub.reviewed = false;
        self.open = false;
        studio.notify(&t("speech.voiceAddedTimeline"), false);
    }
    fn dub_voices(&self) -> Vec<concat_speech::voice_catalog::Voice> {
        let catalog = if self.dub.provider == 1 {
            concat_speech::voice_catalog::gemini()
        } else {
            self.dub.catalog.clone()
        };
        catalog
            .into_iter()
            .filter(|v| {
                self.dub.provider != 2
                    || self.dub.options.accent == 0
                    || v.region == self.dub.options.accent
            })
            .filter(|v| {
                v.matches(
                    self.dub.model(),
                    self.dub.gender,
                    self.dub.provider == 2 && !self.dub.show_paid,
                )
            })
            .collect()
    }
    pub fn dubbing_data(&self, studio: &Studio) -> DubbingData {
        let sources = self
            .dub_sources(studio)
            .into_iter()
            .map(|(_, name)| name.into())
            .collect::<Vec<SharedString>>();
        let whisper = installed(&studio.settings.transcribers)
            .iter()
            .map(|m| SharedString::from(m.name.as_str()))
            .collect::<Vec<_>>();
        let rows = self
            .dub
            .rows
            .iter()
            .map(|row| DubbingRowData {
                text: row.text.as_str().into(),
                start: row.start as f32,
                end: row.end as f32,
                generated: row.audio.is_some(),
            })
            .collect::<Vec<_>>();
        let pending = dubbing::missing_audio(&self.dub.rows);
        DubbingData {
            expressions: self.dub.provider == 1
                && matches!(
                    self.dub.model(),
                    "gemini-3.8-flash-tts" | "gemini-3.8-flash-lite-tts"
                ),
            pending_audio: if pending.is_empty() {
                String::new()
            } else {
                tf(
                    "dubbing.missingAudio",
                    &[&pending
                        .iter()
                        .map(usize::to_string)
                        .collect::<Vec<_>>()
                        .join(", ")],
                )
            }
            .into(),
            voice_choice: self
                .dub_voices()
                .iter()
                .position(|v| v.id == self.dub.voice())
                .map(|i| i as i32 + 1)
                .unwrap_or(0),
            voice_names: slint::ModelRc::from(std::rc::Rc::new(slint::VecModel::from(
                std::iter::once(SharedString::from(t("dubbing.customVoice")))
                    .chain(
                        self.dub_voices()
                            .iter()
                            .map(|v| SharedString::from(v.name.as_str())),
                    )
                    .collect::<Vec<_>>(),
            ))),
            voice_gender: self.dub.gender,
            show_paid: self.dub.show_paid,
            catalog_loading: self.dub.catalog_loading,
            accent: self.dub.options.accent,
            delivery_preset: self.dub.options.delivery_preset,
            mode: self.dub.mode,
            provider: self.dub.provider,
            temperature: self.dub.options.temperature,
            model_choice: {
                let is_sts = self.dub.mode == 3 && self.dub.vc_method == 1;
                let list = if is_sts {
                    cloud_tts::sts_models(self.dub.provider)
                } else {
                    cloud_tts::models(self.dub.provider)
                };
                list.iter()
                    .position(|model| *model == self.dub.model())
                    .unwrap_or(list.len())
                    as i32
            },
            extended_settings: cloud_tts::has_extended_settings(self.dub.model()),
            stability: self.dub.options.stability,
            similarity: self.dub.options.similarity,
            style: self.dub.options.style,
            speed: self.dub.options.speed,
            speaker_boost: self.dub.options.speaker_boost,
            delivery: self.dub.options.delivery.as_str().into(),
            cloud_voice: self.dub.voice().into(),
            cloud_model: self.dub.model().into(),
            source: self.dub.source as i32,
            sources: slint::ModelRc::from(std::rc::Rc::new(slint::VecModel::from(sources))),
            whisper: self.dub.whisper as i32,
            whispers: slint::ModelRc::from(std::rc::Rc::new(slint::VecModel::from(whisper))),
            rows: slint::ModelRc::from(std::rc::Rc::new(slint::VecModel::from(rows))),
            has_preview: self.dub.preview.is_some(),
            can_apply: self.dub.reviewed && self.dub.preview.is_some(),
            mute_original: self.dub.mute_original,
            recording: self.dub.recording,
            vc_method: self.dub.vc_method,
            vc_preset: self.dub.vc_preset,
            vc_pitch: self.dub.vc_pitch,
            vc_speed: if self.dub.vc_speed <= 0.0 {
                1.0
            } else {
                self.dub.vc_speed
            },
            vc_denoise: if self.dub.vc_speed <= 0.0 {
                true
            } else {
                self.dub.vc_denoise
            },
            message: self.message.clone().into(),
        }
    }
    fn dub_transform_voice(&mut self, studio: &mut Studio) {
        if self.running {
            return;
        }
        let Some((id, label)) = self.dub_sources(studio).get(self.dub.source).cloned() else {
            self.message = t("dubbing.chooseSource");
            return;
        };
        let (source_path, audio_stream, source_start, window, source_duration, clip_start) =
            if id == "__standalone_media__" {
                let Some((ref path, _, duration)) = self.dub.standalone_media else {
                    self.message = t("dubbing.chooseSource");
                    return;
                };
                (path.clone(), None, 0.0, None, duration, 0.0)
            } else {
                let Some(clip) = studio.clip(&id).cloned() else {
                    return;
                };
                let Some(media) = studio.project().media_by_id(&clip.media_id) else {
                    return;
                };
                (
                    PathBuf::from(&media.path),
                    clip.audio_stream.map(|s| s as usize),
                    clip.source_start,
                    Some((clip.duration * clip.speed).max(0.1)),
                    clip.duration.max(0.1),
                    clip.start,
                )
            };
        let vc_method = self.dub.vc_method;
        if vc_method == 1 {
            if self.dub.provider == 0 {
                self.message =
                    "Phương án 2 (AI Speech-to-Speech) cần chọn ElevenLabs hoặc Gemini 3.8 ở mục Nguồn giọng đọc."
                        .to_owned();
                return;
            }
            if self.dub.key(studio).trim().is_empty() {
                self.message = t("dubbing.keyRequired");
                return;
            }
            if self.dub.provider == 2
                && !self.dub.show_paid
                && !self
                    .dub
                    .catalog
                    .iter()
                    .any(|v| v.id == self.dub.voice() && v.free)
            {
                self.message = t("dubbing.selectFreeVoice");
                return;
            }
        }
        let preset = self.dub.vc_preset;
        let pitch = self.dub.vc_pitch;
        let speed = if self.dub.vc_speed <= 0.0 {
            1.0
        } else {
            self.dub.vc_speed
        };
        let denoise = if self.dub.vc_speed <= 0.0 {
            true
        } else {
            self.dub.vc_denoise
        };
        let provider = self.dub.provider;
        let key = self.dub.key(studio).to_owned();
        let model = self.dub.model().to_owned();
        let voice = self.dub.voice().to_owned();
        let options = self.dub.options.clone();
        let project = studio
            .standalone_project_dir()
            .to_string_lossy()
            .into_owned();
        self.dub.signature = self.dub_signature(studio);
        self.dub.source_clip = (id != "__standalone_media__").then_some(id);
        self.dub.invalidate();
        let (generation, cancel) = self.dub_start(studio);
        spawn_in_project(
            move || {
                let out_path = if vc_method == 0 {
                    cloud_tts::transform_voice_dsp(
                        &source_path,
                        audio_stream,
                        source_start,
                        window,
                        preset,
                        pitch,
                        speed,
                        denoise,
                        &project,
                        &cancel,
                    )?
                } else {
                    cloud_tts::synthesize_speech_to_speech(
                        provider,
                        &key,
                        &model,
                        &voice,
                        &source_path,
                        audio_stream,
                        source_start,
                        window,
                        denoise,
                        &project,
                        &cancel,
                        &options,
                    )?
                };
                let summary = media::probe(&out_path.to_string_lossy())?;
                let duration = summary.duration.unwrap_or(source_duration).max(0.1);
                let rows = vec![Cue {
                    text: label,
                    start: clip_start,
                    end: clip_start + duration,
                    audio: Some(out_path.clone()),
                    audio_start: 0.0,
                    audio_end: duration,
                }];
                Ok(DubResult {
                    rows,
                    preview: Some(out_path),
                    listen: true,
                    review_all: true,
                    notice: None,
                })
            },
            move |studio, _, _, result| {
                studio.handle(Msg::Speech(SpeechMsg::DubFinished(
                    generation,
                    Box::new(result),
                )))
            },
        );
    }
}
