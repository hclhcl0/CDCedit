// SPDX-License-Identifier: AGPL-3.0-or-later
//! Gemini 3.8 standalone dialogue, voice library, design and replication.
use crate::{
    api_keys::{self, Provider},
    cloud_tts,
};
use base64::{Engine, engine::general_purpose::STANDARD};
use serde_json::{Value, json};
use std::{
    io::Read,
    path::{Path, PathBuf},
    sync::atomic::AtomicBool,
    time::Duration,
};
const BASE: &str = "https://generativelanguage.googleapis.com/v1beta";

#[derive(Clone)]
/// An in-memory project credential, never serialized or displayed.
pub struct Credential(String);
impl std::fmt::Debug for Credential {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Credential([redacted])")
    }
}
impl Credential {
    pub fn expose(&self) -> &str {
        &self.0
    }
}
#[derive(Clone, Debug)]
pub struct Voice {
    pub id: String,
    pub name: String,
    pub description: String,
    pub kind: String,
    pub language: String,
    pub gender: String,
    pub accent: String,
    pub pitch: String,
    pub owner: Credential,
}
#[derive(Clone, Debug, Default)]
pub struct Turn {
    pub speaker: usize,
    pub text: String,
    pub style: String,
}
#[derive(Clone, Debug, Default)]
pub struct Filters {
    pub language: String,
    pub gender: String,
    pub accent: String,
    pub pitch: String,
    pub search: String,
}
pub fn model(index: usize) -> &'static str {
    if index == 1 {
        "gemini-3.8-flash-lite-tts"
    } else {
        "gemini-3.8-flash-tts"
    }
}
fn valid_voice(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 1024
        && id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
}
fn request(
    key: &str,
    path: &str,
    body: Option<&Value>,
    query: &[(&str, String)],
    cancel: &AtomicBool,
) -> Result<Value, String> {
    cloud_tts::check_cancel(cancel)?;
    let agent = ureq::AgentBuilder::new()
        .timeout_connect(Duration::from_secs(15))
        .timeout_read(Duration::from_secs(120))
        .timeout_write(Duration::from_secs(45))
        .build();
    let url = format!("{BASE}/{path}");
    let mut req = if body.is_some() {
        agent.post(&url)
    } else {
        agent.get(&url)
    };
    req = req
        .set("x-goog-api-key", key)
        .set("Content-Type", "application/json");
    for (name, value) in query {
        req = req.query(name, value);
    }
    let response = if let Some(body) = body {
        req.send_bytes(&serde_json::to_vec(body).map_err(|_| "Could not encode Gemini request")?)
    } else {
        req.call()
    }
    .map_err(|e| api_keys::http_error("Gemini Studio", e))?;
    let mut bytes = Vec::new();
    response
        .into_reader()
        .take(100_000_001)
        .read_to_end(&mut bytes)
        .map_err(|_| "Could not read Gemini response")?;
    if bytes.len() > 100_000_000 {
        return Err("Gemini response is too large".into());
    }
    serde_json::from_slice(&bytes).map_err(|_| "Invalid Gemini response".into())
}
fn string(value: &Value, key: &str) -> String {
    value[key].as_str().unwrap_or("").to_owned()
}
fn voice(value: &Value, key: &str) -> Result<Voice, String> {
    let id = string(value, "id");
    if !valid_voice(&id) {
        return Err("Gemini returned an invalid voice ID".into());
    }
    let mut name = string(value, "display_name");
    if name.is_empty() {
        name = id.clone();
    }
    Ok(Voice {
        id,
        name,
        description: string(value, "description"),
        kind: string(value, "type"),
        language: string(value, "language_code"),
        gender: string(value, "gender"),
        accent: string(value, "accent"),
        pitch: string(value, "pitch"),
        owner: Credential(key.into()),
    })
}
pub fn list(keys: &str, filters: &Filters, cancel: &AtomicBool) -> Result<Vec<Voice>, String> {
    api_keys::with_keys(Provider::Gemini, keys, cancel, |key| {
        let mut result = Vec::new();
        let mut token = String::new();
        for _ in 0..20 {
            let mut query = vec![("page_size", "100".into())];
            for (name, value) in [
                ("language_code", &filters.language),
                ("gender", &filters.gender),
                ("accent", &filters.accent),
                ("pitch", &filters.pitch),
                ("search", &filters.search),
            ] {
                if !value.trim().is_empty() {
                    query.push((name, value.trim().into()));
                }
            }
            if !token.is_empty() {
                query.push(("page_token", token.clone()));
            }
            let value = request(key, "voices", None, &query, cancel)?;
            let entries = value["voices"]
                .as_array()
                .ok_or("Gemini returned no voice list")?;
            for entry in entries {
                result.push(voice(entry, key)?);
            }
            let next = string(&value, "next_page_token");
            if next.is_empty() {
                return Ok(result);
            }
            if next == token {
                return Err("Repeated voice library page token".into());
            }
            token = next;
        }
        Err("Voice library exceeded 20 pages; narrow the filters".into())
    })
}
/// Build a single multi-speaker request only for prebuilt voices.
pub fn dialogue_body(
    model: &str,
    voices: [&str; 2],
    turns: &[Turn],
    temperature: f32,
) -> Result<Value, String> {
    if ![self::model(0), self::model(1)].contains(&model) || voices.iter().any(|v| !valid_voice(v))
    {
        return Err("Choose Gemini 3.8 and valid voices for A/B".into());
    }
    if !temperature.is_finite() || !(0.0..=2.0).contains(&temperature) {
        return Err("Temperature must be 0–2".into());
    }
    if turns.is_empty()
        || turns.len() > 100
        || turns
            .iter()
            .any(|t| t.speaker > 1 || t.text.trim().is_empty())
        || turns.iter().map(|t| t.text.chars().count()).sum::<usize>() > 20_000
    {
        return Err("Use 1–100 nonempty turns, up to 20,000 characters".into());
    }
    let content: Vec<_> = turns.iter().map(|turn| json!({"type":"text","text":turn.text,
        "annotations":[{"type":"speech_metadata","speaker":if turn.speaker==0 {"A"} else {"B"}, "style":turn.style}]})).collect();
    Ok(
        json!({"model":model,"input":[{"type":"user_input","content":content}],
        "response_format":{"type":"audio"},"generation_config":{"temperature":temperature,
        "speech_config":{"speakers":[{"speaker":"A","voice":voices[0]},{"speaker":"B","voice":voices[1]}]}}}),
    )
}
fn save_audio(value: &Value, project: &str, label: &str) -> Result<PathBuf, String> {
    let bytes = cloud_tts::gemini_audio(value)?;
    let path = cloud_tts::output(project, label)?;
    std::fs::write(&path, bytes).map_err(|e| e.to_string())?;
    Ok(path)
}
pub fn dialogue(
    keys: &str,
    model: &str,
    voices: [&Voice; 2],
    turns: &[Turn],
    temperature: f32,
    project: &str,
    cancel: &AtomicBool,
) -> Result<PathBuf, String> {
    let body = dialogue_body(model, [&voices[0].id, &voices[1].id], turns, temperature)?;
    if voices.iter().all(|voice| voice.kind == "prebuilt") {
        return api_keys::with_keys(Provider::Gemini, keys, cancel, |key| {
            save_audio(
                &request(key, "interactions", Some(&body), &[], cancel)?,
                project,
                "gemini-dialogue",
            )
        });
    }
    // Custom voices belong to a specific project: synthesize turns using their owner's key.
    let mut joined = Vec::new();
    for turn in turns {
        cloud_tts::check_cancel(cancel)?;
        let voice = voices[turn.speaker];
        let options = cloud_tts::VoiceOptions {
            delivery: turn.style.clone(),
            temperature,
            ..Default::default()
        };
        let key = if voice.owner.expose().is_empty() {
            keys
        } else {
            voice.owner.expose()
        };
        let path = cloud_tts::synthesize(
            1, key, model, &voice.id, &turn.text, project, cancel, &options,
        )?;
        let mut decoder = concat_media::AudioDecoder::open(
            &path,
            &concat_media::AudioOptions {
                rate: 24000,
                channels: 1,
                format: concat_media::SampleFormat::F32,
                ..Default::default()
            },
        )
        .map_err(|e| e.to_string())?;
        let samples = decoder.collect_f32().map_err(|e| e.to_string())?;
        if joined.len() + samples.len() > 24000 * 1800 {
            return Err("Split dialogue into sections under 30 minutes".into());
        }
        joined.extend(samples);
    }
    let path = cloud_tts::output(project, "gemini-dialogue")?;
    cloud_tts::write_wav(&path, &joined, 24000)?;
    Ok(path)
}
#[derive(Debug)]
pub struct TimedResult {
    pub cues: Vec<crate::dubbing::Cue>,
    pub preview: Option<PathBuf>,
    pub notice: Option<String>,
}
/// Generate per sentence and retain successful audio even if another sentence or fitting fails.
pub fn timed_dialogue(
    keys: &str,
    model: &str,
    voices: [&Voice; 2],
    turns: &[Turn],
    spans: &[(f64, f64)],
    temperature: f32,
    project: &str,
    cancel: &AtomicBool,
    cached: &[crate::dubbing::Cue],
) -> TimedResult {
    let mut cues = turns
        .iter()
        .enumerate()
        .map(|(i, t)| {
            let (start, end) = spans.get(i).copied().unwrap_or((0.0, 0.0));
            let previous = cached
                .get(i)
                .filter(|c| c.text == t.text)
                .cloned()
                .unwrap_or_default();
            crate::dubbing::Cue {
                text: t.text.clone(),
                start,
                end,
                ..previous
            }
        })
        .collect::<Vec<_>>();
    let result = (|| -> Result<PathBuf, String> {
        if turns.len() != spans.len() || turns.len() > 100 || turns.iter().any(|t| t.speaker > 1) {
            return Err("Lồng tiếng A/B theo mốc hỗ trợ tối đa 100 câu/lượt. Hãy chia phụ đề dài thành các phần.".into());
        }
        crate::dubbing::validate(&cues)?;
        if cues.last().is_some_and(|c| c.end > 1800.0) {
            return Err("WAV theo mốc hỗ trợ mốc 0–30 phút. Hãy chia đoạn và đưa mốc về đầu đoạn trước khi lồng tiếng A/B.".into());
        }
        for (i, turn) in turns.iter().enumerate() {
            cloud_tts::check_cancel(cancel)?;
            if cues[i].audio.as_ref().is_some_and(|p| p.is_file())
                && cues[i].audio_end.is_finite()
                && cues[i].audio_start.is_finite()
                && cues[i].audio_end > cues[i].audio_start
            {
                continue;
            }
            let voice = voices[turn.speaker];
            let key = if voice.owner.expose().is_empty() {
                keys
            } else {
                voice.owner.expose()
            };
            let options = cloud_tts::VoiceOptions {
                delivery: turn.style.clone(),
                temperature,
                ..Default::default()
            };
            let path = cloud_tts::synthesize(
                1, key, model, &voice.id, &turn.text, project, cancel, &options,
            )?;
            let info = concat_media::probe(&path).map_err(|e| e.to_string())?;
            let duration = info
                .duration
                .map(|d| d.as_f64())
                .filter(|d| d.is_finite() && *d > 0.0)
                .ok_or("Generated audio has no duration")?;
            cues[i].audio = Some(path);
            cues[i].audio_start = 0.0;
            cues[i].audio_end = duration;
        }
        let rendered = crate::dubbing::render(&cues, project, cancel)?;
        let lead = cues[0].start;
        if lead <= 0.0 {
            return Ok(rendered);
        }
        let mut decoder = concat_media::AudioDecoder::open(
            &rendered,
            &concat_media::AudioOptions {
                rate: 24000,
                channels: 1,
                format: concat_media::SampleFormat::F32,
                ..Default::default()
            },
        )
        .map_err(|e| e.to_string())?;
        let mut samples = vec![0.0; (lead * 24000.0).round() as usize];
        samples.extend(decoder.collect_f32().map_err(|e| e.to_string())?);
        cloud_tts::check_cancel(cancel)?;
        let output = cloud_tts::output(project, "gemini-timed-dialogue")?;
        cloud_tts::write_wav(&output, &samples, 24000)?;
        Ok(output)
    })();
    match result {
        Ok(path) => TimedResult {
            cues,
            preview: Some(path),
            notice: None,
        },
        Err(error) => TimedResult {
            cues,
            preview: None,
            notice: Some(error),
        },
    }
}
pub fn builtin(id: &str) -> Voice {
    Voice {
        id: id.into(),
        name: id.into(),
        kind: "prebuilt".into(),
        description: String::new(),
        language: String::new(),
        gender: String::new(),
        accent: String::new(),
        pitch: String::new(),
        owner: Credential(String::new()),
    }
}
fn audio_file(path: &Path, reference: bool) -> Result<Value, String> {
    let summary = concat_media::probe(path).map_err(|e| e.to_string())?;
    let duration = summary.duration.ok_or("Audio has no duration")?.as_f64();
    if !duration.is_finite()
        || duration <= 0.0
        || duration > 60.0
        || (reference && !(10.0..=30.0).contains(&duration))
    {
        return Err(
            "Reference must be 10–30 seconds; consent recording must be at most 60 seconds".into(),
        );
    }
    let mut decoder = concat_media::AudioDecoder::open(
        path,
        &concat_media::AudioOptions {
            duration: Some(60.0),
            rate: 24000,
            channels: 1,
            format: concat_media::SampleFormat::F32,
            ..Default::default()
        },
    )
    .map_err(|e| e.to_string())?;
    let samples = decoder.collect_f32().map_err(|e| e.to_string())?;
    let seconds = samples.len() as f64 / 24000.0;
    if samples.is_empty() || (reference && !(10.0..=30.1).contains(&seconds)) {
        return Err("Decoded reference audio must be 10–30 seconds".into());
    }
    let pcm: Vec<u8> = samples
        .iter()
        .flat_map(|s| ((s.clamp(-1.0, 1.0) * 32767.0) as i16).to_le_bytes())
        .collect();
    Ok(json!({"mime_type":"audio/wav","data":STANDARD.encode(cloud_tts::pcm_wav(&pcm,24000)?)}))
}
/// Create a server-stored voice; the UI must explicitly request creation.
pub fn create(
    keys: &str,
    model: &str,
    name: &str,
    description: &str,
    language: &str,
    gender: &str,
    recordings: Option<(&Path, &Path)>,
    project: &str,
    cancel: &AtomicBool,
) -> Result<(Voice, Option<PathBuf>), String> {
    if ![self::model(0), self::model(1)].contains(&model)
        || name.trim().is_empty()
        || name.chars().count() > 100
    {
        return Err("Enter a voice name (up to 100 characters) and select Gemini 3.8".into());
    }
    let mut profile = json!({"model":model,"display_name":name.trim()});
    if let Some((source, consent)) = recordings {
        profile["type"] = json!("replicated");
        profile["replicated"] = json!({"source_audio":audio_file(source,true)?,"consent_audio":audio_file(consent,false)?});
    } else {
        if description.trim().is_empty() || description.chars().count() > 4000 {
            return Err("Describe the voice in 1–4,000 characters".into());
        }
        profile["type"] = json!("prompted");
        profile["prompted"] = json!({"input":description.trim()});
        if !language.trim().is_empty() {
            profile["language_code"] = json!(language.trim());
        }
        if !gender.is_empty() {
            profile["gender"] = json!(gender);
        }
    }
    let body = json!({"store":true,"voice":profile});
    // A successful server-side creation is a committed resource: retain its ID even
    // if Cancel arrived while Google was processing it. Subsequent attempts still
    // check the real cancellation flag before sending anything.
    api_keys::with_keys(Provider::Gemini, keys, &AtomicBool::new(false), |key| {
        let value = request(key, "voices", Some(&body), &[], cancel)?;
        let created = voice(&value, key)?;
        // Keep a created ID even when sample audio is missing/invalid: do not recreate a paid voice.
        let preview = value.get("sample_audio").and_then(|audio| {
            save_audio(
                &json!({"output_audio":audio}),
                project,
                "gemini-voice-sample",
            )
            .ok()
        });
        Ok((created, preview))
    })
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn dialogue_maps_speakers_styles_and_keeps_tags() {
        let body = dialogue_body(
            model(0),
            ["Charon", "Kore"],
            &[
                Turn {
                    speaker: 0,
                    text: "<laugh> Xin chào |ừ|".into(),
                    style: "cheerful".into(),
                },
                Turn {
                    speaker: 1,
                    text: "Chào bạn".into(),
                    style: "calm".into(),
                },
            ],
            1.0,
        )
        .unwrap();
        assert_eq!(
            body["input"][0]["content"][0]["text"],
            "<laugh> Xin chào |ừ|"
        );
        assert_eq!(
            body["input"][0]["content"][1]["annotations"][0]["speaker"],
            "B"
        );
        assert_eq!(
            body["generation_config"]["speech_config"]["speakers"][1]["voice"],
            "Kore"
        );
        assert!(dialogue_body(model(0), ["Charon", "Kore"], &[], 1.0).is_err());
        assert!(
            dialogue_body(
                model(0),
                ["../bad", "Kore"],
                &[Turn {
                    speaker: 0,
                    text: "test".into(),
                    style: String::new()
                }],
                1.0
            )
            .is_err()
        );
    }
    #[test]
    fn voice_credentials_are_not_printed() {
        let v = voice(
            &json!({"id":"voice_example","display_name":"Test","type":"prompted"}),
            "private-test-key",
        )
        .unwrap();
        assert!(!format!("{v:?}").contains("private-test-key"));
    }
}
