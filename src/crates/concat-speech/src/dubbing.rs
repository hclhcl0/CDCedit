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

fn fold_char(c: char) -> char {
    match c {
        'à' | 'á' | 'ả' | 'ã' | 'ạ' | 'ă' | 'ằ' | 'ắ' | 'ẳ' | 'ẵ' | 'ặ' | 'â' | 'ầ' | 'ấ'
        | 'ẩ' | 'ẫ' | 'ậ' => 'a',
        'đ' => 'd',
        'è' | 'é' | 'ẻ' | 'ẽ' | 'ẹ' | 'ê' | 'ề' | 'ế' | 'ể' | 'ễ' | 'ệ' => 'e',
        'ì' | 'í' | 'ỉ' | 'ĩ' | 'ị' => 'i',
        'ò' | 'ó' | 'ỏ' | 'õ' | 'ọ' | 'ô' | 'ồ' | 'ố' | 'ổ' | 'ỗ' | 'ộ' | 'ơ' | 'ờ' | 'ớ'
        | 'ở' | 'ỡ' | 'ợ' => 'o',
        'ù' | 'ú' | 'ủ' | 'ũ' | 'ụ' | 'ư' | 'ừ' | 'ứ' | 'ử' | 'ữ' | 'ự' => 'u',
        'ỳ' | 'ý' | 'ỷ' | 'ỹ' | 'ỵ' => 'y',
        other => other,
    }
}

fn folded(text: &str) -> String {
    normalized(text).chars().map(fold_char).collect()
}

fn char_edit_distance(a: &str, b: &str) -> usize {
    let ac: Vec<char> = a.chars().collect();
    let bc: Vec<char> = b.chars().collect();
    let (n, m) = (ac.len(), bc.len());
    if n.abs_diff(m) > 2 {
        return n.abs_diff(m);
    }
    let mut prev: Vec<usize> = (0..=m).collect();
    let mut curr = vec![0; m + 1];
    for i in 1..=n {
        curr[0] = i;
        for j in 1..=m {
            let cost = usize::from(ac[i - 1] != bc[j - 1]);
            curr[j] = (prev[j] + 1).min(curr[j - 1] + 1).min(prev[j - 1] + cost);
        }
        std::mem::swap(&mut prev, &mut curr);
    }
    prev[m]
}

fn words_match(a: &str, b: &str) -> bool {
    if a == b {
        return true;
    }
    let fa = folded(a);
    let fb = folded(b);
    if !fa.is_empty() && fa == fb {
        return true;
    }
    let la = fa.chars().count();
    let lb = fb.chars().count();
    la >= 4 && lb >= 4 && char_edit_distance(&fa, &fb) <= 1
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
            table[at(i, j)] = if words_match(&expected[i - 1].1, &heard[j - 1].1) {
                table[at(i - 1, j - 1)] + 1
            } else {
                table[at(i - 1, j)].max(table[at(i, j - 1)])
            };
        }
    }
    if table[at(n, m)] as f64 / (n as f64) < 0.55 {
        return Err("Whisper alignment confidence is too low. Try a better multilingual model or correct the script.".into());
    }
    let mut matches = vec![Vec::new(); lines.len()];
    let (mut i, mut j) = (n, m);
    while i > 0 && j > 0 {
        if words_match(&expected[i - 1].1, &heard[j - 1].1)
            && table[at(i, j)] == table[at(i - 1, j - 1)] + 1
        {
            matches[expected[i - 1].0].push(heard[j - 1].0);
            i -= 1;
            j -= 1;
        } else if table[at(i - 1, j)] >= table[at(i, j - 1)] {
            i -= 1;
        } else {
            j -= 1;
        }
    }
    for matched in &mut matches {
        matched.sort_unstable();
        matched.dedup();
    }
    // Recover unclaimed words in gaps for short or misrecognized sentences (e.g. a 1-word cue like "Khoa").
    let mut claimed = vec![false; words.len()];
    for matched in &matches {
        for &w_idx in matched {
            if w_idx < claimed.len() {
                claimed[w_idx] = true;
            }
        }
    }
    let row_counts: Vec<usize> = (0..lines.len())
        .map(|row| expected.iter().filter(|(r, _)| *r == row).count())
        .collect();
    for row in 0..lines.len() {
        let count = row_counts[row];
        if !matches[row].is_empty() && (matches[row].len() as f64 / count.max(1) as f64) >= 0.4 {
            continue;
        }
        let prev_word = (0..row)
            .rev()
            .find_map(|r| matches[r].last().copied());
        let next_word = ((row + 1)..lines.len())
            .find_map(|r| matches[r].first().copied());
        // Only recover unclaimed words when bounded by neighboring sentences or for single-word sentences.
        if (prev_word.is_some() && next_word.is_some()) || count <= 1 {
            let gap_words: Vec<usize> = heard
                .iter()
                .map(|&(w_idx, _)| w_idx)
                .filter(|&w_idx| {
                    !claimed[w_idx]
                        && prev_word.map_or(true, |p| w_idx > p)
                        && next_word.map_or(true, |nx| w_idx < nx)
                })
                .take(count.max(1))
                .collect();
            for w_idx in gap_words {
                claimed[w_idx] = true;
                matches[row].push(w_idx);
            }
            matches[row].sort_unstable();
        }
    }
    let mut raw_spans: Vec<Option<(f64, f64)>> = Vec::with_capacity(lines.len());
    for (row, matched) in matches.iter().enumerate() {
        let count = row_counts[row];
        if !matched.is_empty() && (matched.len() as f64 / count.max(1) as f64) >= 0.4 {
            let start = words[matched[0]].start;
            let end = words[*matched.last().unwrap()].end;
            if !start.is_finite() || !end.is_finite() || end <= start {
                return Err("Invalid Whisper word timing".into());
            }
            raw_spans.push(Some((start, end)));
        } else if count <= 1 {
            // Single-word or vocal-tag-only sentence where Whisper skipped or merged the word:
            // interpolate its timing span from neighbors below instead of failing the dub.
            raw_spans.push(None);
        } else {
            return Err(format!(
                "Whisper could not align sentence {} reliably. Regenerate or shorten it.",
                row + 1
            ));
        }
    }
    let mut spans = vec![(0.0, 0.0); lines.len()];
    for row in 0..lines.len() {
        if let Some(span) = raw_spans[row] {
            spans[row] = span;
        }
    }
    for row in 0..lines.len() {
        if raw_spans[row].is_some() {
            continue;
        }
        let prev = (0..row).rev().find_map(|r| raw_spans[r].map(|s| (r, s)));
        let next = ((row + 1)..lines.len()).find_map(|r| raw_spans[r].map(|s| (r, s)));
        let span = match (prev, next) {
            (Some((pr, (ps, pe))), Some((nr, (ns, ne)))) => {
                if ns > pe + 0.04 {
                    (pe, ns)
                } else {
                    let mid = (pe + ns) / 2.0;
                    let half = 0.12_f64
                        .min((pe - ps) * 0.25)
                        .min((ne - ns) * 0.25)
                        .max(0.02);
                    spans[pr].1 = (mid - half).max(spans[pr].0 + 0.02);
                    spans[nr].0 = (mid + half).min(spans[nr].1 - 0.02);
                    raw_spans[pr] = Some(spans[pr]);
                    raw_spans[nr] = Some(spans[nr]);
                    (spans[pr].1, spans[nr].0)
                }
            }
            (Some((pr, (ps, pe))), None) => {
                let tail = words.last().map(|w| w.end).unwrap_or(pe);
                if tail > pe + 0.04 {
                    (pe, tail)
                } else {
                    let w = 0.2_f64.min((pe - ps) * 0.25).max(0.02);
                    spans[pr].1 = (pe - w).max(ps + 0.02);
                    raw_spans[pr] = Some(spans[pr]);
                    (spans[pr].1, pe)
                }
            }
            (None, Some((nr, (ns, ne)))) => {
                if ns > 0.04 {
                    (0.0, ns)
                } else {
                    let w = 0.2_f64.min((ne - ns) * 0.25).max(0.02);
                    spans[nr].0 = (ns + w).min(ne - 0.02);
                    raw_spans[nr] = Some(spans[nr]);
                    (ns, spans[nr].0)
                }
            }
            (None, None) => {
                let start = words.first().map(|w| w.start).unwrap_or(0.0);
                let end = words.last().map(|w| w.end).unwrap_or(start + 0.2);
                (start, end.max(start + 0.05))
            }
        };
        spans[row] = span;
        raw_spans[row] = Some(span);
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
    let mut word_count = 0usize;
    for (idx, word) in words.iter().enumerate() {
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
        word_count += 1;
        let next_gap = words
            .get(idx + 1)
            .map(|next| next.start - word.end)
            .unwrap_or(f64::INFINITY);
        let ends_punct = word.text.ends_with(['.', '!', '?', '。', '！', '？']);
        if (ends_punct && (word_count >= 3 || next_gap > 2.5)) || cue.end - cue.start >= 10.0 {
            if let Some(cue) = current.take() {
                result.push(cue);
                word_count = 0;
            }
        } else if ends_punct && word_count <= 2 && next_gap <= 2.5 {
            while cue
                .text
                .ends_with(['.', ',', ';', ':', '…', '!', '?', '。', '！', '？'])
            {
                cue.text.pop();
            }
        }
    }
    if let Some(cue) = current {
        result.push(cue);
    }
    crate::caption_layout::clean_stumbles_and_orphans(result)
}

/// Reject invalid or excessively long target slots.
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
    }
    if let (Some(first), Some(last)) = (cues.first(), cues.last()) {
        if last.end - first.start > 1800.0 {
            return Err("Split dubbing into sections of at most 30 minutes".into());
        }
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
    if cues.is_empty() {
        return Err("No sentences to render".into());
    }
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
    let mut sorted_cues = cues.to_vec();
    sorted_cues.sort_by(|a, b| a.start.total_cmp(&b.start));

    const RATE: u32 = 24000;
    let origin = sorted_cues.iter().map(|c| c.start).fold(f64::INFINITY, f64::min).max(0.0);
    let mut max_end = sorted_cues.iter().map(|c| c.end).fold(origin + 0.1, f64::max);
    for cue in &sorted_cues {
        let dur = (cue.audio_end - cue.audio_start).max(0.0);
        max_end = max_end.max(cue.start + dur);
    }
    let length = (((max_end - origin) * RATE as f64).ceil() as usize).max(RATE as usize);
    let mut mix = vec![0.0f32; length];

    for (index, cue) in sorted_cues.iter().enumerate() {
        cloud_tts::check_cancel(cancel)?;
        let file = cue
            .audio
            .as_ref()
            .ok_or_else(|| format!("Sentence {} has no generated audio", index + 1))?;
        let mut duration = (cue.audio_end - cue.audio_start).max(0.0);
        let target = (cue.end - cue.start).max(0.05);

        if duration <= 0.001 {
            if let Ok(info) = concat_media::probe(file) {
                duration = info.duration.map(|d| d.as_f64()).unwrap_or(target).max(0.1);
            } else {
                duration = target;
            }
        }

        // Clamp rate to 0.5..2.5 so speech stays comprehensible and does not fail
        let rate = (duration / target).clamp(0.5, 2.5);

        let samples = AudioDecoder::open(
            file,
            &AudioOptions {
                start: Some(cue.audio_start),
                duration: Some(duration),
                filters: tempo(rate),
                rate: RATE,
                channels: 1,
                format: SampleFormat::F32,
                stream: None,
            },
        )
        .and_then(|mut d| d.collect_f32())
        .ok()
        .filter(|s| !s.is_empty())
        .or_else(|| {
            AudioDecoder::open(
                file,
                &AudioOptions {
                    start: Some(cue.audio_start),
                    duration: Some(duration),
                    filters: vec![],
                    rate: RATE,
                    channels: 1,
                    format: SampleFormat::F32,
                    stream: None,
                },
            )
            .and_then(|mut d| d.collect_f32())
            .ok()
        })
        .unwrap_or_default();

        if samples.is_empty() {
            continue;
        }

        let offset = (((cue.start - origin).max(0.0)) * RATE as f64).round() as usize;
        let limit = samples.len().min(mix.len().saturating_sub(offset));
        for (dst, src) in mix.iter_mut().skip(offset).take(limit).zip(samples) {
            *dst = (*dst + src).clamp(-1.0, 1.0);
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
    fn aligns_single_word_sentence_like_khoa_with_tone_mismatch_or_gap() {
        // Case 1: Whisper heard "Khóa" (with acute accent) or "Qua" instead of 1-word "Khoa"
        let words_toned = [
            ("Xin", 0.0, 0.3),
            ("chào", 0.3, 0.6),
            ("bạn", 0.6, 0.9),
            ("Khóa", 1.0, 1.3),
            ("Hôm", 1.4, 1.7),
            ("nay", 1.7, 2.0),
            ("trời", 2.0, 2.3),
            ("đẹp", 2.3, 2.6),
        ]
        .map(|(t, s, e)| Segment {
            text: t.into(),
            start: s,
            end: e,
        });
        let lines = vec![
            "Xin chào bạn".into(),
            "Khoa".into(),
            "Hôm nay trời đẹp".into(),
        ];
        let spans = align(&lines, &words_toned).unwrap();
        assert_eq!(spans.len(), 3);
        assert!(spans[0].1 <= spans[1].0 && spans[1].0 < spans[1].1 && spans[1].1 <= spans[2].0);

        // Case 2: Whisper transcribed an unrelated word ("À") in the gap for "Khoa"
        let words_gap = [
            ("Xin", 0.0, 0.3),
            ("chào", 0.3, 0.6),
            ("bạn", 0.6, 0.9),
            ("À", 1.0, 1.3),
            ("Hôm", 1.4, 1.7),
            ("nay", 1.7, 2.0),
            ("trời", 2.0, 2.3),
            ("đẹp", 2.3, 2.6),
        ]
        .map(|(t, s, e)| Segment {
            text: t.into(),
            start: s,
            end: e,
        });
        let spans_gap = align(&lines, &words_gap).unwrap();
        assert_eq!(spans_gap.len(), 3);
        assert!(spans_gap[1].0 < spans_gap[1].1);

        // Case 3: Whisper completely skipped the 1-word utterance "Khoa"
        let words_skipped = [
            ("Xin", 0.0, 0.3),
            ("chào", 0.3, 0.6),
            ("bạn", 0.6, 0.9),
            ("Hôm", 1.4, 1.7),
            ("nay", 1.7, 2.0),
            ("trời", 2.0, 2.3),
            ("đẹp", 2.3, 2.6),
        ]
        .map(|(t, s, e)| Segment {
            text: t.into(),
            start: s,
            end: e,
        });
        let spans_skipped = align(&lines, &words_skipped).unwrap();
        assert_eq!(spans_skipped.len(), 3);
        assert!(spans_skipped[0].1 <= spans_skipped[1].0);
        assert!(spans_skipped[1].0 < spans_skipped[1].1);
        assert!(spans_skipped[1].1 <= spans_skipped[2].0);
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

    #[test]
    fn sentences_merges_stumbled_single_word_khoa_into_full_sentence() {
        let words = vec![
            Segment {
                start: 10.0,
                end: 10.4,
                text: "Khoa.".into(),
            },
            Segment {
                start: 10.6,
                end: 10.9,
                text: "Khoa".into(),
            },
            Segment {
                start: 10.9,
                end: 11.2,
                text: "Phòng".into(),
            },
            Segment {
                start: 11.2,
                end: 11.5,
                text: "chống".into(),
            },
            Segment {
                start: 11.5,
                end: 11.8,
                text: "bệnh".into(),
            },
            Segment {
                start: 11.8,
                end: 12.1,
                text: "truyền".into(),
            },
            Segment {
                start: 12.1,
                end: 12.6,
                text: "nhiễm.".into(),
            },
        ];
        let grouped = sentences(&words);
        assert_eq!(grouped.len(), 1);
        assert_eq!(grouped[0].text, "Khoa Phòng chống bệnh truyền nhiễm.");
        assert_eq!((grouped[0].start, grouped[0].end), (10.0, 12.6));
    }
}
