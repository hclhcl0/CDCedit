// SPDX-License-Identifier: AGPL-3.0-or-later
//! Opt-in cloud TTS. Credentials never enter filenames, logs or project data.
use base64::{Engine, engine::general_purpose::STANDARD};
use serde_json::{Value, json};
use std::{
    io::Read,
    path::{Path, PathBuf},
    sync::atomic::{AtomicBool, AtomicU64, Ordering},
    time::Duration,
};

/// Settings supported by the selected cloud model; not project state.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct VoiceOptions {
    pub delivery: String,
    /// Sampling temperature for Gemini speech generation.
    pub temperature: f32,
    /// Vietnamese accent instruction: 0 default, 1 north, 2 central, 3 south.
    pub accent: i32,
    /// Gemini speaking style preset.
    pub delivery_preset: i32,
    pub stability: f32,
    pub similarity: f32,
    pub style: f32,
    pub speed: f32,
    pub speaker_boost: bool,
}
impl Default for VoiceOptions {
    fn default() -> Self {
        Self {
            delivery: String::new(),
            temperature: 1.0,
            accent: 0,
            delivery_preset: 0,
            stability: 0.5,
            similarity: 0.75,
            style: 0.0,
            speed: 1.0,
            speaker_boost: true,
        }
    }
}
pub const GEMINI_MODELS: &[&str] = &[
    "gemini-3.8-flash-tts",
    "gemini-3.8-flash-lite-tts",
    "gemini-2.5-flash-preview-tts",
    "gemini-2.5-pro-preview-tts",
];
pub const ELEVEN_MODELS: &[&str] = &[
    "eleven_v3",
    "eleven_v4",
    "eleven_v4_turbo",
    "eleven_flash_v2_5",
    "eleven_multilingual_v2",
];
pub fn models(provider: i32) -> &'static [&'static str] {
    if provider == 1 {
        GEMINI_MODELS
    } else {
        ELEVEN_MODELS
    }
}
/// v4 accepts stability/similarity only. Unknown models get the same conservative controls.
pub fn has_extended_settings(model: &str) -> bool {
    matches!(
        model,
        "eleven_v3" | "eleven_multilingual_v2" | "eleven_flash_v2_5"
    )
}
fn voice_settings(model: &str, options: &VoiceOptions) -> Value {
    let stability = options.stability.clamp(0.0, 1.0);
    let mut settings = json!({"stability": if model == "eleven_v3" { (stability * 2.0).round() / 2.0 } else { stability }, "similarity_boost": options.similarity.clamp(0.0,1.0)});
    if has_extended_settings(model) {
        settings["style"] = json!(options.style.clamp(0.0, 1.0));
        settings["speed"] = json!(options.speed.clamp(0.7, 1.2));
        settings["use_speaker_boost"] = json!(options.speaker_boost);
    }
    settings
}

static SERIAL: AtomicU64 = AtomicU64::new(0);
/// Allocate a unique narration file in the project audio folder.
pub fn output(project: &str, label: &str) -> Result<PathBuf, String> {
    let dir = Path::new(project).join("audio");
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|e| e.to_string())?
        .as_millis();
    Ok(dir.join(format!(
        "{label}-{stamp}-{}-{}.wav",
        std::process::id(),
        SERIAL.fetch_add(1, Ordering::Relaxed)
    )))
}
/// Stop before publishing a cancelled job.
pub fn check_cancel(cancel: &AtomicBool) -> Result<(), String> {
    if cancel.load(Ordering::Relaxed) {
        Err("Speech generation cancelled".into())
    } else {
        Ok(())
    }
}
/// Single speaker. Gemini uses the documented Interactions TTS API; ElevenLabs returns 24kHz PCM.
pub fn synthesize(
    provider: i32,
    key: &str,
    model: &str,
    voice: &str,
    text: &str,
    project: &str,
    cancel: &AtomicBool,
    options: &VoiceOptions,
) -> Result<PathBuf, String> {
    if text.chars().count() > 20000 {
        return Err("Use up to 20,000 characters per dubbing pass".into());
    }
    let chunks = script_chunks(text, 4500)?;
    if chunks.len() == 1 {
        return synthesize_one(
            provider, key, model, voice, &chunks[0], "", project, cancel, options,
        );
    }
    let mut joined = Vec::new();
    let mut previous = String::new();
    for chunk in chunks {
        check_cancel(cancel)?;
        let file = synthesize_one(
            provider, key, model, voice, &chunk, &previous, project, cancel, options,
        )?;
        let mut reader = concat_media::AudioDecoder::open(
            &file,
            &concat_media::AudioOptions {
                rate: 24000,
                channels: 1,
                format: concat_media::SampleFormat::F32,
            ..Default::default()

            },
        )
        .map_err(|e| e.to_string())?;
        if !joined.is_empty() {
            joined.extend(std::iter::repeat_n(0.0, 2400));
        }
        joined.extend(reader.collect_f32().map_err(|e| e.to_string())?);
        previous = chunk;
    }
    check_cancel(cancel)?;
    let file = output(project, "dub-unified")?;
    write_wav(&file, &joined, 24000)?;
    Ok(file)
}
fn script_chunks(text: &str, limit: usize) -> Result<Vec<String>, String> {
    let mut chunks = Vec::new();
    let mut current = String::new();
    let mut count = 0;
    // Keep inline vocal events such as <short pause> together across requests.
    let mut in_tag = false;
    for word in text
        .split(|c: char| {
            if c == '<' {
                in_tag = true;
            }
            if c == '>' {
                in_tag = false;
            }
            c.is_whitespace() && !in_tag
        })
        .filter(|word| !word.is_empty())
    {
        let size = word.chars().count();
        if size > limit {
            return Err("A word is longer than the speech service limit".into());
        }
        if count + size + 1 > limit && !current.is_empty() {
            chunks.push(std::mem::take(&mut current));
            count = 0;
        }
        if !current.is_empty() {
            current.push(' ');
            count += 1;
        }
        current.push_str(word);
        count += size;
    }
    if !current.is_empty() {
        chunks.push(current);
    }
    if chunks.is_empty() {
        return Err("Enter text to read aloud".into());
    }
    Ok(chunks)
}
fn synthesize_one(
    provider: i32,
    keys: &str,
    model: &str,
    voice: &str,
    text: &str,
    previous: &str,
    project: &str,
    cancel: &AtomicBool,
    options: &VoiceOptions,
) -> Result<PathBuf, String> {
    let pool = match provider {
        1 => crate::api_keys::Provider::Gemini,
        2 => crate::api_keys::Provider::ElevenLabs,
        _ => return Err("Unknown TTS provider".into()),
    };
    crate::api_keys::with_keys(pool, keys, cancel, |key| {
        synthesize_one_key(
            provider, key, model, voice, text, previous, project, cancel, options,
        )
    })
}
fn synthesize_one_key(
    provider: i32,
    key: &str,
    model: &str,
    voice: &str,
    text: &str,
    previous: &str,
    project: &str,
    cancel: &AtomicBool,
    options: &VoiceOptions,
) -> Result<PathBuf, String> {
    check_cancel(cancel)?;
    if key.trim().is_empty() {
        return Err("Enter an API key for the selected TTS service".into());
    }
    if text.trim().is_empty() || text.chars().count() > 5000 {
        return Err(
            "Use between 1 and 5,000 characters per generation. Split longer tracks first.".into(),
        );
    }
    if voice.is_empty()
        || !voice
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
    {
        return Err("Invalid voice name or voice ID".into());
    }
    let (url, header, body) = if provider == 1 {
        let (url, body) = gemini_request(model, voice, text, options)?;
        (url, "x-goog-api-key", body)
    } else if provider == 2 {
        (
            format!("https://api.elevenlabs.io/v1/text-to-speech/{voice}?output_format=pcm_24000"),
            "xi-api-key",
            json!({"text":text,"model_id":model,"previous_text":previous,"voice_settings":voice_settings(model, options)}),
        )
    } else {
        return Err("Unknown TTS provider".into());
    };
    let agent = ureq::AgentBuilder::new()
        .timeout_connect(Duration::from_secs(15))
        .timeout_read(Duration::from_secs(120))
        .timeout_write(Duration::from_secs(30))
        .build();
    let body = serde_json::to_vec(&body).map_err(|e| e.to_string())?;
    let response = agent
        .post(&url)
        .set(header, key.trim())
        .set("Content-Type", "application/json")
        .send_bytes(&body)
        .map_err(|error| crate::api_keys::http_error("TTS service", error))?;
    let mut bytes = Vec::new();
    response
        .into_reader()
        .take(100_000_001)
        .read_to_end(&mut bytes)
        .map_err(|_| "Could not read TTS audio")?;
    check_cancel(cancel)?;
    if bytes.len() > 100_000_000 {
        return Err("TTS response is too large".into());
    }
    let wav = if provider == 1 {
        let response = serde_json::from_slice(&bytes).map_err(|_| "Invalid Gemini TTS response")?;
        if is_gemini_25(model) {
            gemini_25_audio(&response)?
        } else {
            gemini_audio(&response)?
        }
    } else {
        pcm_wav(&bytes, 24_000)?
    };
    let file = output(project, "dubbing")?;
    std::fs::write(&file, wav).map_err(|e| e.to_string())?;
    Ok(file)
}
fn is_gemini_25(model: &str) -> bool {
    matches!(
        model,
        "gemini-2.5-flash-preview-tts" | "gemini-2.5-pro-preview-tts"
    )
}
fn gemini_request(
    model: &str,
    voice: &str,
    text: &str,
    options: &VoiceOptions,
) -> Result<(String, Value), String> {
    if !options.temperature.is_finite() || !(0.0..=2.0).contains(&options.temperature) {
        return Err("Temperature must be between 0 and 2.".into());
    }
    if is_gemini_25(model) {
        let prompt = if options.delivery.trim().is_empty() {
            text.to_owned()
        } else {
            format!(
                "Read aloud exactly the transcript below. Do not read the instructions. Delivery instructions: {}\n\nTranscript:\n{}",
                options.delivery.trim(),
                text
            )
        };
        Ok((
            format!(
                "https://generativelanguage.googleapis.com/v1beta/models/{model}:generateContent"
            ),
            json!({"contents":[{"role":"user","parts":[{"text":prompt}]}],"generationConfig":{
                "temperature":options.temperature,"responseModalities":["AUDIO"],
                "speechConfig":{"voiceConfig":{"prebuiltVoiceConfig":{"voiceName":voice}}}
            }}),
        ))
    } else {
        let mut part = json!({"type":"text","text":text});
        if !options.delivery.trim().is_empty() {
            part["annotations"] =
                json!([{"type":"speech_metadata","style":options.delivery.trim()}]);
        }
        Ok((
            "https://generativelanguage.googleapis.com/v1beta/interactions".into(),
            json!({
                "model":model,"input":[{"type":"user_input","content":[part]}],"response_format":{"type":"audio"},
                "generation_config":{"temperature":options.temperature,"speech_config":[{"voice":voice}]}
            }),
        ))
    }
}
fn gemini_25_audio(response: &Value) -> Result<Vec<u8>, String> {
    let candidate = response["candidates"]
        .as_array()
        .and_then(|c| c.first())
        .ok_or("Gemini returned no audio candidate. Check quota and safety restrictions.")?;
    let mut pcm = Vec::new();
    for part in candidate["content"]["parts"]
        .as_array()
        .into_iter()
        .flatten()
    {
        let Some(audio) = part.get("inlineData").or_else(|| part.get("inline_data")) else {
            continue;
        };
        let mime = audio
            .get("mimeType")
            .or_else(|| audio.get("mime_type"))
            .and_then(Value::as_str)
            .unwrap_or("");
        if !mime.starts_with("audio/L16;") && !mime.starts_with("audio/pcm;") {
            return Err("Unexpected Gemini 2.5 audio format.".into());
        }
        if !mime.split(';').any(|p| p.trim() == "rate=24000") {
            return Err("Expected Gemini 2.5 mono 24 kHz PCM.".into());
        }
        if mime
            .split(';')
            .any(|p| p.trim().starts_with("channels=") && p.trim() != "channels=1")
        {
            return Err("Expected mono Gemini audio.".into());
        }
        let data = audio["data"]
            .as_str()
            .ok_or("Gemini audio data is missing")?;
        pcm.extend(
            STANDARD
                .decode(data)
                .map_err(|_| "Invalid Gemini audio encoding")?,
        );
    }
    pcm_wav(&pcm, 24_000)
}
pub(crate) fn gemini_audio(response: &Value) -> Result<Vec<u8>, String> {
    let audio = response
        .get("steps")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter(|step| step.get("type").and_then(Value::as_str) == Some("model_output"))
        .filter_map(|step| step.get("content").and_then(Value::as_array))
        .flatten()
        .filter(|part| part.get("type").and_then(Value::as_str) == Some("audio"))
        .last()
        .or_else(|| response.get("output_audio"))
        .ok_or("Gemini returned no audio. Check the model and text.")?;
    let encoded = audio
        .get("data")
        .and_then(Value::as_str)
        .ok_or("Gemini audio data is missing")?;
    let bytes = STANDARD
        .decode(encoded)
        .map_err(|_| "Invalid Gemini audio encoding")?;
    if bytes.len() < 44 || &bytes[..4] != b"RIFF" || &bytes[8..12] != b"WAVE" {
        return Err("Gemini TTS must return WAV audio. Choose a Gemini 3.8 TTS model.".into());
    }
    Ok(bytes)
}
/// Wrap signed little-endian mono PCM in a WAV header.
pub fn pcm_wav(pcm: &[u8], rate: u32) -> Result<Vec<u8>, String> {
    if pcm.is_empty() || pcm.len() % 2 != 0 || pcm.len() > u32::MAX as usize - 36 {
        return Err("Invalid PCM audio length".into());
    }
    let mut wav = Vec::with_capacity(pcm.len() + 44);
    wav.extend_from_slice(b"RIFF");
    wav.extend_from_slice(&(36 + pcm.len() as u32).to_le_bytes());
    wav.extend_from_slice(b"WAVEfmt ");
    wav.extend_from_slice(&16u32.to_le_bytes());
    wav.extend_from_slice(&1u16.to_le_bytes());
    wav.extend_from_slice(&1u16.to_le_bytes());
    wav.extend_from_slice(&rate.to_le_bytes());
    wav.extend_from_slice(&(rate * 2).to_le_bytes());
    wav.extend_from_slice(&2u16.to_le_bytes());
    wav.extend_from_slice(&16u16.to_le_bytes());
    wav.extend_from_slice(b"data");
    wav.extend_from_slice(&(pcm.len() as u32).to_le_bytes());
    wav.extend_from_slice(pcm);
    Ok(wav)
}
/// Write mono float samples as a portable PCM16 WAV.
pub fn write_wav(path: &Path, samples: &[f32], rate: u32) -> Result<(), String> {
    let pcm: Vec<u8> = samples
        .iter()
        .flat_map(|s| ((s.clamp(-1.0, 1.0) * 32767.0) as i16).to_le_bytes())
        .collect();
    std::fs::write(path, pcm_wav(&pcm, rate)?).map_err(|e| e.to_string())
}

/// Returns `(default_pitch_semitones, default_denoise)` for a DSP voice changer preset.
pub fn dsp_preset_defaults(preset: i32) -> (f32, bool) {
    match preset {
        0 => (0.0, true),   // Phòng thu (Khử ồn + Làm ấm)
        1 => (-3.5, true),  // Nam trầm ấm (Deep Male)
        2 => (4.0, true),   // Nữ cao / Giọng sáng (Bright Female)
        3 => (-5.5, true),  // Ẩn danh / Phóng sự (Anonymous)
        4 => (7.0, false),  // Trẻ em / Sóc chuột (Chipmunk)
        5 => (-7.0, false), // Khổng lồ / Quái vật (Giant)
        6 => (0.0, false),  // Robot / Cơ khí (Robot)
        7 => (0.0, false),  // Bộ đàm / Điện thoại (Radio / Phone)
        8 => (0.0, false),  // Vang vọng / Hội trường (Hall Echo)
        _ => (0.0, false),  // Tùy chỉnh Cao độ & Tốc độ
    }
}

/// Offline DSP Voice Changer & Studio Clean via FFmpeg avfilter.
pub fn transform_voice_dsp(
    source_path: &Path,
    audio_stream: Option<usize>,
    source_start: f64,
    window: Option<f64>,
    preset: i32,
    pitch_semitones: f32,
    speed: f32,
    denoise: bool,
    project: &str,
    cancel: &AtomicBool,
) -> Result<PathBuf, String> {
    check_cancel(cancel)?;
    let mut chain_parts: Vec<String> = Vec::new();
    if source_start > 0.001 || window.is_some() {
        if let Some(dur) = window {
            chain_parts.push(format!(
                "atrim=start={:.3}:duration={:.3},asetpts=PTS-STARTPTS",
                source_start.max(0.0),
                dur.max(0.05)
            ));
        } else {
            chain_parts.push(format!(
                "atrim=start={:.3},asetpts=PTS-STARTPTS",
                source_start.max(0.0)
            ));
        }
    }
    if denoise || preset == 0 {
        chain_parts.push("highpass=f=75,afftdn=nf=-25".to_owned());
    }
    let pitch = pitch_semitones.clamp(-12.0, 12.0);
    let spd = speed.clamp(0.5, 2.0);
    if pitch.abs() > 0.01 || (spd - 1.0).abs() > 0.01 {
        let ratio = 2.0_f64.powf(f64::from(pitch) / 12.0);
        let mut tempo = f64::from(spd) / ratio;
        let mut atempos = Vec::new();
        while tempo < 0.5 {
            atempos.push("atempo=0.5".to_owned());
            tempo /= 0.5;
        }
        while tempo > 2.0 {
            atempos.push("atempo=2.0".to_owned());
            tempo /= 2.0;
        }
        atempos.push(format!("atempo={:.6}", tempo.clamp(0.5, 2.0)));
        chain_parts.push(format!(
            "aresample=48000,asetrate={:.4},aresample=48000,{}",
            48000.0 * ratio,
            atempos.join(",")
        ));
    }
    let tone = match preset {
        0 => {
            "equalizer=f=150:t=q:w=1.0:g=2.5,equalizer=f=3200:t=q:w=1.2:g=2.0,acompressor=threshold=-18dB:ratio=2.5:attack=10:release=100,alimiter=limit=0.95"
        }
        1 => {
            "highpass=f=55,equalizer=f=140:t=q:w=1.0:g=3.0,equalizer=f=2600:t=q:w=0.9:g=-1.2,alimiter=limit=0.94"
        }
        2 => "highpass=f=110,equalizer=f=2800:t=q:w=1.1:g=2.0,alimiter=limit=0.94",
        3 => "highpass=f=90,lowpass=f=4200,equalizer=f=120:t=q:w=1.0:g=3.5,alimiter=limit=0.94",
        4 => "highpass=f=120,alimiter=limit=0.94",
        5 => {
            "highpass=f=50,equalizer=f=130:t=q:w=1.0:g=4.0,aecho=0.8:0.85:60|110:0.4|0.25,alimiter=limit=0.94"
        }
        6 => {
            "aphaser=type=t:speed=1.2:decay=0.6,flanger=delay=2:depth=4:speed=0.8,equalizer=f=1800:t=q:w=1.2:g=3,alimiter=limit=0.94"
        }
        7 => {
            "highpass=f=350,lowpass=f=3200,acrusher=bits=11:mode=log:mix=0.45,equalizer=f=1600:t=q:w=1.4:g=4.0,alimiter=limit=0.92"
        }
        8 => "aecho=0.8:0.85:60|110|180|270:0.45|0.35|0.25|0.18,alimiter=limit=0.94",
        _ => "alimiter=limit=0.95",
    };
    chain_parts.push(tone.to_owned());
    let mut decoder = concat_media::AudioDecoder::open(
        source_path,
        &concat_media::AudioOptions {
            stream: audio_stream,
            filters: vec![chain_parts.join(",")],
            rate: 24000,
            channels: 1,
            format: concat_media::SampleFormat::F32,
            ..Default::default()
        },
    )
    .map_err(|e| format!("Không thể đọc âm thanh nguồn: {e}"))?;
    let samples = decoder
        .collect_f32()
        .map_err(|e| format!("Lỗi xử lý DSP âm thanh: {e}"))?;
    if samples.is_empty() {
        return Err("Bản thu hoặc file nguồn không có dữ liệu âm thanh.".into());
    }
    check_cancel(cancel)?;
    let out = output(project, "voice-dsp")?;
    write_wav(&out, &samples, 24000)?;
    Ok(out)
}

/// Cloud AI Speech-to-Speech (Voice Conversion keeping original cadence/emotion).
pub fn synthesize_speech_to_speech(
    provider: i32,
    keys: &str,
    model: &str,
    voice: &str,
    source_path: &Path,
    audio_stream: Option<usize>,
    source_start: f64,
    window: Option<f64>,
    denoise: bool,
    project: &str,
    cancel: &AtomicBool,
    options: &VoiceOptions,
) -> Result<PathBuf, String> {
    check_cancel(cancel)?;
    if keys.trim().is_empty() {
        return Err(
            "Vui lòng nhập API Key (Gemini hoặc ElevenLabs) trong Cài đặt để đổi giọng AI S2S."
                .into(),
        );
    }
    if voice.is_empty()
        || !voice
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
    {
        return Err("Voice ID không hợp lệ.".into());
    }
    let mut pre_filters = Vec::new();
    if source_start > 0.001 || window.is_some() {
        if let Some(dur) = window {
            pre_filters.push(format!(
                "atrim=start={:.3}:duration={:.3},asetpts=PTS-STARTPTS",
                source_start.max(0.0),
                dur.max(0.05)
            ));
        } else {
            pre_filters.push(format!(
                "atrim=start={:.3},asetpts=PTS-STARTPTS",
                source_start.max(0.0)
            ));
        }
    }
    if denoise && provider == 1 {
        pre_filters.push("highpass=f=75,afftdn=nf=-25".to_owned());
    }
    let filters = if pre_filters.is_empty() {
        Vec::new()
    } else {
        vec![pre_filters.join(",")]
    };
    let mut decoder = concat_media::AudioDecoder::open(
        source_path,
        &concat_media::AudioOptions {
            stream: audio_stream,
            filters,
            rate: 24000,
            channels: 1,
            format: concat_media::SampleFormat::F32,
            ..Default::default()
        },
    )
    .map_err(|e| format!("Không thể đọc file âm thanh nguồn: {e}"))?;
    let samples = decoder.collect_f32().map_err(|e| e.to_string())?;
    if samples.is_empty() {
        return Err("File nguồn không có dữ liệu âm thanh.".into());
    }
    if samples.len() > 24000 * 300 {
        return Err(
            "Đổi giọng AI Speech-to-Speech hỗ trợ tối đa 5 phút mỗi lượt. Hãy cắt ngắn đoạn thu trước."
                .into(),
        );
    }
    let pcm: Vec<u8> = samples
        .iter()
        .flat_map(|s| ((s.clamp(-1.0, 1.0) * 32767.0) as i16).to_le_bytes())
        .collect();
    let source_wav = pcm_wav(&pcm, 24000)?;
    check_cancel(cancel)?;

    if provider == 2 {
        let sts_model = if model.contains("sts") {
            model
        } else {
            "eleven_multilingual_sts_v2"
        };
        let settings_json = serde_json::to_string(&json!({
            "stability": options.stability.clamp(0.0, 1.0),
            "similarity_boost": options.similarity.clamp(0.0, 1.0),
            "style": options.style.clamp(0.0, 1.0),
            "use_speaker_boost": options.speaker_boost,
        }))
        .map_err(|e| e.to_string())?;
        let boundary = "----CDCeditElevenLabsSTSBoundary98421";
        let mut body = Vec::with_capacity(source_wav.len() + 1024);
        let mut add_field = |name: &str, val: &str| {
            body.extend_from_slice(format!("--{boundary}\r\n").as_bytes());
            body.extend_from_slice(
                format!("Content-Disposition: form-data; name=\"{name}\"\r\n\r\n{val}\r\n")
                    .as_bytes(),
            );
        };
        add_field("model_id", sts_model);
        add_field(
            "remove_background_noise",
            if denoise { "true" } else { "false" },
        );
        add_field("voice_settings", &settings_json);
        body.extend_from_slice(format!("--{boundary}\r\n").as_bytes());
        body.extend_from_slice(
            b"Content-Disposition: form-data; name=\"audio\"; filename=\"source.wav\"\r\n",
        );
        body.extend_from_slice(b"Content-Type: audio/wav\r\n\r\n");
        body.extend_from_slice(&source_wav);
        body.extend_from_slice(format!("\r\n--{boundary}--\r\n").as_bytes());

        let url =
            format!("https://api.elevenlabs.io/v1/speech-to-speech/{voice}?output_format=pcm_24000");
        let content_type = format!("multipart/form-data; boundary={boundary}");
        crate::api_keys::with_keys(
            crate::api_keys::Provider::ElevenLabs,
            keys,
            cancel,
            |key| {
                check_cancel(cancel)?;
                let agent = ureq::AgentBuilder::new()
                    .timeout_connect(Duration::from_secs(15))
                    .timeout_read(Duration::from_secs(180))
                    .timeout_write(Duration::from_secs(60))
                    .build();
                let response = agent
                    .post(&url)
                    .set("xi-api-key", key.trim())
                    .set("Content-Type", &content_type)
                    .send_bytes(&body)
                    .map_err(|e| crate::api_keys::http_error("ElevenLabs Speech-to-Speech", e))?;
                let mut out_bytes = Vec::new();
                response
                    .into_reader()
                    .take(100_000_001)
                    .read_to_end(&mut out_bytes)
                    .map_err(|_| "Không đọc được dữ liệu âm thanh từ ElevenLabs S2S")?;
                check_cancel(cancel)?;
                let wav = pcm_wav(&out_bytes, 24_000)?;
                let file = output(project, "voice-sts-eleven")?;
                std::fs::write(&file, wav).map_err(|e| e.to_string())?;
                Ok(file)
            },
        )
    } else if provider == 1 {
        let encoded_wav = STANDARD.encode(&source_wav);
        let (transcript, detected_style) = crate::api_keys::with_keys(
            crate::api_keys::Provider::Gemini,
            keys,
            cancel,
            |key| {
                check_cancel(cancel)?;
                let url = "https://generativelanguage.googleapis.com/v1beta/models/gemini-2.5-flash:generateContent";
                let req_body = serde_json::to_vec(&json!({
                    "contents": [{
                        "role": "user",
                        "parts": [
                            {
                                "inlineData": {
                                    "mimeType": "audio/wav",
                                    "data": encoded_wav,
                                }
                            },
                            {
                                "text": "Listen carefully to this audio recording. Return a JSON object with two fields: \"transcript\" (the exact verbatim words spoken in the original language, preserving natural pauses or expressions) and \"style\" (a concise English description of the speaker's emotional tone, pacing, energy, and inflection so a voice actor can replicate the exact delivery)."
                            }
                        ]
                    }],
                    "generationConfig": {
                        "temperature": 0.2,
                        "responseMimeType": "application/json"
                    }
                }))
                .map_err(|e| e.to_string())?;
                let agent = ureq::AgentBuilder::new()
                    .timeout_connect(Duration::from_secs(15))
                    .timeout_read(Duration::from_secs(120))
                    .timeout_write(Duration::from_secs(60))
                    .build();
                let resp = agent
                    .post(url)
                    .set("x-goog-api-key", key.trim())
                    .set("Content-Type", "application/json")
                    .send_bytes(&req_body)
                    .map_err(|e| crate::api_keys::http_error("Gemini Audio S2S", e))?;
                let mut resp_bytes = Vec::new();
                resp.into_reader()
                    .take(10_000_000)
                    .read_to_end(&mut resp_bytes)
                    .map_err(|_| "Không đọc được phản hồi từ Gemini")?;
                let parsed: Value = serde_json::from_slice(&resp_bytes)
                    .map_err(|_| "Phản hồi Gemini không hợp lệ")?;
                let text_out = parsed["candidates"][0]["content"]["parts"][0]["text"]
                    .as_str()
                    .ok_or("Gemini không nhận diện được giọng nói trong bản thu.")?;
                let info: Value = serde_json::from_str(text_out).unwrap_or_else(|_| {
                    json!({
                        "transcript": text_out,
                        "style": ""
                    })
                });
                let tr = info["transcript"]
                    .as_str()
                    .unwrap_or(text_out)
                    .trim()
                    .to_owned();
                let st = info["style"].as_str().unwrap_or("").trim().to_owned();
                if tr.is_empty() {
                    return Err("Không phát hiện lời nói trong bản thu âm.".into());
                }
                Ok((tr, st))
            },
        )?;
        let mut s2s_options = options.clone();
        if !detected_style.is_empty() {
            s2s_options.delivery = if s2s_options.delivery.trim().is_empty() {
                detected_style
            } else {
                format!("{}. {}", s2s_options.delivery.trim(), detected_style)
            };
        }
        synthesize(
            1,
            keys,
            model,
            voice,
            &transcript,
            project,
            cancel,
            &s2s_options,
        )
    } else {
        Err("Hãy chọn Gemini hoặc ElevenLabs cho chế độ AI Speech-to-Speech.".into())
    }

}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn gemini_versions_use_their_own_protocol_and_temperature() {
        let mut options = VoiceOptions::default();
        options.temperature = 0.35;
        options.delivery = "Warm narration".into();
        let (url, body) =
            gemini_request("gemini-2.5-flash-preview-tts", "Kore", "Xin chào", &options).unwrap();
        assert!(url.ends_with("gemini-2.5-flash-preview-tts:generateContent"));
        assert!((body["generationConfig"]["temperature"].as_f64().unwrap() - 0.35).abs() < 0.0001);
        assert_eq!(
            body["generationConfig"]["speechConfig"]["voiceConfig"]["prebuiltVoiceConfig"]["voiceName"],
            "Kore"
        );
        assert!(
            body["contents"][0]["parts"][0]["text"]
                .as_str()
                .unwrap()
                .ends_with("Xin chào")
        );
        let (url, body) =
            gemini_request("gemini-3.8-flash-tts", "Kore", "Xin chào", &options).unwrap();
        assert!(url.ends_with("/interactions"));
        assert_eq!(body["input"][0]["content"][0]["text"], "Xin chào");
        assert!(body["generation_config"]["temperature"].is_number());
        options.temperature = f32::NAN;
        assert!(gemini_request("gemini-2.5-pro-preview-tts", "Kore", "test", &options).is_err());
        let restored: VoiceOptions = serde_json::from_str("{}").unwrap();
        assert_eq!(restored.temperature, 1.0);
    }
    #[test]
    fn gemini_25_pcm_is_wrapped_and_text_only_or_wrong_rate_rejected() {
        let response = json!({"candidates":[{"content":{"parts":[{"inlineData":{"mimeType":"audio/L16;codec=pcm;rate=24000", "data":STANDARD.encode([1,0,2,0])}}]}}]});
        let wav = gemini_25_audio(&response).unwrap();
        assert_eq!(&wav[44..], &[1, 0, 2, 0]);
        assert_eq!(&wav[..4], b"RIFF");
        assert!(
            gemini_25_audio(&json!({"candidates":[{"content":{"parts":[{"text":"no audio"}]}}]}))
                .is_err()
        );
        let mut bad = response;
        bad["candidates"][0]["content"]["parts"][0]["inlineData"]["mimeType"] =
            json!("audio/L16;rate=16000");
        assert!(gemini_25_audio(&bad).is_err());
    }
    #[test]
    fn model_capabilities_filter_request_settings() {
        let options = VoiceOptions {
            stability: 0.66,
            speed: 1.1,
            style: 0.4,
            ..Default::default()
        };
        let v4 = voice_settings("eleven_v4", &options);
        assert_eq!(v4.as_object().unwrap().len(), 2);
        assert!(v4.get("speed").is_none());
        assert!(v4.get("style").is_none());
        let v3 = voice_settings("eleven_v3", &options);
        assert_eq!(v3["stability"], 0.5);
        assert!(v3.get("speed").is_some());
        assert!(v3.get("use_speaker_boost").is_some());
    }
    #[test]
    fn reads_documented_interaction_audio_and_rejects_non_wav() {
        let wav = pcm_wav(&[0, 0, 1, 0], 24000).unwrap();
        let response = json!({"steps":[{"type":"model_output","content":[{"type":"audio","data":STANDARD.encode(&wav)}]}]});
        assert_eq!(gemini_audio(&response).unwrap(), wav);
        assert!(gemini_audio(&json!({"output_audio":{"data":STANDARD.encode(b"bad")}})).is_err());
    }
    #[test]
    fn invalid_credentials_and_cancel_do_not_make_requests() {
        let cancel = AtomicBool::new(false);
        assert!(
            synthesize(
                1,
                "",
                "gemini-3.8-flash-tts",
                "Kore",
                "Hello",
                ".",
                &cancel,
                &VoiceOptions::default()
            )
            .unwrap_err()
            .contains("API key")
        );
        cancel.store(true, Ordering::Relaxed);
        assert!(
            synthesize(
                2,
                "key",
                "eleven_v3",
                "id",
                "Hello",
                ".",
                &cancel,
                &VoiceOptions::default()
            )
            .unwrap_err()
            .contains("cancelled")
        );
    }
    #[test]
    fn inline_vocal_tags_survive_chunking_and_gemini_request() {
        let text = "Xin <short pause> chào <laugh>";
        let chunks = script_chunks(text, 16).unwrap();
        assert_eq!(chunks.join(" "), text);
        for chunk in chunks {
            assert_eq!(chunk.matches('<').count(), chunk.matches('>').count());
        }
        let (_, body) = gemini_request(
            "gemini-3.8-flash-tts",
            "Kore",
            text,
            &VoiceOptions::default(),
        )
        .unwrap();
        assert_eq!(body["input"][0]["content"][0]["text"], text);
    }
    #[test]
    fn long_unicode_scripts_split_without_losing_words() {
        let text = "Xin chào Việt Nam. ".repeat(1000);
        let chunks = script_chunks(&text, 4500).unwrap();
        assert!(chunks.len() > 1);
        assert!(chunks.iter().all(|c| c.chars().count() <= 4500));
        assert_eq!(
            chunks.join(" "),
            text.split_whitespace().collect::<Vec<_>>().join(" ")
        );
    }
}
