// SPDX-License-Identifier: AGPL-3.0-or-later
//! Readable caption lines and bounded cues, independent of the recognition provider.
use crate::transcribe::Segment;
use unicode_segmentation::UnicodeSegmentation;

/// A conservative line length that also adapts to portrait frames and large type.
pub fn columns(width: u32, height: u32, font_size: f64) -> usize {
    ((width.max(1) as f64 / height.max(1) as f64 * 0.82 / (font_size.max(0.01) * 0.8)).floor()
        as usize)
        .clamp(12, 32)
}
fn g_len(text: &str) -> usize {
    text.graphemes(true).count()
}

fn is_clause_break(word: &str) -> bool {
    word.ends_with([',', ';', ':', '–', '—'])
}

fn is_sentence_break(word: &str) -> bool {
    word.ends_with(['.', '!', '?', '…', '。', '！', '？'])
}

fn is_conjunction(word: &str) -> bool {
    let lower = word
        .trim_matches(|c: char| !c.is_alphanumeric())
        .to_lowercase();
    matches!(
        lower.as_str(),
        "nhưng"
            | "mà"
            | "vì"
            | "nên"
            | "để"
            | "khi"
            | "nếu"
            | "hoặc"
            | "và"
            | "thì"
            | "rằng"
            | "bởi"
            | "tuy"
            | "còn"
            | "hay"
            | "and"
            | "but"
            | "or"
            | "so"
            | "because"
            | "when"
            | "if"
            | "while"
            | "although"
    )
}

fn is_unsplittable_tail(word: &str) -> bool {
    let lower = word
        .trim_matches(|c: char| !c.is_alphanumeric())
        .to_lowercase();
    matches!(
        lower.as_str(),
        "là"
            | "tên"
            | "anh"
            | "chị"
            | "em"
            | "ông"
            | "bà"
            | "cô"
            | "chú"
            | "bác"
            | "bạn"
            | "một"
            | "các"
            | "những"
            | "mọi"
            | "cái"
            | "con"
            | "người"
            | "của"
            | "cho"
            | "với"
            | "trong"
            | "tại"
            | "vào"
            | "đến"
            | "từ"
            | "đang"
            | "đã"
            | "sẽ"
            | "vừa"
            | "rất"
            | "quá"
            | "khá"
            | "không"
            | "chưa"
            | "chẳng"
            | "khoa"
            | "phòng"
            | "ban"
            | "bộ"
            | "sở"
            | "viện"
            | "trường"
            | "ủy"
    )
}

fn norm_word(word: &str) -> String {
    word.trim_matches(|c: char| !c.is_alphanumeric())
        .to_lowercase()
}

fn strip_pause_punct(word: &str) -> String {
    word.trim_end_matches(['.', ',', ';', ':', '…', '-', '–', '—', '!', '?', '。', '！', '？'])
        .to_owned()
}

fn is_reduplicative_word(norm: &str) -> bool {
    matches!(
        norm,
        "ngày"
            | "đêm"
            | "nhà"
            | "người"
            | "từ"
            | "dần"
            | "nhanh"
            | "chậm"
            | "xa"
            | "gần"
            | "luôn"
            | "đời"
            | "lớp"
            | "hàng"
            | "tầng"
            | "mãi"
            | "vừa"
            | "nhỏ"
            | "to"
            | "lớn"
            | "đỏ"
            | "xanh"
            | "trắng"
            | "đen"
            | "vàng"
            | "tím"
            | "hồng"
            | "nhè"
            | "nhẹ"
            | "ầm"
            | "ào"
            | "rầm"
            | "rào"
            | "lơ"
            | "mờ"
            | "sáng"
            | "tối"
            | "chiều"
            | "sớm"
            | "thường"
            | "đều"
            | "cùng"
            | "từng"
            | "mỗi"
            | "ai"
            | "gì"
            | "đâu"
            | "sao"
            | "thế"
            | "vậy"
            | "ừ"
            | "ờ"
            | "à"
            | "ơ"
            | "ơi"
    )
}

fn is_standalone_interjection(norm: &str) -> bool {
    matches!(
        norm,
        "vâng"
            | "dạ"
            | "đúng"
            | "không"
            | "được"
            | "rồi"
            | "có"
            | "chào"
            | "ok"
            | "yes"
            | "no"
            | "hello"
            | "hi"
            | "cảm"
            | "xin"
    )
}

fn is_abbreviation(word: &str) -> bool {
    let lower = word.trim().to_lowercase();
    matches!(
        lower.as_str(),
        "tp."
            | "ts."
            | "bs."
            | "gs."
            | "pgs."
            | "ths."
            | "ks."
            | "cn."
            | "mr."
            | "mrs."
            | "ms."
            | "dr."
            | "prof."
            | "st."
            | "q."
            | "p."
            | "tx."
            | "tt."
    )
}

/// Cleans immediate 1-2 word stutters (e.g. "Khoa, Khoa Phòng chống..." or "Khoa. Khoa Phòng chống...")
/// and removes false hesitation periods after a single initial word ("Khoa. Phòng chống...").
pub fn clean_segment_text(text: &str) -> String {
    let raw: Vec<&str> = text.split_whitespace().collect();
    if raw.len() <= 1 {
        return text.trim().to_owned();
    }
    let mut out: Vec<String> = Vec::with_capacity(raw.len());
    let mut i = 0;
    while i < raw.len() {
        // Check for a 2-word stumble repeat: e.g. "Tại khoa, tại khoa Phòng chống..."
        if i + 3 < raw.len() {
            let w0 = norm_word(raw[i]);
            let w1 = norm_word(raw[i + 1]);
            let w2 = norm_word(raw[i + 2]);
            let w3 = norm_word(raw[i + 3]);
            if !w0.is_empty() && !w1.is_empty() && w0 == w2 && w1 == w3 {
                i += 2;
                continue;
            }
        }
        // Check for a 1-word stumble repeat: e.g. "Khoa, Khoa Phòng..." or "Khoa. Khoa Phòng..." or "Khoa Khoa Phòng..."
        if i + 1 < raw.len() {
            let t0 = raw[i];
            let t1 = raw[i + 1];
            let w0 = norm_word(t0);
            let w1 = norm_word(t1);
            if !w0.is_empty() && w0 == w1 {
                let has_pause_punct =
                    t0.ends_with(['.', ',', ';', ':', '…', '-', '–', '—', '!', '?']);
                let at_clause_start = out.is_empty()
                    || out
                        .last()
                        .is_some_and(|prev| is_sentence_break(prev) || is_clause_break(prev));
                if has_pause_punct || (at_clause_start && !is_reduplicative_word(&w0)) {
                    i += 1;
                    continue;
                }
            }
        }
        let mut token = raw[i].to_owned();
        // If the very first word of a multi-word phrase has a false hesitation period (e.g. "Khoa. Phòng chống..."),
        // strip the period so it doesn't break the sentence after 1 word.
        let at_sentence_start =
            out.is_empty() || out.last().is_some_and(|prev| is_sentence_break(prev));
        if at_sentence_start
            && i + 1 < raw.len()
            && is_sentence_break(&token)
            && !is_abbreviation(&token)
        {
            let w = norm_word(&token);
            if is_unsplittable_tail(&w) || !is_standalone_interjection(&w) {
                let stripped = strip_pause_punct(&token);
                if !stripped.is_empty() {
                    token = stripped;
                }
            }
        }
        out.push(token);
        i += 1;
    }
    out.join(" ")
}

/// Merges 1-2 word speaker stumbles/hesitations (such as a standalone "Khoa" before
/// "Khoa Phòng chống bệnh truyền nhiễm" or "Phòng chống bệnh truyền nhiễm") across
/// adjacent segments and cleans up stuttered repeats.
pub fn clean_stumbles_and_orphans(segments: Vec<Segment>) -> Vec<Segment> {
    let mut segs: Vec<Segment> = segments
        .into_iter()
        .filter(|s| {
            s.start.is_finite()
                && s.end.is_finite()
                && s.end > s.start
                && !s.text.trim().is_empty()
        })
        .collect();
    if segs.is_empty() {
        return segs;
    }

    // Pass 1: If segment `i` ends with a 1-word orphan after a sentence break (e.g. "... câu trước. Khoa")
    // and segment `i + 1` follows within 2.5s, move that trailing word to the start of segment `i + 1`.
    for i in 0..segs.len().saturating_sub(1) {
        let gap = segs[i + 1].start - segs[i].end;
        if gap > 2.5 {
            continue;
        }
        let words: Vec<&str> = segs[i].text.split_whitespace().collect();
        if words.len() >= 4 && is_sentence_break(words[words.len() - 2]) {
            let tail_word = strip_pause_punct(words[words.len() - 1]);
            let head_text = words[..words.len() - 1].join(" ");
            if !tail_word.is_empty() && !head_text.is_empty() {
                let total_chars = g_len(&segs[i].text).max(1) as f64;
                let tail_chars = g_len(&tail_word).max(1) as f64;
                let dur = segs[i].end - segs[i].start;
                let shift = (dur * (tail_chars / total_chars)).clamp(0.05, (dur * 0.35).max(0.05));
                let split_at = (segs[i].end - shift).max(segs[i].start + 0.05);
                segs[i].text = head_text;
                segs[i].end = split_at;
                segs[i + 1].start = segs[i + 1].start.min(split_at);
                segs[i + 1].text = format!("{tail_word} {}", segs[i + 1].text.trim());
            }
        }
    }

    // Pass 2: Merge 1-2 word stumble/orphan segments with their following (or preceding) segment.
    let mut merged: Vec<Segment> = Vec::with_capacity(segs.len());
    let mut i = 0;
    while i < segs.len() {
        let curr = &segs[i];
        let curr_words: Vec<&str> = curr.text.split_whitespace().collect();
        let curr_norms: Vec<String> = curr_words
            .iter()
            .map(|w| norm_word(w))
            .filter(|w| !w.is_empty())
            .collect();

        if (1..=2).contains(&curr_norms.len()) && i + 1 < segs.len() {
            let gap = segs[i + 1].start - curr.end;
            let next_words: Vec<&str> = segs[i + 1].text.split_whitespace().collect();
            let next_norms: Vec<String> = next_words
                .iter()
                .map(|w| norm_word(w))
                .filter(|w| !w.is_empty())
                .collect();

            // Case A: Stuttered prefix repeat across segments (e.g. Seg i = "Khoa", Seg i+1 = "Khoa Phòng chống...")
            if gap <= 3.0
                && next_norms.len() >= curr_norms.len()
                && next_norms[..curr_norms.len()] == curr_norms[..]
            {
                segs[i + 1].start = curr.start;
                i += 1;
                continue;
            }

            // Case B: 1-word hesitation segment (e.g. Seg i = "Khoa." or "Khoa", Seg i+1 = "Phòng chống...")
            // or 2-word incomplete prefix (ends with unsplittable tail or conjunction, or has no sentence break).
            let should_merge_forward = gap <= 2.5
                && (curr_norms.len() == 1
                    || is_unsplittable_tail(curr_norms.last().unwrap())
                    || is_conjunction(curr_norms.last().unwrap())
                    || !is_sentence_break(curr_words.last().unwrap()));
            if should_merge_forward {
                let mut prefix_words: Vec<String> =
                    curr_words.iter().map(|&w| w.to_owned()).collect();
                if let Some(last) = prefix_words.last_mut() {
                    let stripped = strip_pause_punct(last);
                    if !stripped.is_empty() {
                        *last = stripped;
                    }
                }
                let prefix = prefix_words.join(" ");
                segs[i + 1].start = curr.start;
                segs[i + 1].text = format!("{prefix} {}", segs[i + 1].text.trim());
                i += 1;
                continue;
            }
        }

        // Case C: A trailing 1-word orphan segment at the end (or before a long pause) merges into the previous segment.
        if curr_norms.len() == 1
            && let Some(prev) = merged.last_mut()
            && curr.start - prev.end <= 2.0
        {
            let prev_trimmed = strip_pause_punct(&prev.text);
            if !prev_trimmed.is_empty() {
                prev.text = format!("{prev_trimmed} {}", curr.text.trim());
            } else {
                prev.text = format!("{} {}", prev.text.trim(), curr.text.trim());
            }
            prev.end = curr.end;
            i += 1;
            continue;
        }

        merged.push(curr.clone());
        i += 1;
    }

    // Pass 3: Clean internal stutters and false initial periods in each segment.
    for seg in &mut merged {
        seg.text = clean_segment_text(&seg.text);
    }
    merged.retain(|s| !s.text.trim().is_empty() && s.end > s.start);
    merged
}

/// Tokenize text so that no single token exceeds `columns` graphemes.
fn tokenize(text: &str, columns: usize) -> Vec<String> {
    let cap = columns.max(1);
    let mut tokens = Vec::new();
    for word in text.split_whitespace() {
        let graphemes = word.graphemes(true).collect::<Vec<_>>();
        if graphemes.len() <= cap {
            tokens.push(word.to_owned());
        } else {
            for part in graphemes.chunks(cap) {
                tokens.push(part.concat());
            }
        }
    }
    tokens
}

/// Try to format `words` into 1 or 2 balanced lines where each line is at most `columns` graphemes.
fn try_pack_two_lines(words: &[String], columns: usize) -> Option<String> {
    if words.is_empty() {
        return None;
    }
    let single = words.join(" ");
    let total = g_len(&single);
    if total <= columns {
        return Some(single);
    }
    if words.len() < 2 {
        return None;
    }
    let mut best: Option<(i64, String)> = None;
    for k in 1..words.len() {
        let line1 = words[..k].join(" ");
        let l1 = g_len(&line1);
        if l1 > columns {
            break;
        }
        let line2 = words[k..].join(" ");
        let l2 = g_len(&line2);
        if l2 > columns {
            continue;
        }
        let mut score = (l1.abs_diff(l2) as i64) * 10;
        if is_clause_break(&words[k - 1]) || is_sentence_break(&words[k - 1]) {
            score -= 80;
        } else if is_conjunction(&words[k]) {
            score -= 40;
        }
        if is_unsplittable_tail(&words[k - 1]) {
            score += 120;
        }
        if k == words.len() - 1 && words.len() >= 4 {
            score += 150;
        }
        let candidate = format!("{line1}\n{line2}");
        if best.as_ref().is_none_or(|(best_score, _)| score < *best_score) {
            best = Some((score, candidate));
        }
    }
    best.map(|(_, text)| text)
}

/// Split a sentence's tokens into balanced 1–2 line subtitle cues without orphan 1-word cues.
fn split_sentence_cues(words: &[String], columns: usize) -> Vec<String> {
    if words.is_empty() {
        return Vec::new();
    }
    if let Some(packed) = try_pack_two_lines(words, columns) {
        return vec![packed];
    }
    let n = words.len();
    let word_lens: Vec<usize> = words.iter().map(|w| g_len(w)).collect();
    let total_chars: usize = word_lens.iter().sum::<usize>() + n.saturating_sub(1);
    let max_cue_chars = (columns * 2).max(1);
    let est_cues = total_chars.div_ceil(max_cue_chars).max(2);
    let target_chars = (total_chars / est_cues).max(columns / 2);

    let mut dp = vec![f64::INFINITY; n + 1];
    let mut prev = vec![0usize; n + 1];
    let mut packed_cache: Vec<Vec<Option<String>>> = vec![vec![None; n + 1]; n];
    dp[0] = 0.0;

    for i in 0..n {
        if !dp[i].is_finite() {
            continue;
        }
        for j in (i + 1)..=n {
            let Some(packed) = try_pack_two_lines(&words[i..j], columns) else {
                // Once a slice exceeds 2 * columns + columns, longer slices won't fit either.
                let slice_chars: usize =
                    word_lens[i..j].iter().sum::<usize>() + (j - i).saturating_sub(1);
                if slice_chars > columns * 2 + 2 {
                    break;
                }
                continue;
            };
            let count = j - i;
            let slice_chars: usize = word_lens[i..j].iter().sum::<usize>() + count.saturating_sub(1);
            let mut cost = 1000.0;
            // Strongly penalize 1-word and 2-word orphan cues (such as a lone "Khoa").
            if count == 1 && n > 1 {
                cost += 50_000.0;
            } else if count == 2 && n >= 5 {
                cost += 5_000.0;
            }
            let diff = slice_chars as f64 - target_chars as f64;
            cost += (diff * diff) * 0.8;

            if j < n {
                if is_sentence_break(&words[j - 1]) {
                    cost -= 500.0;
                } else if is_clause_break(&words[j - 1]) {
                    cost -= 300.0;
                } else if is_conjunction(&words[j]) {
                    cost -= 150.0;
                }
                if is_unsplittable_tail(&words[j - 1]) {
                    cost += 350.0;
                }
            }
            let next_cost = dp[i] + cost;
            if next_cost < dp[j] {
                dp[j] = next_cost;
                prev[j] = i;
            }
            packed_cache[i][j] = Some(packed);
        }
    }

    if !dp[n].is_finite() {
        // Fallback if something couldn't pack: chunk greedily.
        let mut out = Vec::new();
        let mut start = 0;
        while start < n {
            let mut end = start + 1;
            let mut last_packed = words[start].clone();
            while end <= n {
                if let Some(p) = try_pack_two_lines(&words[start..end], columns) {
                    last_packed = p;
                    end += 1;
                } else {
                    break;
                }
            }
            out.push(last_packed);
            start = (end - 1).max(start + 1);
        }
        return out;
    }

    let mut cues = Vec::new();
    let mut cur = n;
    while cur > 0 {
        let p = prev[cur];
        let text = packed_cache[p][cur]
            .take()
            .unwrap_or_else(|| words[p..cur].join(" "));
        cues.push(text);
        cur = p;
    }
    cues.reverse();
    cues
}

/// Group tokens into sentences first, keeping short `< 3` word fragments attached so
/// single-word names or particles are never isolated.
fn cues_for_segment(text: &str, columns: usize) -> Vec<String> {
    let tokens = tokenize(text, columns);
    if tokens.is_empty() {
        return Vec::new();
    }
    let mut sentences: Vec<Vec<String>> = Vec::new();
    let mut current: Vec<String> = Vec::new();
    for (idx, token) in tokens.iter().enumerate() {
        current.push(token.clone());
        let remaining = tokens.len() - (idx + 1);
        if is_sentence_break(token) && current.len() >= 3 && remaining >= 3 {
            sentences.push(std::mem::take(&mut current));
        }
    }
    if !current.is_empty() {
        if current.len() < 3
            && let Some(last) = sentences.last_mut()
        {
            last.extend(current);
        } else {
            sentences.push(current);
        }
    }
    let mut groups = Vec::new();
    for sentence in sentences {
        groups.extend(split_sentence_cues(&sentence, columns));
    }
    groups
}

/// Split long recognition results into at most two lines per cue, preserving their time window.
/// Where the provider supplies only a segment time, sub-cue boundaries are proportional estimates.
pub fn readable(segments: Vec<Segment>, columns: usize) -> Vec<Segment> {
    let segments = clean_stumbles_and_orphans(segments);
    let mut result = Vec::new();
    for segment in segments {
        if !segment.start.is_finite() || !segment.end.is_finite() || segment.end <= segment.start {
            continue;
        }
        let groups = cues_for_segment(&segment.text, columns);
        if groups.is_empty() {
            continue;
        }
        let weights = groups
            .iter()
            .map(|text| text.graphemes(true).count().max(1))
            .collect::<Vec<_>>();
        let total = weights.iter().sum::<usize>().max(1) as f64;
        let mut used = 0;
        let mut start = segment.start;
        for (i, (text, weight)) in groups.iter().zip(weights.iter()).enumerate() {
            used += weight;
            let end = if i + 1 == groups.len() {
                segment.end
            } else {
                segment.start + (segment.end - segment.start) * used as f64 / total
            };
            result.push(Segment {
                start,
                end,
                text: text.clone(),
            });
            start = end;
        }
    }
    result
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn vietnamese_text_wraps_and_splits_without_extending_timing() {
        let text = "Đây là đoạn phụ đề tiếng Việt khá dài cần được chia thành các dòng ngắn để người xem có thể đọc dễ dàng mà không bị tràn ra khỏi khung hình của video.";
        let parts = readable(
            vec![Segment {
                start: 4.0,
                end: 10.0,
                text: text.into(),
            }],
            32,
        );
        assert!(parts.len() > 1);
        assert_eq!(parts.first().unwrap().start, 4.0);
        assert_eq!(parts.last().unwrap().end, 10.0);
        for p in &parts {
            assert!(p.text.lines().count() <= 2);
            assert!(
                p.text
                    .lines()
                    .all(|line| line.graphemes(true).count() <= 32)
            );
        }
        for pair in parts.windows(2) {
            assert_eq!(pair[0].end, pair[1].start);
        }
        assert_eq!(
            parts
                .iter()
                .map(|p| p.text.split_whitespace().collect::<Vec<_>>().join(" "))
                .collect::<Vec<_>>()
                .join(" "),
            text
        );
    }
    #[test]
    fn portrait_and_combining_accents_are_kept_readable() {
        assert!(columns(1080, 1920, 0.05) < columns(1920, 1080, 0.05));
        let text = "Vie\u{0323}\u{0302}t".repeat(20);
        let parts = readable(
            vec![Segment {
                start: 0.0,
                end: 6.0,
                text: text.clone(),
            }],
            12,
        );
        assert_eq!(
            parts
                .iter()
                .flat_map(|p| p.text.split_whitespace())
                .collect::<String>(),
            text
        );
        assert!(parts.iter().all(|p| {
            p.text
                .lines()
                .all(|line| line.graphemes(true).count() <= 12)
        }));
    }

    #[test]
    fn stumbled_single_word_khoa_merges_with_following_sentence() {
        // Case 1: Speaker paused after "Khoa" so ASR created a separate "Khoa." segment.
        let parts = readable(
            vec![
                Segment {
                    start: 10.0,
                    end: 10.5,
                    text: "Khoa.".into(),
                },
                Segment {
                    start: 10.7,
                    end: 13.5,
                    text: "Phòng chống bệnh truyền nhiễm.".into(),
                },
            ],
            32,
        );
        assert_eq!(parts.len(), 1);
        assert_eq!(parts[0].start, 10.0);
        assert_eq!(parts[0].end, 13.5);
        assert_eq!(
            parts[0].text.replace('\n', " "),
            "Khoa Phòng chống bệnh truyền nhiễm."
        );

        // Case 2: Speaker stumbled and repeated "Khoa" across two segments.
        let parts_repeat = readable(
            vec![
                Segment {
                    start: 10.0,
                    end: 10.4,
                    text: "Khoa".into(),
                },
                Segment {
                    start: 10.6,
                    end: 13.5,
                    text: "Khoa Phòng chống bệnh truyền nhiễm.".into(),
                },
            ],
            32,
        );
        assert_eq!(parts_repeat.len(), 1);
        assert_eq!(parts_repeat[0].start, 10.0);
        assert_eq!(parts_repeat[0].end, 13.5);
        assert_eq!(
            parts_repeat[0].text.replace('\n', " "),
            "Khoa Phòng chống bệnh truyền nhiễm."
        );

        // Case 3: Speaker stumbled within the same segment ("Khoa, Khoa Phòng chống...").
        let parts_inline = readable(
            vec![Segment {
                start: 10.0,
                end: 13.5,
                text: "Khoa, Khoa Phòng chống bệnh truyền nhiễm.".into(),
            }],
            32,
        );
        assert_eq!(parts_inline.len(), 1);
        assert_eq!(
            parts_inline[0].text.replace('\n', " "),
            "Khoa Phòng chống bệnh truyền nhiễm."
        );
    }
}
