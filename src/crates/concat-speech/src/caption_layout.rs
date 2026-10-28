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
fn lines(text: &str, columns: usize) -> Vec<String> {
    let mut result = Vec::new();
    let mut line = String::new();
    let mut length = 0;
    for word in text.split_whitespace() {
        let graphemes = word.graphemes(true).collect::<Vec<_>>();
        for part in graphemes.chunks(columns.max(1)) {
            let size = part.len();
            if length > 0 && length + 1 + size > columns {
                result.push(std::mem::take(&mut line));
                length = 0;
            }
            if !line.is_empty() {
                line.push(' ');
                length += 1;
            }
            line.push_str(&part.concat());
            length += size;
        }
    }
    if !line.is_empty() {
        result.push(line);
    }
    result
}
/// Split long recognition results into at most two lines per cue, preserving their time window.
/// Where the provider supplies only a segment time, sub-cue boundaries are proportional estimates.
pub fn readable(segments: Vec<Segment>, columns: usize) -> Vec<Segment> {
    let mut result = Vec::new();
    for segment in segments {
        if !segment.start.is_finite() || !segment.end.is_finite() || segment.end <= segment.start {
            continue;
        }
        let rows = lines(&segment.text, columns);
        let groups = rows
            .chunks(2)
            .map(|rows| rows.join("\n"))
            .collect::<Vec<_>>();
        let weights = groups
            .iter()
            .map(|text| text.graphemes(true).count())
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
}
