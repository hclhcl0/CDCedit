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
}
#[derive(Clone, Debug)]
pub struct DubResult {
    rows: Vec<Cue>,
    preview: Option<PathBuf>,
    listen: bool,
    review_all: bool,
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
        if self.provider == 1 {
            if self.gemini_model.is_empty() {
                "gemini-3.8-flash-tts"
            } else {
                &self.gemini_model
            }
        } else {
            if self.eleven_model.is_empty() {
                "eleven_v3"
            } else {
                &self.eleven_model
            }
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
    fn dub_sources(&self, studio: &Studio) -> Vec<(String, String)> {
        if self.dub.mode == 1 {
            studio
                .timeline()
                .tracks
                .iter()
                .filter_map(|track| {
                    let count = studio
                        .timeline()
                        .clips
                        .iter()
                        .filter(|c| c.kind == ClipKind::Text && c.track_id == track.id)
                        .count();
                    (count > 0).then(|| (track.id.clone(), format!("{} Â· {}", track.id, count)))
                })
                .collect()
        } else {
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
                .map(|c| (c.id.clone(), format!("{} Â· {:.2}s", c.name, c.start)))
                .collect()
        }
    }
    fn dub_signature(&self, studio: &Studio) -> String {
        let Some((id, _)) = self.dub_sources(studio).get(self.dub.source).cloned() else {
            return String::new();
        };
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
        if matches!(
            action,
            0 | 1 | 3 | 4 | 5 | 6 | 7 | 8 | 9 | 12 | 13 | 14 | 18..=25
        ) {
            studio.close_source();
        }
        match action {
            0 => {
                self.dub.mode = index.clamp(0, 2);
                self.dub.mute_original = self.dub.mode == 2;
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
                if let Some(model) = cloud_tts::models(self.dub.provider).get(index.max(0) as usize)
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
            _ => {}
        }
        if matches!(action, 3 | 4 | 18..=25) {
            for row in &mut self.dub.rows {
                row.audio = None;
            }
            self.dub.invalidate();
            self.dub.save_preferences(studio);
        }
    }
    fn dub_load_track(&mut self, studio: &Studio) {
        let Some((track, _)) = self.dub_sources(studio).get(self.dub.source).cloned() else {
            self.message = t("dubbing.chooseSource");
            return;
        };
        let mut rows: Vec<_> = studio
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
            .collect();
        rows.sort_by(|a, b| a.start.total_cmp(&b.start));
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
        let Some(model) = installed(&studio.settings.transcribers)
            .get(self.dub.whisper)
            .map(|m| m.id.clone())
        else {
            self.message = t("captions.downloadTranscriberModelFirst");
            return;
        };
        let request = TranscribeRequest {
            path: media.path.clone(),
            audio_stream: clip.audio_stream,
            source_start: clip.source_start,
            window: clip.duration * clip.speed,
            model_id: model,
        };
        self.dub.signature = self.dub_signature(studio);
        self.dub.source_clip = Some(id);
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
                        start: clip.start + s.start / clip.speed,
                        end: (clip.start + s.end / clip.speed).min(clip.start + clip.duration),
                        ..Cue::default()
                    })
                    .collect();
                Ok(DubResult {
                    rows,
                    preview: None,
                    listen: false,
                    review_all: false,
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
            .session
            .as_ref()
            .ok_or("Open a project first")?
            .path()
            .to_owned();
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
        let Some(project) = studio.session.as_ref().map(|s| s.path().to_owned()) else {
            return;
        };
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
        let options = self.dub.options.clone();
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
                    let words = transcriber.transcribe_words(&dirs, &request, report(epoch))?;
                    let lines = rows.iter().map(|r| r.text.clone()).collect::<Vec<_>>();
                    let mut spans = dubbing::align(&lines, &words)?;
                    if let Some(last) = spans.last_mut() {
                        last.1 = duration;
                    }
                    for (cue, (start, end)) in rows.iter_mut().zip(spans) {
                        cue.audio = Some(path.clone());
                        cue.audio_start = start;
                        cue.audio_end = end;
                    }
                } else {
                    let index = row.unwrap_or(0);
                    let cue = &mut rows[index];
                    cue.audio = Some(path);
                    cue.audio_start = 0.0;
                    cue.audio_end = duration;
                    if mode == 0 {
                        cue.end = cue.start + duration;
                    }
                }
                cloud_tts::check_cancel(&cancel)?;
                let preview = if rows.iter().all(|r| r.audio.is_some()) {
                    Some(dubbing::render(&rows, &project, &cancel)?)
                } else {
                    None
                };
                Ok(DubResult {
                    rows,
                    preview,
                    listen: false,
                    review_all: false,
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
                let card = format!("dub:{}", path.display());
                studio.preview_file(&card, &t("dubbing.preview"), &path);
                self.dub.reviewed = studio
                    .source
                    .as_ref()
                    .is_some_and(|source| source.card == card);
                return;
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
        let Some(project) = studio.session.as_ref().map(|s| s.path().to_owned()) else {
            return;
        };
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
            Err(error) => self.message = error,
        }
    }
    fn dub_apply(&mut self, studio: &mut Studio) {
        if self.running || !self.dub.reviewed {
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
        if self.dub.mode == 2 && self.dub.mute_original {
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
        DubbingData {
            mode: self.dub.mode,
            provider: self.dub.provider,
            model_choice: cloud_tts::models(self.dub.provider)
                .iter()
                .position(|model| *model == self.dub.model())
                .unwrap_or(cloud_tts::models(self.dub.provider).len())
                as i32,
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
        }
    }
}
