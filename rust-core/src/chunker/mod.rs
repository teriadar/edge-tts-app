use serde::{Deserialize, Serialize};
use unicode_segmentation::UnicodeSegmentation;

use crate::tts::TtsChunk;

const ABBREVIATIONS: &[&str] = &[
    "art.", "artt.", "avv.", "cap.", "cfr.", "cit.", "dott.", "dott.ssa", "dr.", "ecc.", "es.",
    "etc.", "fig.", "figg.", "geom.", "ing.", "mr.", "mrs.", "ms.", "mons.", "n.", "nn.", "nr.",
    "num.", "pag.", "pagg.", "prof.", "prof.ssa", "rag.", " s.p.a.", "s.r.l.", "sig.", "sig.na",
    "sig.ra", "sigg.", "spett.", "st.", "tel.", "uff.", "v.", "vs.", "vol.", "voll.",
];

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WordSpan {
    pub id: u32,
    pub text: String,
    pub char_start: u32,
    pub char_end: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum ParagraphKind {
    #[default]
    Body,
    Title,
    Subtitle,
    Quote,
    Footnote,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ParagraphData {
    pub id: u32,
    pub text: String,
    #[serde(default)]
    pub kind: ParagraphKind,
    #[serde(default)]
    pub chapter_offset: u32,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub words: Vec<WordSpan>,
}

impl ParagraphData {

    pub fn words_or_tokenize(&self) -> std::borrow::Cow<'_, [WordSpan]> {
        if !self.words.is_empty() {
            std::borrow::Cow::Borrowed(&self.words)
        } else {
            std::borrow::Cow::Owned(tokenize_words_linear(&self.text, 0))
        }
    }
}

pub fn tokenize_words_linear(text: &str, start_word_id: u32) -> Vec<WordSpan> {
    let mut words = Vec::new();
    let mut next_id = start_word_id;
    let mut cur_byte = 0usize;
    let mut cur_char = 0u32;

    for (byte_offset, word) in text.split_word_bound_indices() {
        if byte_offset > cur_byte {
            cur_char += text[cur_byte..byte_offset].chars().count() as u32;
        }
        let char_len = word.chars().count() as u32;
        let char_start = cur_char;
        cur_byte = byte_offset + word.len();
        cur_char += char_len;

        if !word.chars().any(|c| c.is_alphanumeric()) {
            continue;
        }
        words.push(WordSpan {
            id: next_id,
            text: word.to_string(),
            char_start,
            char_end: char_start + char_len,
        });
        next_id += 1;
    }

    words
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChapterData {
    #[serde(default)]
    pub index: u32,
    pub title: String,
    #[serde(default)]
    pub href: String,
    pub paragraphs: Vec<ParagraphData>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SentenceChunk {
    pub chunk_index: u32,
    #[serde(default)]
    pub paragraph_id: u32,
    pub text: String,
    pub char_start: u32,
    pub char_end: u32,
    pub word_ids: Vec<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WordAudioTiming {
    pub word_id: u32,
    #[serde(default)]
    pub paragraph_id: u32,
    pub text: String,
    pub char_start: u32,
    pub char_end: u32,

    #[serde(alias = "audio_start_ms")]
    pub audio_offset_ms: u64,

    #[serde(default)]
    pub audio_duration_ms: u64,

    pub audio_end_ms: u64,
}

pub fn tokenize_paragraph(paragraph_id: u32, text: &str, start_word_id: u32) -> ParagraphData {
    ParagraphData {
        id: paragraph_id,
        text: text.to_string(),
        kind: ParagraphKind::Body,
        chapter_offset: 0,
        words: tokenize_words_linear(text, start_word_id),
    }
}

fn ends_with_abbreviation(segment: &str) -> bool {
    let trimmed = segment.trim_end();
    if !trimmed.ends_with('.') {
        return false;
    }
    let lower = trimmed.to_lowercase();
    for abbr in ABBREVIATIONS {
        if lower.ends_with(abbr) {
            let prefix_len = lower.len() - abbr.len();
            if prefix_len == 0
                || lower[..prefix_len]
                    .chars()
                    .last()
                    .is_none_or(|c| !c.is_alphabetic())
            {
                return true;
            }
        }
    }

    let without_dot = &trimmed[..trimmed.len() - 1];
    if let Some(last_char) = without_dot.chars().last() {
        if last_char.is_uppercase() {
            let before_len = without_dot.len() - last_char.len_utf8();
            if before_len == 0
                || without_dot[..before_len]
                    .chars()
                    .last()
                    .is_none_or(|c| !c.is_alphabetic())
            {
                return true;
            }
        }
    }

    false
}

pub fn split_sentences_with_offsets(text: &str) -> Vec<(usize, usize, &str)> {
    let raw_bounds: Vec<(usize, &str)> = text.split_sentence_bound_indices().collect();
    if raw_bounds.is_empty() {
        return Vec::new();
    }

    let mut merged: Vec<(usize, usize, &str)> = Vec::new();
    let mut acc_start: Option<usize> = None;

    for (byte_start, slice) in raw_bounds {
        let start = acc_start.unwrap_or(byte_start);
        let end = byte_start + slice.len();
        let combined = &text[start..end];

        if ends_with_abbreviation(combined) {
            acc_start = Some(start);
        } else {
            if !combined.trim().is_empty() {
                merged.push((start, end, combined));
            }
            acc_start = None;
        }
    }

    if let Some(start) = acc_start {
        let end = text.len();
        let combined = &text[start..end];
        if !combined.trim().is_empty() {
            merged.push((start, end, combined));
        }
    }

    merged
}

pub fn chunk_paragraph_sentences(
    paragraph: &ParagraphData,
    sentences_per_chunk: usize,
) -> Vec<SentenceChunk> {
    let group_size = sentences_per_chunk.clamp(1, 10);
    let sentences = split_sentences_with_offsets(&paragraph.text);
    if sentences.is_empty() {
        return Vec::new();
    }

    let words = paragraph.words_or_tokenize();
    let mut chunks = Vec::new();
    let mut cur_byte = 0usize;
    let mut cur_char = 0u32;

    for (chunk_idx, group) in sentences.chunks(group_size).enumerate() {
        let first_byte = group.first().unwrap().0;
        let last_byte = group.last().unwrap().1;
        let slice = &paragraph.text[first_byte..last_byte];

        if first_byte > cur_byte {
            cur_char += paragraph.text[cur_byte..first_byte].chars().count() as u32;
        }
        let char_start = cur_char;
        let char_len = slice.chars().count() as u32;
        let char_end = char_start + char_len;
        cur_byte = last_byte;
        cur_char = char_end;

        let word_ids: Vec<u32> = words
            .iter()
            .filter(|w| w.char_start >= char_start && w.char_end <= char_end)
            .map(|w| w.id)
            .collect();

        chunks.push(SentenceChunk {
            chunk_index: chunk_idx as u32,
            paragraph_id: paragraph.id,
            text: slice.trim().to_string(),
            char_start,
            char_end,
            word_ids,
        });
    }

    chunks
}

fn normalize_token(s: &str) -> String {
    s.chars()
        .filter(|c| c.is_alphanumeric())
        .flat_map(|c| c.to_lowercase())
        .collect()
}

pub fn align_word_boundaries(
    paragraph: &ParagraphData,
    chunk_char_start: u32,
    tts_chunks: &[TtsChunk],
) -> Vec<WordAudioTiming> {
    let words = paragraph.words_or_tokenize();
    let candidate_words: Vec<&WordSpan> = words
        .iter()
        .filter(|w| w.char_start >= chunk_char_start)
        .collect();

    let mut timings = Vec::new();
    let mut cursor = 0usize;

    for chunk in tts_chunks {
        let TtsChunk::WordBoundary {
            offset,
            duration,
            text,
        } = chunk
        else {
            continue;
        };

        let target_norm = normalize_token(text);
        if target_norm.is_empty() {
            continue;
        }

        let search_end = (cursor + 6).min(candidate_words.len());
        let mut matched_idx = None;
        for (i, candidate) in candidate_words[cursor..search_end].iter().enumerate() {
            let cand_norm = normalize_token(&candidate.text);
            if cand_norm == target_norm
                || cand_norm.starts_with(&target_norm)
                || target_norm.starts_with(&cand_norm)
            {
                matched_idx = Some(cursor + i);
                break;
            }
        }

        let chosen_idx = matched_idx.or({
            if cursor < candidate_words.len() {
                Some(cursor)
            } else {
                None
            }
        });

        if let Some(idx) = chosen_idx {
            let span = candidate_words[idx];
            let audio_offset_ms = offset / 10_000;
            let audio_duration_ms = (duration / 10_000).max(1);
            timings.push(WordAudioTiming {
                word_id: span.id,
                paragraph_id: paragraph.id,
                text: span.text.clone(),
                char_start: span.char_start,
                char_end: span.char_end,
                audio_offset_ms,
                audio_duration_ms,
                audio_end_ms: audio_offset_ms + audio_duration_ms,
            });
            cursor = idx + 1;
        }
    }

    timings
}

pub fn find_word_at_offset(words: &[WordSpan], char_offset: u32) -> Option<&WordSpan> {
    if words.is_empty() {
        return None;
    }

    match words.binary_search_by(|span| {
        if char_offset < span.char_start {
            std::cmp::Ordering::Greater
        } else if char_offset >= span.char_end {
            std::cmp::Ordering::Less
        } else {
            std::cmp::Ordering::Equal
        }
    }) {
        Ok(idx) => words.get(idx),
        Err(insert_idx) => {
            if insert_idx == 0 {
                words.first()
            } else if insert_idx >= words.len() {
                words.last()
            } else {
                let prev = &words[insert_idx - 1];
                let next = &words[insert_idx];
                let dist_prev = char_offset.saturating_sub(prev.char_end);
                let dist_next = next.char_start.saturating_sub(char_offset);
                if dist_next <= dist_prev {
                    Some(next)
                } else {
                    Some(prev)
                }
            }
        }
    }
}

pub fn find_active_word_by_position_ms(
    timings: &[WordAudioTiming],
    current_position_ms: u64,
) -> Option<&WordAudioTiming> {
    if timings.is_empty() {
        return None;
    }

    match timings.binary_search_by(|t| {
        if current_position_ms < t.audio_offset_ms {
            std::cmp::Ordering::Greater
        } else if current_position_ms > t.audio_end_ms {
            std::cmp::Ordering::Less
        } else {
            std::cmp::Ordering::Equal
        }
    }) {
        Ok(idx) => timings.get(idx),
        Err(insert_idx) => {

            if insert_idx > 0 {
                timings.get(insert_idx - 1)
            } else {
                timings.first()
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_abbreviation_aware_sentence_chunking() {
        let text = "Il Dott. Rossi ha citato l'art. 5 del codice. Questa è la seconda frase! E questa è la terza? Infine la quarta frase.";
        let para = tokenize_paragraph(0, text, 0);
        let sentences = split_sentences_with_offsets(&para.text);

        assert_eq!(sentences.len(), 4);
        assert!(sentences[0].2.contains("Dott. Rossi ha citato l'art. 5 del codice."));

        let chunks = chunk_paragraph_sentences(&para, 3);
        assert_eq!(chunks.len(), 2);
        assert!(!chunks[0].word_ids.is_empty());
        assert!(!chunks[1].word_ids.is_empty());
    }

    #[test]
    fn test_find_word_at_offset_handles_whitespace_and_punctuation() {
        let para = tokenize_paragraph(0, "Hello, world! How are you?", 0);

        let on_comma = find_word_at_offset(&para.words, 5).unwrap();
        assert_eq!(on_comma.text, "Hello");
        let on_space = find_word_at_offset(&para.words, 6).unwrap();
        assert_eq!(on_space.text, "world");
    }

    #[test]
    fn test_align_word_boundaries_and_exoplayer_lookup() {
        let para = tokenize_paragraph(1, "Ciao mondo bello!", 10);
        let tts_events = vec![
            TtsChunk::WordBoundary {
                offset: 1_000_000,
                duration: 3_000_000,
                text: "Ciao".to_string(),
            },
            TtsChunk::WordBoundary {
                offset: 4_500_000,
                duration: 4_000_000,
                text: "mondo".to_string(),
            },
            TtsChunk::WordBoundary {
                offset: 9_000_000,
                duration: 3_500_000,
                text: "bello".to_string(),
            },
        ];

        let timings = align_word_boundaries(&para, 0, &tts_events);
        assert_eq!(timings.len(), 3);
        assert_eq!(timings[0].word_id, 10);
        assert_eq!(timings[0].audio_offset_ms, 100);
        assert_eq!(timings[1].word_id, 11);
        assert_eq!(timings[1].audio_offset_ms, 450);

        let active = find_active_word_by_position_ms(&timings, 500).unwrap();
        assert_eq!(active.text, "mondo");
        assert_eq!(active.word_id, 11);

        let before_first = find_active_word_by_position_ms(&timings, 0).unwrap();
        assert_eq!(before_first.word_id, 10);

        let in_gap = find_active_word_by_position_ms(&timings, 420).unwrap();
        assert_eq!(in_gap.word_id, 10);

        let after_last = find_active_word_by_position_ms(&timings, 5000).unwrap();
        assert_eq!(after_last.word_id, 12);
    }

    #[test]
    fn test_tokenize_compound_words_with_apostrophes_and_hyphens() {
        let text = "L'architettura well-known dell’antico manoscritto.";
        let para = tokenize_paragraph(7, text, 100);
        let words: Vec<&str> = para.words.iter().map(|w| w.text.as_str()).collect();
        assert_eq!(
            words,
            vec!["L'architettura", "well", "known", "dell’antico", "manoscritto"]
        );
        assert_eq!(para.words[0].id, 100);
        assert_eq!(para.words[4].id, 104);
    }
}
