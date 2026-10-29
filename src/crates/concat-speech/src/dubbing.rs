// SPDX-License-Identifier: AGPL-3.0-or-later
//! Match script sentences to Whisper word timings, and render a reviewable timed dub.
use crate::{cloud_tts, transcribe::Segment};
use concat_media::{AudioDecoder, AudioOptions, SampleFormat};
use std::{path::PathBuf, sync::atomic::AtomicBool};

#[derive(Clone, Debug, Default)]
/// A draft sentence, its target slot, and the generated source audio range.
pub struct Cue {
    /// Editable script.
    pub text: String,
    /// Target start on the timeline, in seconds.
    pub start: f64,
    /// Target end on the timeline, in seconds.
    pub end: f64,
    /// Generated audio file; absent after a text edit.
    pub audio: Option<PathBuf>,
    /// Start of this sentence within generated audio.
    pub audio_start: f64,
    /// End of this sentence within generated audio.
    pub audio_end: f64,
}
fn normalized(text: &str) -> String {
    text.chars()
        .flat_map(char::to_lowercase)
        .filter(|c| c.is_alphanumeric())
        .collect()
}
/// Exclude non-spoken Gemini vocal events from lexical Whisper alignment.
pub fn without_vocal_tags(text: &str) -> String {
    let mut output = String::new();
    let mut rest = text;
    while let Some(start) = rest.find('<') {
        output.push_str(&rest[..start]);
        let Some(end) = rest[start..].find('>').map(|end| start + end) else {
            output.push_str(&rest[start..]);
            return output;
        };
        let tag = rest[start + 1..end].trim().to_ascii_lowercase();
        if matches!(
            tag.as_str(),
            "argh"
                | "breath"
                | "heavy breath"
                | "exhales"
                | "cackle"
                | "cheer"
                | "chuckle"
                | "chuckles"
                | "cough"
                | "cry"
                | "gasp"
                | "giggle"
                | "groan"
                | "growl"
                | "grunt"
                | "grr"
                | "hiss"
                | "laugh"
                | "laughter"
                | "moan"
                | "pant"
                | "pff"
                | "phew"
                | "scream"
                | "shout"
                | "shriek"
                | "sigh"
                | "sighs"
                | "sneeze"
                | "snicker"
                | "snort"
                | "sob"
                | "throat-clearing"
                | "tsk"
                | "whimper"
                | "whispers"
                | "whispering"
                | "yawn"
                | "short pause"
                | "long pause"
        ) {
            output.push(' ');
        } else {
            output.push_str(&rest[start..=end]);
        }
        rest = &rest[end + 1..];
    }
    output.push_str(rest);
    output
}
/// LCS matching tolerates punctuation and occasional recognition mistakes. Reject low-confidence rows.
pub fn align(lines: &[String], words: &[Segment]) -> Result<Vec<(f64, f64)>, String> {
    let mut expected = Vec::new();
    for (row, line) in lines.iter().enumerate() {
        let spoken = without_vocal_tags(line);
        for word in spoken.split_whitespace() {
            let word = normalized(word);
            if !word.is_empty() {
                expected.push((row, word));
            }
        }
    }
    let heard: Vec<_> = words
        .iter()
        .enumerate()
        .filter_map(|(i, w)| {
            let norm = normalized(&w.text);
            (!norm.is_empty()).then_some((i, norm))
        })
        .collect();
    let (n, m) = (expected.len(), heard.len());
    if n == 0 || m == 0 || n > 6000 || m > 8000 {
        return Err(
            "Could not align this script. Use a shorter track and a multilingual Whisper model."
                .into(),
        );
    }
    let mut table = vec![0u16; (n + 1) * (m + 1)];
    let at = |i: usize, j: usize| i * (m + 1) + j;
    for i in 1..=n {
        for j in 1..=m {
            table[at(i, j)] = if expected[i - 1].1 == heard[j - 1].1 {
                table[at(i - 1, j - 1)] + 1
            } else {
                table[at(i - 1, j)].max(table[at(i, j - 1)])
            };
        }
    }
    if table[at(n, m)] as f64 / (n as f64) < 0.6 {
        return Err("Whisper alignment confidence is too low. Try a better multilingual model or correct the script.".into());
    }
    let mut matches = vec![Vec::new(); lines.len()];
    let (mut i, mut j) = (n, m);
    while i > 0 && j > 0 {
        if expected[i - 1].1 == heard[j - 1].1 {
            matches[expected[i - 1].0].push(heard[j - 1].0);
            i -= 1;
            j -= 1;
        } else if table[at(i - 1, j)] >= table[at(i, j - 1)] {
            i -= 1;
        } else {
            j -= 1;
        }
    }
    let mut spans = Vec::new();
    for (row, matched) in matches.iter_mut().enumerate() {
        matched.sort_unstable();
        let count = expected.iter().filter(|(r, _)| *r == row).count();
        if matched.is_empty() || (matched.len() as f64 / count.max(1) as f64) < 0.4 {
            return Err(format!(
                "Whisper could not align sentence {} reliably. Regenerate or shorten it.",
                row + 1
            ));
        }
        let start = words[matched[0]].start;
        let end = words[*matched.last().unwrap()].end;
        if !start.is_finite() || !end.is_finite() || end <= start {
            return Err("Invalid Whisper word timing".into());
        }
        spans.push((start, end));
    }
    // Include unmatched edge words within each sentence instead of dropping them.
    for row in 1..spans.len() {
        let boundary = (spans[row - 1].1 + spans[row].0) / 2.0;
        if spans[row - 1].1 > spans[row].0 {
            return Err("Whisper returned overlapping sentence timings. Retry alignment.".into());
        }
        spans[row - 1].1 = boundary;
        spans[row].0 = boundary;
    }
    if let Some(first) = spans.first_mut() {
        first.0 = 0.0;
    }
    Ok(spans)
}
/// Group Whisper words into editable sentences, retaining source timings.
pub fn sentences(words: &[Segment]) -> Vec<Segment> {
    let mut result = Vec::new();
    let mut current: Option<Segment> = None;
    for word in words {
        let cue = current.get_or_insert_with(|| Segment {
            start: word.start,
            end: word.end,
            text: String::new(),
        });
        if !cue.text.is_empty() {
            cue.text.push(' ');
        }
        cue.text.push_str(&word.text);
        cue.end = word.end;
        if word.text.ends_with(['.', '!', '?', '。', '！', '？']) || cue.end - cue.start >= 10.0
        {
            if let Some(cue) = current.take() {
                result.push(cue);
            }
        }
    }
    if let Some(cue) = current {
        result.push(cue);
    }
    result
}

/// Reject invalid, overlapping, or excessively long target slots.
pub fn validate(cues: &[Cue]) -> Result<(), String> {
    if cues.is_empty() {
        return Err("Load some sentences first".into());
    }
    for (i, cue) in cues.iter().enumerate() {
        if cue.text.trim().is_empty()
            || !cue.start.is_finite()
            || !cue.end.is_finite()
            || cue.start < 0.0
            || cue.end <= cue.start
        {
            return Err(format!(
                "Sentence {} needs text and valid start/end times",
                i + 1
            ));
        }
        if i > 0 && cue.start < cues[i - 1].end - 0.001 {
            return Err(format!("Sentence {} overlaps the previous sentence", i + 1));
        }
    }
    if cues.last().unwrap().end - cues[0].start > 1800.0 {
        return Err("Split dubbing into sections of at most 30 minutes".into());
    }
    Ok(())
}
fn tempo(mut rate: f64) -> Vec<String> {
    let mut out = Vec::new();
    while rate > 2.0 {
        out.push("atempo=2".into());
        rate /= 2.0;
    }
    while rate < 0.5 {
        out.push("atempo=0.5".into());
        rate /= 0.5;
    }
    out.push(format!("atempo={rate:.9}"));
    out
}
/// One-based sentence numbers that still need speech generation.
pub fn missing_audio(cues: &[Cue]) -> Vec<usize> {
    cues.iter()
        .enumerate()
        .filter_map(|(i, cue)| cue.audio.is_none().then_some(i + 1))
        .collect()
}
/// Render selected cues at their edited positions with pitch-preserving atempo, including gaps.
pub fn render(cues: &[Cue], project: &str, cancel: &AtomicBool) -> Result<PathBuf, String> {
    validate(cues)?;
    let missing = missing_audio(cues);
    if !missing.is_empty() {
        return Err(format!(
            "Sentences {} have no generated audio. Generate these sentences before previewing or applying.",
            missing
                .iter()
                .map(usize::to_string)
                .collect::<Vec<_>>()
                .join(", ")
        ));
    }
    const RATE: u32 = 24000;
    let origin = cues[0].start;
    let length = ((cues.last().unwrap().end - origin) * RATE as f64).ceil() as usize;
    let mut mix = vec![0.0f32; length];
    for (index, cue) in cues.iter().enumerate() {
        cloud_tts::check_cancel(cancel)?;
        let file = cue
            .audio
            .as_ref()
            .ok_or_else(|| format!("Sentence {} has no generated audio", index + 1))?;
        let duration = cue.audio_end - cue.audio_start;
        let target = cue.end - cue.start;
        if duration <= 0.0 || duration / target > 4.0 || duration / target < 0.25 {
            return Err("A sentence needs an extreme speed change. Adjust its timing or regenerate shorter text.".into());
        }
        let mut decoder = AudioDecoder::open(
            file,
            &AudioOptions {
                start: Some(cue.audio_start),
                duration: Some(duration),
                filters: tempo(duration / target),
                rate: RATE,
                channels: 1,
                format: SampleFormat::F32,
                stream: None,
            },
        )
        .map_err(|e| e.to_string())?;
        let samples = decoder.collect_f32().map_err(|e| e.to_string())?;
        if samples.is_empty() {
            return Err("Generated sentence has no audio".into());
        }
        let offset = ((cue.start - origin) * RATE as f64).round() as usize;
        let limit = (target * RATE as f64).round() as usize;
        // FFmpeg's atempo can be a few frames long; trim/pad the tail, never shift later sentences.
        for (dst, src) in mix.iter_mut().skip(offset).take(limit).zip(samples) {
            *dst = src;
        }
    }
    cloud_tts::check_cancel(cancel)?;
    let path = cloud_tts::output(project, "dub-preview")?;
    cloud_tts::write_wav(&path, &mix, RATE)?;
    Ok(path)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn missing_audio_identifies_original_sentence_numbers_before_decoding() {
        let cues = (0..4)
            .map(|i| Cue {
                text: format!("Sentence {i}"),
                start: i as f64,
                end: i as f64 + 1.0,
                audio: if i % 2 == 0 {
                    Some(PathBuf::from("not-opened.wav"))
                } else {
                    None
                },
                audio_start: 0.0,
                audio_end: 1.0,
            })
            .collect::<Vec<_>>();
        assert_eq!(missing_audio(&cues), vec![2, 4]);
        let error = render(&cues, "unused", &AtomicBool::new(false)).unwrap_err();
        assert!(error.contains("2, 4"), "{error}");
    }
    #[test]
    fn one_generated_sentence_can_preview_while_other_sentences_are_missing() {
        let root = std::env::temp_dir().join(format!("concat-dub-partial-{}", std::process::id()));
        std::fs::create_dir_all(&root).unwrap();
        let source = root.join("source.wav");
        cloud_tts::write_wav(&source, &vec![0.2f32; 4800], 24000).unwrap();
        let mut cues = vec![
            Cue {
                text: "Created".into(),
                start: 0.0,
                end: 0.2,
                audio: Some(source.clone()),
                audio_start: 0.0,
                audio_end: 0.2,
            },
            Cue {
                text: "Pending".into(),
                start: 0.3,
                end: 0.5,
                ..Cue::default()
            },
        ];
        let cancel = AtomicBool::new(false);
        assert_eq!(missing_audio(&cues), vec![2]);
        assert!(
            render(&cues[..1], &root.to_string_lossy(), &cancel)
                .unwrap()
                .is_file()
        );
        assert!(
            render(&cues, &root.to_string_lossy(), &cancel)
                .unwrap_err()
                .contains("2")
        );
        cues[1].audio = Some(source.clone());
        cues[1].audio_end = 0.2;
        assert!(missing_audio(&cues).is_empty());
        assert!(
            render(&cues, &root.to_string_lossy(), &cancel)
                .unwrap()
                .is_file()
        );
        // A later render/timing failure leaves the already generated audio usable.
        cues[1].audio_end = 5.0;
        assert!(render(&cues, &root.to_string_lossy(), &cancel).is_err());
        assert!(
            cues.iter()
                .all(|c| c.audio.as_ref().is_some_and(|p| p.is_file()))
        );
        assert!(render(&cues[..1], &root.to_string_lossy(), &cancel).is_ok());
    }
    #[test]
    fn aligns_spoken_words_without_counting_vocal_events() {
        let words = [
            Segment {
                text: "Xin".into(),
                start: 0.2,
                end: 0.5,
            },
            Segment {
                text: "chào".into(),
                start: 0.5,
                end: 0.9,
            },
        ];
        let spans = align(&["<laugh> Xin <short pause> chào <cry>".into()], &words).unwrap();
        assert_eq!(spans, vec![(0.0, 0.9)]);
        assert_eq!(without_vocal_tags("Xin<laugh>chào"), "Xin chào");
        assert_eq!(without_vocal_tags("a <unknown> b"), "a <unknown> b");
    }
    #[test]
    fn aligns_vietnamese_punctuation_and_repeated_words_in_order() {
        let words = [
            ("Xin", 0., 0.3),
            ("chào", 0.3, 0.6),
            ("bạn", 0.6, 0.9),
            ("Xin", 1., 1.3),
            ("chào", 1.3, 1.6),
        ]
        .map(|(t, s, e)| Segment {
            text: t.into(),
            start: s,
            end: e,
        });
        let spans = align(&["Xin chào bạn!".into(), "Xin chào.".into()], &words).unwrap();
        assert_eq!(spans.len(), 2);
        assert_eq!(spans[0].0, 0.0);
        assert_eq!(spans[1].1, 1.6);
        assert!(spans[0].1 <= spans[1].0);
        assert!(align(&["Hoàn toàn khác".into()], &words).is_err());
    }
    #[test]
    fn rejects_a_missing_third_sentence_even_when_the_track_matches_well() {
        let words = ["Xin", "chào", "bạn", "Hôm", "nay", "trời", "đẹp"]
            .into_iter()
            .enumerate()
            .map(|(i, text)| Segment {
                text: text.into(),
                start: i as f64 * 0.3,
                end: (i + 1) as f64 * 0.3,
            })
            .collect::<Vec<_>>();
        let error = align(
            &[
                "Xin chào bạn".into(),
                "Hôm nay trời đẹp".into(),
                "rất vui".into(),
            ],
            &words,
        )
        .unwrap_err();
        assert!(error.contains("sentence 3"), "{error}");
    }
    #[test]
    fn rejects_overlaps_and_non_finite_times() {
        let mut cues = vec![
            Cue {
                text: "one".into(),
                start: 1.,
                end: 3.,
                ..Cue::default()
            },
            Cue {
                text: "two".into(),
                start: 2.,
                end: 4.,
                ..Cue::default()
            },
        ];
        assert!(validate(&cues).is_err());
        cues[1].start = 3.;
        assert!(validate(&cues).is_ok());
        cues[0].start = f64::NAN;
        assert!(validate(&cues).is_err());
    }
    #[test]
    fn rendered_dub_preserves_target_slots_and_silence_gaps() {
        let root = std::env::temp_dir().join(format!("concat-dub-test-{}", std::process::id()));
        std::fs::create_dir_all(&root).unwrap();
        let source = root.join("tone.wav");
        let tone: Vec<f32> = (0..4800)
            .map(|i| ((i as f32 * 440.0 / 24000.0) * std::f32::consts::TAU).sin() * 0.3)
            .collect();
        cloud_tts::write_wav(&source, &tone, 24000).unwrap();
        let cues = vec![
            Cue {
                text: "first".into(),
                start: 10.0,
                end: 10.4,
                audio: Some(source.clone()),
                audio_start: 0.0,
                audio_end: 0.2,
            },
            Cue {
                text: "second".into(),
                start: 10.7,
                end: 10.9,
                audio: Some(source),
                audio_start: 0.0,
                audio_end: 0.2,
            },
        ];
        let cancel = AtomicBool::new(false);
        let out = render(&cues, &root.to_string_lossy(), &cancel).unwrap();
        let mut reader = AudioDecoder::open(
            &out,
            &AudioOptions {
                rate: 24000,
                channels: 1,
                format: SampleFormat::F32,
                ..AudioOptions::default()
            },
        )
        .unwrap();
        let samples = reader.collect_f32().unwrap();
        assert!((samples.len() as i64 - 21600).abs() < 8);
        assert!(samples[1000..4000].iter().any(|s| s.abs() > 0.1));
        assert!(samples[10000..16000].iter().all(|s| s.abs() < 0.0001));
        assert!(samples[17500..20500].iter().any(|s| s.abs() > 0.1));
        cancel.store(true, std::sync::atomic::Ordering::Relaxed);
        assert!(
            render(&cues, &root.to_string_lossy(), &cancel)
                .unwrap_err()
                .contains("cancelled")
        );
        drop(reader);
        std::fs::remove_dir_all(root).unwrap();
    }
    #[test]
    #[ignore = "downloads Kokoro and Whisper into CONCAT_DUB_TEST_DIR"]
    fn local_voice_whisper_alignment_roundtrip() {
        use crate::{
            transcribe::{TranscribeRequest, Transcriber},
            tts::{SpeakRequest, Speech},
        };
        let root =
            PathBuf::from(std::env::var("CONCAT_DUB_TEST_DIR").expect("set CONCAT_DUB_TEST_DIR"));
        std::fs::create_dir_all(&root).unwrap();
        let project_root = root.join("Roundtrip");
        if !concat_host::projects::is_project(&project_root) {
            concat_host::projects::create(
                &root.to_string_lossy(),
                "Roundtrip",
                160,
                90,
                25,
                1,
                [0, 0, 0, 255],
            )
            .unwrap();
        }
        let dirs = concat_host::AppDirs::under(&root);
        let speech = Speech::new();
        let transcriber = Transcriber::new();
        let model = "kokoro-int8-multi-lang-v1_0";
        speech.download_model(&dirs, model, |_| {}).unwrap();
        transcriber.download_model(&dirs, "tiny", |_| {}).unwrap();
        let lines = vec![
            "Hello everyone, welcome to our video.".to_owned(),
            "Today we are testing a new voice for this project.".to_owned(),
        ];
        let spoken = speech
            .speak(
                &dirs,
                &SpeakRequest {
                    model_id: model.into(),
                    voice: 3,
                    reference: None,
                    text: lines.join("\n"),
                    speed: 1.0,
                    pauses: Some(0.2),
                    steps: None,
                    project: project_root.to_string_lossy().into(),
                },
                |_| {},
            )
            .unwrap();
        let words = transcriber
            .transcribe_words(
                &dirs,
                &TranscribeRequest {
                    path: spoken.path.clone(),
                    audio_stream: None,
                    source_start: 0.0,
                    window: spoken.duration,
                    model_id: "tiny".into(),
                },
                |_| {},
            )
            .unwrap();
        eprintln!("Whisper words: {words:?}");
        let spans = align(&lines, &words).unwrap();
        assert_eq!(spans.len(), 2);
        assert!(spans[0].1 <= spans[1].0);
        assert!(spans[1].1 <= spoken.duration + 0.1);
        let cues = lines
            .into_iter()
            .zip(spans)
            .enumerate()
            .map(|(i, (text, (start, end)))| Cue {
                text,
                start: i as f64 * 5.0,
                end: (i + 1) as f64 * 5.0,
                audio: Some(PathBuf::from(&spoken.path)),
                audio_start: start,
                audio_end: end,
            })
            .collect::<Vec<_>>();
        let out = render(&cues, &root.to_string_lossy(), &AtomicBool::new(false)).unwrap();
        eprintln!("Local dubbing preview: {}", out.display());
    }
}
