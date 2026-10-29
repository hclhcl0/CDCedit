//! Standalone Gemini 3.8 apps: dialogue, performance and custom voices.
use crate::{
    host::spawn_in_project,
    panes::Msg,
    studio::Studio,
    ui::{GeminiStudioData, GeminiTurnData, GeminiVoiceData},
};
use concat_speech::{
    api_keys, cloud_tts,
    gemini_studio::{self as service, Filters, Turn, Voice},
};
use std::{
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};
#[derive(Debug)]
pub enum GeminiMsg {
    Action(i32, i32, String, f32),
    Finished(u64, Result<Outcome, String>),
    Picked(u64, usize, PathBuf),
    Save(PathBuf, PathBuf),
}
#[derive(Debug)]
pub enum Outcome {
    Timed(service::TimedResult),
    Sentence(usize, concat_speech::dubbing::Cue),
    Audio(PathBuf),
    Library(Vec<Voice>),
    Created(Voice, Option<PathBuf>),
}
pub struct GeminiStudio {
    pub open: bool,
    mode: usize,
    model: usize,
    temperature: f32,
    text: String,
    style: String,
    turns: Vec<Turn>,
    timed: Option<Vec<(f64, f64)>>,
    timed_cache: Vec<concat_speech::dubbing::Cue>,
    voices: Vec<Voice>,
    a: usize,
    b: usize,
    library: Vec<Voice>,
    filters: Filters,
    key_index: usize,
    name: String,
    description: String,
    language: String,
    gender: usize,
    reference: Option<PathBuf>,
    consent: Option<PathBuf>,
    rights: bool,
    busy: bool,
    generation: u64,
    cancel: Arc<AtomicBool>,
    message: String,
    preview: Option<PathBuf>,
    created: Option<Voice>,
}
impl Default for GeminiStudio {
    fn default() -> Self {
        let voices: Vec<_> = concat_speech::voice_catalog::gemini()
            .iter()
            .map(|v| service::builtin(&v.id))
            .collect();
        let a = voices.iter().position(|v| v.id == "Charon").unwrap_or(0);
        let b = voices.iter().position(|v| v.id == "Kore").unwrap_or(0);
        Self {
            open: false,
            mode: 0,
            model: 0,
            temperature: 1.0,
            text: "Xin chào! Đây là giọng đọc thử của CDCedit.".into(),
            style: String::new(),
            turns: vec![
                Turn {
                    speaker: 0,
                    text: "Xin chào, hôm nay bạn thế nào?".into(),
                    style: "warm and friendly".into(),
                },
                Turn {
                    speaker: 1,
                    text: "Mình rất vui được gặp bạn!".into(),
                    style: "cheerful".into(),
                },
            ],
            timed: None,
            timed_cache: Vec::new(),
            voices,
            a,
            b,
            library: Vec::new(),
            filters: Filters::default(),
            key_index: 0,
            name: String::new(),
            description: String::new(),
            language: "vi-VN".into(),
            gender: 0,
            reference: None,
            consent: None,
            rights: false,
            busy: false,
            generation: 0,
            cancel: Arc::default(),
            message: String::new(),
            preview: None,
            created: None,
        }
    }
}
impl Drop for GeminiStudio {
    fn drop(&mut self) {
        self.cancel.store(true, Ordering::Relaxed);
    }
}
impl GeminiStudio {
    pub fn load_timed(&mut self, turns: Vec<Turn>, spans: Vec<(f64, f64)>) -> Result<(), String> {
        if self.busy {
            return Err("Hãy chờ tác vụ Gemini đang chạy kết thúc.".into());
        }
        if turns.is_empty() || turns.len() > 100 || turns.len() != spans.len() {
            return Err(
                "Lồng tiếng A/B hỗ trợ 1–100 câu/lượt; hãy chia phụ đề dài thành các đoạn nhỏ."
                    .into(),
            );
        }
        self.turns = turns;
        self.timed = Some(spans);
        self.timed_cache.clear();
        self.preview = None;
        self.mode = 0;
        self.open = true;
        self.message="Đã nạp bản dịch và mốc SRT. Chọn giọng A/B, kiểm tra từng câu rồi tạo toàn bộ. WAV giữ cả khoảng lặng từ mốc 0.".into();
        Ok(())
    }
    fn keys(&self, studio: &Studio) -> Result<String, String> {
        let raw = studio.prefs.gemini_api_key.expose();
        if self.key_index == 0 {
            return Ok(raw.into());
        }
        api_keys::parse(raw)
            .get(self.key_index - 1)
            .map(|k| (*k).to_owned())
            .ok_or("Key đã chọn không còn trong Cài đặt.".into())
    }
    fn start(&mut self) -> (u64, Arc<AtomicBool>) {
        self.generation = self.generation.wrapping_add(1);
        self.cancel = Arc::default();
        self.busy = true;
        self.message = "Đang xử lý với Gemini…".into();
        (self.generation, self.cancel.clone())
    }
    fn add_voice(&mut self, voice: Voice) -> usize {
        // Same ID in distinct Google projects may require different credentials.
        if let Some(i) = self
            .voices
            .iter()
            .position(|v| v.id == voice.id && v.owner.expose() == voice.owner.expose())
        {
            self.voices[i] = voice;
            i
        } else {
            self.voices.push(voice);
            self.voices.len() - 1
        }
    }
    pub fn update(&mut self, msg: GeminiMsg, studio: &mut Studio) {
        match msg {
            GeminiMsg::Finished(g, result) if g == self.generation => {
                self.busy = false;
                if self.cancel.load(Ordering::Relaxed)
                    && !matches!(
                        &result,
                        Ok(Outcome::Created(..) | Outcome::Timed(..) | Outcome::Sentence(..))
                    )
                {
                    self.message="Đã dừng. Nếu yêu cầu tạo giọng đã được Google nhận, hãy tải thư viện để kiểm tra trước khi tạo lại.".into();
                    return;
                }
                match result {
                    Ok(Outcome::Sentence(i, cue)) => {
                        self.preview = cue.audio.clone();
                        self.timed_cache
                            .resize_with(self.turns.len(), Default::default);
                        if let Some(row) = self.timed_cache.get_mut(i) {
                            *row = cue;
                        }
                        self.message="Đã giữ giọng câu vừa tạo, chưa co giãn theo mốc; tạo toàn bộ để ghép WAV.".into();
                    }
                    Ok(Outcome::Timed(result)) => {
                        self.timed_cache = result.cues;
                        self.preview = result.preview;
                        let count = self
                            .timed_cache
                            .iter()
                            .filter(|c| c.audio.is_some())
                            .count();
                        self.message = result
                            .notice
                            .map(|e| {
                                format!("Đã giữ {count} câu có âm thanh. Chưa ghép được WAV: {e}")
                            })
                            .unwrap_or_else(|| {
                                "Đã ghép WAV theo mốc SRT. Nghe thử trước khi lưu.".into()
                            });
                    }
                    Ok(Outcome::Audio(path)) => {
                        self.preview = Some(path);
                        self.message = "Đã tạo âm thanh. Bấm Nghe thử trước khi lưu.".into();
                    }
                    Ok(Outcome::Library(voices)) => {
                        self.message =
                            format!("Đã tải {} giọng của dự án/key đang dùng.", voices.len());
                        self.library = voices;
                    }
                    Ok(Outcome::Created(voice, preview)) => {
                        self.timed_cache.clear();
                        let id = voice.id.clone();
                        let index = self.add_voice(voice.clone());
                        self.a = index;
                        self.created = Some(voice);
                        self.preview = preview;
                        self.message = format!(
                            "Đã tạo giọng {id}. Đã chọn cho nhân vật A. Có thể nghe mẫu hoặc tạo lời đọc thử."
                        );
                    }
                    Err(e) => self.message = e,
                }
            }
            GeminiMsg::Picked(g, slot, path) if g == self.generation => {
                if slot == 0 {
                    self.reference = Some(path)
                } else {
                    self.consent = Some(path)
                };
                self.rights = false;
            }
            GeminiMsg::Save(source, destination) => match std::fs::copy(source, &destination) {
                Ok(_) => {
                    self.message = format!("Đã lưu {}", destination.display());
                }
                Err(e) => self.message = format!("Không lưu được WAV: {e}"),
            },
            GeminiMsg::Action(action, index, text, value) => {
                if action == 0 {
                    self.open = true;
                    if !self.busy {
                        self.mode = index.clamp(0, 4) as usize;
                    }
                    return;
                }
                if action == 1 {
                    self.open = false;
                    return;
                }
                if action == 2 {
                    self.cancel.store(true, Ordering::Relaxed);
                    self.message =
                        "Đang dừng; chờ yêu cầu đang gửi kết thúc để tránh tạo trùng giọng.".into();
                    return;
                }
                if self.busy {
                    return;
                }
                if matches!(action, 4 | 5 | 8 | 9 | 46 | 47) {
                    self.timed_cache.clear();
                }
                match action {
                    3 => self.mode = index.clamp(0, 4) as usize,
                    4 => {
                        self.model = index.clamp(0, 1) as usize;
                        self.preview = None;
                    }
                    5 if value.is_finite() => {
                        self.temperature = value.clamp(0.0, 2.0);
                        self.preview = None;
                    }
                    6 => {
                        self.text = text;
                        self.preview = None;
                    }
                    7 => {
                        self.style = text;
                        self.preview = None;
                    }
                    8 | 9 => {
                        if index >= 0 && (index as usize) < self.voices.len() {
                            if action == 8 {
                                self.a = index as usize
                            } else {
                                self.b = index as usize
                            };
                            self.preview = None;
                        }
                    }
                    10 => {
                        self.key_index = index.max(0) as usize;
                    }
                    20 => {
                        if self.timed.is_some() {
                            self.message = "Bỏ mốc SRT trước khi thêm lượt thoại mới.".into();
                            return;
                        }
                        if self.turns.len() < 100 {
                            self.turns.push(Turn {
                                speaker: self.turns.len() % 2,
                                ..Default::default()
                            });
                            self.preview = None;
                        }
                    }
                    21 => {
                        if index >= 0 && (index as usize) < self.turns.len() {
                            self.turns.remove(index as usize);
                            if let Some(spans) = &mut self.timed {
                                spans.remove(index as usize);
                            }
                            if (index as usize) < self.timed_cache.len() {
                                self.timed_cache.remove(index as usize);
                            }
                            self.preview = None;
                        }
                    }
                    22 | 23 | 24 => {
                        if let Some(turn) = self.turns.get_mut(index.max(0) as usize) {
                            match action {
                                22 => turn.speaker = if text == "B" { 1 } else { 0 },
                                23 => turn.text = text,
                                _ => turn.style = text,
                            };
                            if let Some(cue) = self.timed_cache.get_mut(index.max(0) as usize) {
                                cue.audio = None;
                            }
                            self.preview = None;
                        }
                    }
                    25 => {
                        if let Some(turn) = self.turns.get_mut(index.max(0) as usize) {
                            turn.text.push_str(" |ừ, đúng vậy|");
                            self.preview = None;
                        }
                    }
                    26 => {
                        self.timed = None;
                        self.timed_cache.clear();
                        self.preview = None;
                    }
                    27 | 28 if value.is_finite() => {
                        if let Some(span) = self
                            .timed
                            .as_mut()
                            .and_then(|s| s.get_mut(index.max(0) as usize))
                        {
                            if action == 27 {
                                span.0 = value.max(0.0) as f64;
                            } else {
                                span.1 = value.max(0.0) as f64;
                            }
                            self.preview = None;
                        }
                    }
                    29 => {
                        if let Some(path) = self
                            .timed_cache
                            .get(index.max(0) as usize)
                            .and_then(|c| c.audio.clone())
                        {
                            self.preview = Some(path.clone());
                            studio.preview_file(
                                &format!("gemini-sentence:{}", path.display()),
                                "Giọng câu đã tạo",
                                &path,
                            );
                            self.message =
                                "Đây là giọng gốc của một câu, chưa co giãn theo mốc.".into();
                        }
                    }
                    30 | 31 => self.generate(
                        studio,
                        if action == 31 {
                            Some(index.max(0) as usize)
                        } else {
                            None
                        },
                    ),
                    32 => {
                        if let Some(path) = &self.preview {
                            studio.preview_file(
                                &format!("gemini-studio:{}", path.display()),
                                "Gemini TTS 3.8",
                                path,
                            );
                        }
                    }
                    34 => studio.close_source(),
                    33 => {
                        if let Some(source) = self.preview.clone() {
                            let folder = PathBuf::from(crate::studio::home_folder("Documents"));
                            crate::host::after_dialog(
                                move || {
                                    crate::platform::save_file(
                                        "Lưu giọng Gemini (.WAV)",
                                        &folder,
                                        "gemini-tts.wav",
                                        ("WAV Audio", &["wav"]),
                                    )
                                    .map(|p| p.with_extension("wav"))
                                },
                                move |studio, _, _, path| {
                                    studio.handle(Msg::Gemini(GeminiMsg::Save(source, path)))
                                },
                            );
                        }
                    }
                    40 => self.load_library(studio),
                    41 => self.filters.language = text,
                    42 => {
                        self.filters.gender =
                            ["", "female", "male", "neutral"][index.clamp(0, 3) as usize].into()
                    }
                    43 => self.filters.accent = text,
                    44 => {
                        self.filters.pitch =
                            ["", "low", "medium", "high"][index.clamp(0, 3) as usize].into()
                    }
                    45 => self.filters.search = text,
                    46 => {
                        if let Some(voice) = self.library.get(index.max(0) as usize).cloned() {
                            let i = self.add_voice(voice);
                            if text == "B" {
                                self.b = i
                            } else {
                                self.a = i
                            };
                            self.preview = None;
                            self.message =
                                "Đã gán giọng. Chọn Hội thoại hoặc Diễn cảm để tạo lời đọc.".into();
                        }
                    }
                    47 => {
                        if let Some(voice) = self.library.get(index.max(0) as usize).cloned() {
                            self.a = self.add_voice(voice);
                            self.generate_sample(studio);
                        }
                    }
                    50 => self.name = text,
                    51 => self.description = text,
                    52 => self.gender = index.clamp(0, 3) as usize,
                    53 => self.language = text,
                    54 | 55 => {
                        let slot = if action == 54 { 0 } else { 1 };
                        let generation = self.generation;
                        crate::host::pick_files_then(
                            if slot == 0 {
                                "Chọn mẫu giọng 10–30 giây"
                            } else {
                                "Chọn bản ghi đồng ý của người nói"
                            }
                            .into(),
                            Some((
                                "Audio / Video".into(),
                                &["wav", "mp3", "m4a", "aac", "ogg", "flac", "mp4"],
                            )),
                            move |studio, _, _, paths| {
                                if let Some(path) = paths.into_iter().next() {
                                    studio.handle(Msg::Gemini(GeminiMsg::Picked(
                                        generation, slot, path,
                                    )));
                                }
                            },
                        );
                    }
                    57 => self.rights = index != 0,
                    58 => self.create_voice(studio),
                    59 => self.generate_sample(studio),
                    _ => {}
                }
            }
            _ => {}
        }
    }
    fn load_library(&mut self, studio: &mut Studio) {
        let keys = match self.keys(studio) {
            Ok(k) => k,
            Err(e) => {
                self.message = e;
                return;
            }
        };
        let filters = self.filters.clone();
        let (g, cancel) = self.start();
        spawn_in_project(
            move || service::list(&keys, &filters, &cancel).map(Outcome::Library),
            move |studio, _, _, result| studio.handle(Msg::Gemini(GeminiMsg::Finished(g, result))),
        );
    }
    fn generate_sample(&mut self, studio: &mut Studio) {
        let old_mode = self.mode;
        self.mode = 1;
        self.generate(studio, None);
        self.mode = old_mode;
    }
    fn generate(&mut self, studio: &mut Studio, row: Option<usize>) {
        let keys = match self.keys(studio) {
            Ok(k) => k,
            Err(e) => {
                self.message = e;
                return;
            }
        };
        let voices = [self.voices[self.a].clone(), self.voices[self.b].clone()];
        let model = service::model(self.model);
        let temperature = self.temperature;
        let project = studio
            .standalone_project_dir()
            .to_string_lossy()
            .into_owned();
        let text = self.text.clone();
        let style = self.style.clone();
        let turns = if let Some(row) = row {
            match self.turns.get(row) {
                Some(turn) => vec![turn.clone()],
                None => return,
            }
        } else {
            self.turns.clone()
        };
        let single = self.mode == 1 || row.is_some();
        let configured = api_keys::parse(studio.prefs.gemini_api_key.expose());
        let used: Vec<usize> = if single {
            vec![if row.is_some() { turns[0].speaker } else { 0 }]
        } else {
            turns.iter().map(|turn| turn.speaker).collect()
        };
        if used.iter().any(|i| {
            !voices[*i].owner.expose().is_empty()
                && !configured.contains(&voices[*i].owner.expose())
        }) {
            self.message="Key của giọng đã chọn không còn trong Cài đặt. Hãy thêm lại key hoặc tải thư viện và chọn giọng khác.".into();
            return;
        }
        if single && row.is_none() && (text.trim().is_empty() || text.chars().count() > 20_000) {
            self.message = "Nhập lời đọc từ 1 đến 20.000 ký tự.".into();
            return;
        }
        let spans = self.timed.clone();
        let cached = self.timed_cache.clone();
        let (g, cancel) = self.start();
        spawn_in_project(
            move || {
                if !single {
                    if let Some(spans) = spans.as_ref() {
                        return Ok(Outcome::Timed(service::timed_dialogue(
                            &keys,
                            model,
                            [&voices[0], &voices[1]],
                            &turns,
                            &spans,
                            temperature,
                            &project,
                            &cancel,
                            &cached,
                        )));
                    }
                }
                let path = if single {
                    let (voice, text, style) = if row.is_some() {
                        (&voices[turns[0].speaker], &turns[0].text, &turns[0].style)
                    } else {
                        (&voices[0], &text, &style)
                    };
                    let key = if voice.owner.expose().is_empty() {
                        keys.as_str()
                    } else {
                        voice.owner.expose()
                    };
                    let options = cloud_tts::VoiceOptions {
                        delivery: style.clone(),
                        temperature,
                        ..Default::default()
                    };
                    cloud_tts::synthesize(
                        1, key, model, &voice.id, text, &project, &cancel, &options,
                    )?
                } else {
                    service::dialogue(
                        &keys,
                        model,
                        [&voices[0], &voices[1]],
                        &turns,
                        temperature,
                        &project,
                        &cancel,
                    )?
                };
                if let (Some(i), Some(spans)) = (row, spans.as_ref()) {
                    let info = concat_media::probe(&path).map_err(|e| e.to_string())?;
                    return Ok(Outcome::Sentence(
                        i,
                        concat_speech::dubbing::Cue {
                            text: turns[0].text.clone(),
                            start: spans[i].0,
                            end: spans[i].1,
                            audio: Some(path),
                            audio_start: 0.0,
                            audio_end: info.duration.ok_or("Generated audio has no duration")?.as_f64(),
                        },
                    ));
                }
                Ok(Outcome::Audio(path))
            },
            move |studio, _, _, result| studio.handle(Msg::Gemini(GeminiMsg::Finished(g, result))),
        );
    }
    fn create_voice(&mut self, studio: &mut Studio) {
        if self.mode != 2 && self.mode != 3 {
            return;
        }
        if self.mode == 3 && (!self.rights || self.reference.is_none() || self.consent.is_none()) {
            self.message="Cần mẫu giọng, bản ghi đồng ý của cùng người trưởng thành và xác nhận quyền sử dụng.".into();
            return;
        }
        let keys = match self.keys(studio) {
            Ok(k) => k,
            Err(e) => {
                self.message = e;
                return;
            }
        };
        let model = service::model(self.model);
        let name = self.name.clone();
        let description = self.description.clone();
        let language = self.language.clone();
        let gender = ["", "female", "male", "neutral"][self.gender];
        let recordings = if self.mode == 3 {
            Some((
                self.reference.clone().unwrap(),
                self.consent.clone().unwrap(),
            ))
        } else {
            None
        };
        let project = studio
            .standalone_project_dir()
            .to_string_lossy()
            .into_owned();
        let (g, cancel) = self.start();
        spawn_in_project(
            move || {
                service::create(
                    &keys,
                    model,
                    &name,
                    &description,
                    &language,
                    gender,
                    recordings.as_ref().map(|(a, b)| (a.as_path(), b.as_path())),
                    &project,
                    &cancel,
                )
                .map(|(v, p)| Outcome::Created(v, p))
            },
            move |studio, _, _, result| studio.handle(Msg::Gemini(GeminiMsg::Finished(g, result))),
        );
    }
    pub fn data(&self, studio: &Studio) -> GeminiStudioData {
        fn strings(items: Vec<String>) -> slint::ModelRc<slint::SharedString> {
            slint::ModelRc::from(std::rc::Rc::new(slint::VecModel::from(
                items.into_iter().map(Into::into).collect::<Vec<_>>(),
            )))
        }
        let keys = api_keys::parse(studio.prefs.gemini_api_key.expose());
        let mut key_names = vec!["Tự xoay key".into()];
        key_names.extend((1..=keys.len()).map(|i| format!("Key {i} (dự án riêng)")));
        let voice_names = self
            .voices
            .iter()
            .map(|v| format!("{} · {}", v.name, v.id))
            .collect();
        let turns = self
            .turns
            .iter()
            .enumerate()
            .map(|(i, t)| GeminiTurnData {
                start: self
                    .timed
                    .as_ref()
                    .and_then(|s| s.get(i))
                    .map(|s| s.0 as f32)
                    .unwrap_or(0.0),
                end: self
                    .timed
                    .as_ref()
                    .and_then(|s| s.get(i))
                    .map(|s| s.1 as f32)
                    .unwrap_or(0.0),
                generated: self.timed_cache.get(i).is_some_and(|c| c.audio.is_some()),
                speaker: t.speaker as i32,
                text: t.text.as_str().into(),
                style: t.style.as_str().into(),
            })
            .collect::<Vec<_>>();
        let library = self
            .library
            .iter()
            .map(|v| GeminiVoiceData {
                name: format!("{} · {}", v.name, v.id).into(),
                detail: format!(
                    "{} · {} · {} · {} · {}\n{}",
                    v.kind, v.language, v.gender, v.accent, v.pitch, v.description
                )
                .into(),
            })
            .collect::<Vec<_>>();
        GeminiStudioData {
            open: self.open,
            mobile: cfg!(any(target_os = "android", target_os = "ios")),
            mode: self.mode as i32,
            model: self.model as i32,
            temperature: self.temperature,
            timed: self.mode == 0 && self.timed.is_some(),
            busy: self.busy,
            ready: !keys.is_empty(),
            message: self.message.as_str().into(),
            text: self.text.as_str().into(),
            style: self.style.as_str().into(),
            voices: strings(voice_names),
            a: self.a as i32,
            b: self.b as i32,
            turns: slint::ModelRc::from(std::rc::Rc::new(slint::VecModel::from(turns))),
            library: slint::ModelRc::from(std::rc::Rc::new(slint::VecModel::from(library))),
            key_names: strings(key_names),
            key_index: self.key_index as i32,
            filter_language: self.filters.language.as_str().into(),
            filter_accent: self.filters.accent.as_str().into(),
            filter_search: self.filters.search.as_str().into(),
            filter_gender: ["", "female", "male", "neutral"]
                .iter()
                .position(|s| *s == self.filters.gender)
                .unwrap_or(0) as i32,
            filter_pitch: ["", "low", "medium", "high"]
                .iter()
                .position(|s| *s == self.filters.pitch)
                .unwrap_or(0) as i32,
            name: self.name.as_str().into(),
            description: self.description.as_str().into(),
            language: self.language.as_str().into(),
            gender: self.gender as i32,
            reference: self
                .reference
                .as_ref()
                .map(|p| p.to_string_lossy().into_owned())
                .unwrap_or_default()
                .into(),
            consent: self
                .consent
                .as_ref()
                .map(|p| p.to_string_lossy().into_owned())
                .unwrap_or_default()
                .into(),
            rights: self.rights,
            has_preview: self.preview.is_some(),
            created_id: self
                .created
                .as_ref()
                .map(|v| v.id.clone())
                .unwrap_or_default()
                .into(),
        }
    }
}
