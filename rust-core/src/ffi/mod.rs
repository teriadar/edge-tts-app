pub mod jni_bridge;

use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use serde::{Deserialize, Serialize};

pub use crate::chunker::{
    align_word_boundaries, chunk_paragraph_sentences, find_active_word_by_position_ms,
    find_word_at_offset, ChapterData, ParagraphData, SentenceChunk, WordAudioTiming, WordSpan,
};
pub use crate::epub::{
    fetch_book_metadata_online, parse_epub_book, parse_epub_file, EpubBookData, OnlineBookMetadata,
};
pub use crate::tts::{
    check_edge_health, english_voice_presets, list_english_voices, quick_ws_ping, BoundaryType,
    Communicate, EdgeHealthReport, EdgeTtsError, ServiceStatus, TtsCache, TtsCacheStats, TtsChunk,
    Voice, VoicePreset,
};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SynthesizedChunk {
    pub paragraph_id: u32,
    pub chunk_index: u32,
    pub audio_mp3: Vec<u8>,
    pub word_timings: Vec<WordAudioTiming>,
}

#[derive(Clone)]
pub struct PrebufferSession {
    cache: TtsCache,
    voice: String,
    rate: String,
    pitch: String,
    generation: Arc<AtomicU64>,
}

impl PrebufferSession {

    pub fn new(cache_dir: Option<impl AsRef<Path>>, voice: impl Into<String>, rate: impl Into<String>) -> Self {
        let raw_rate: String = rate.into();
        let (clean_rate, parsed_pitch) = if let Some((r, p)) = raw_rate.split_once("|pitch=") {
            (
                if r.trim().is_empty() { "+0%".to_string() } else { r.trim().to_string() },
                if p.trim().is_empty() { "+0Hz".to_string() } else { p.trim().to_string() },
            )
        } else {
            (raw_rate, "+0Hz".to_string())
        };
        Self {
            cache: TtsCache::new(cache_dir),
            voice: voice.into(),
            rate: clean_rate,
            pitch: parsed_pitch,
            generation: Arc::new(AtomicU64::new(1)),
        }
    }

    pub fn voice(&self) -> &str {
        &self.voice
    }

    pub fn rate(&self) -> &str {
        &self.rate
    }

    pub fn pitch(&self) -> &str {
        &self.pitch
    }

    pub fn cache(&self) -> &TtsCache {
        &self.cache
    }

    pub fn current_generation(&self) -> u64 {
        self.generation.load(Ordering::SeqCst)
    }

    pub fn cancel_pending_queue(&self) -> u64 {
        self.generation.fetch_add(1, Ordering::SeqCst) + 1
    }

    pub async fn get_or_synthesize_chunk(
        &self,
        paragraph: &ParagraphData,
        sentence_chunk: &SentenceChunk,
    ) -> Result<SynthesizedChunk, EdgeTtsError> {
        let key = TtsCache::compute_key(
            &sentence_chunk.text,
            &self.voice,
            &self.rate,
            "+0%",
            &self.pitch,
        );

        if let Some(mut cached) = self.cache.get(&key) {
            cached.paragraph_id = paragraph.id;
            cached.chunk_index = sentence_chunk.chunk_index;
            return Ok(cached);
        }

        let synthesized = PaginaEngine::synthesize_sentence_chunk_with_pitch(
            paragraph,
            sentence_chunk,
            &self.voice,
            &self.rate,
            &self.pitch,
        )
        .await?;

        let _ = self.cache.put(&key, &synthesized);
        Ok(synthesized)
    }

    pub async fn synthesize_with_prebuffer(
        &self,
        paragraph: &ParagraphData,
        chunks: &[SentenceChunk],
        active_idx: usize,
    ) -> Result<SynthesizedChunk, EdgeTtsError> {
        self.synthesize_with_prebuffer_and_next_paragraph(paragraph, chunks, active_idx, None)
            .await
    }

    pub async fn synthesize_with_prebuffer_and_next_paragraph(
        &self,
        paragraph: &ParagraphData,
        chunks: &[SentenceChunk],
        active_idx: usize,
        next_paragraph: Option<&ParagraphData>,
    ) -> Result<SynthesizedChunk, EdgeTtsError> {
        let Some(current_chunk) = chunks.get(active_idx) else {
            return Err(EdgeTtsError::InvalidConfig(format!(
                "Invalid chunk index {}",
                active_idx
            )));
        };

        let active_gen = self.current_generation();

        if let Some(next_chunk) = chunks.get(active_idx + 1).cloned() {
            let session_clone = self.clone();
            let para_clone = paragraph.clone();
            tokio::spawn(async move {
                if session_clone.current_generation() != active_gen {
                    return;
                }
                let _ = session_clone
                    .get_or_synthesize_chunk(&para_clone, &next_chunk)
                    .await;
            });
        } else if let Some(next_para) = next_paragraph.cloned() {
            let session_clone = self.clone();
            tokio::spawn(async move {
                if session_clone.current_generation() != active_gen {
                    return;
                }
                let next_para_chunks = chunk_paragraph_sentences(&next_para, 4);
                if let Some(first_next_chunk) = next_para_chunks.first() {
                    let _ = session_clone
                        .get_or_synthesize_chunk(&next_para, first_next_chunk)
                        .await;
                }
            });
        }

        self.get_or_synthesize_chunk(paragraph, current_chunk).await
    }

    pub fn click_to_read_resolve(
        &self,
        paragraph: &ParagraphData,
        char_offset: u32,
        sentences_per_chunk: usize,
    ) -> Option<(WordSpan, usize, Vec<SentenceChunk>)> {
        let words = paragraph.words_or_tokenize();
        let tapped_word = find_word_at_offset(&words, char_offset)?.clone();
        self.cancel_pending_queue();

        let chunks = chunk_paragraph_sentences(paragraph, sentences_per_chunk);
        let chunk_idx = chunks
            .iter()
            .position(|c| c.word_ids.contains(&tapped_word.id))
            .unwrap_or(0);

        Some((tapped_word, chunk_idx, chunks))
    }
}

pub struct PaginaEngine;

impl PaginaEngine {

    pub async fn check_health(timeout_ms: u64) -> EdgeHealthReport {
        check_edge_health(timeout_ms).await
    }

    pub async fn quick_ping(timeout_ms: u64) -> EdgeHealthReport {
        quick_ws_ping(timeout_ms).await
    }

    pub fn get_english_voice_presets() -> Vec<VoicePreset> {
        english_voice_presets()
    }

    pub async fn fetch_english_voices() -> Result<Vec<Voice>, EdgeTtsError> {
        list_english_voices(None).await
    }

    pub fn load_epub(path: &str) -> Result<Vec<ChapterData>, EdgeTtsError> {
        parse_epub_file(path)
    }

    pub fn load_epub_book(
        path: &str,
        cover_output_dir: Option<&Path>,
    ) -> Result<EpubBookData, EdgeTtsError> {
        parse_epub_book(path, cover_output_dir)
    }

    pub async fn fetch_book_metadata_online(
        title: &str,
        author: Option<&str>,
        isbn: Option<&str>,
        cover_output_dir: Option<&Path>,
    ) -> Result<OnlineBookMetadata, EdgeTtsError> {
        fetch_book_metadata_online(title, author, isbn, cover_output_dir).await
    }

    pub fn chunk_paragraph(
        paragraph: &ParagraphData,
        sentences_per_chunk: usize,
    ) -> Vec<SentenceChunk> {
        chunk_paragraph_sentences(paragraph, sentences_per_chunk)
    }

    pub async fn synthesize_sentence_chunk(
        paragraph: &ParagraphData,
        sentence_chunk: &SentenceChunk,
        voice: &str,
        rate: &str,
    ) -> Result<SynthesizedChunk, EdgeTtsError> {
        Self::synthesize_sentence_chunk_with_pitch(paragraph, sentence_chunk, voice, rate, "+0Hz")
            .await
    }

    pub async fn synthesize_sentence_chunk_with_pitch(
        paragraph: &ParagraphData,
        sentence_chunk: &SentenceChunk,
        voice: &str,
        rate: &str,
        pitch: &str,
    ) -> Result<SynthesizedChunk, EdgeTtsError> {
        let tts_chunks =
            Self::synthesize_with_word_boundaries_and_pitch(&sentence_chunk.text, voice, rate, pitch)
                .await?;

        let mut audio_mp3 = Vec::new();
        for c in &tts_chunks {
            if let TtsChunk::Audio { data } = c {
                audio_mp3.extend_from_slice(data);
            }
        }

        let word_timings =
            align_word_boundaries(paragraph, sentence_chunk.char_start, &tts_chunks);

        Ok(SynthesizedChunk {
            paragraph_id: paragraph.id,
            chunk_index: sentence_chunk.chunk_index,
            audio_mp3,
            word_timings,
        })
    }

    pub async fn synthesize_with_word_boundaries(
        text: &str,
        voice: &str,
        rate: &str,
    ) -> Result<Vec<TtsChunk>, EdgeTtsError> {
        Self::synthesize_with_word_boundaries_and_pitch(text, voice, rate, "+0Hz").await
    }

    pub async fn synthesize_with_word_boundaries_and_pitch(
        text: &str,
        voice: &str,
        rate: &str,
        pitch: &str,
    ) -> Result<Vec<TtsChunk>, EdgeTtsError> {
        let mut comm = Communicate::with_options(
            text,
            voice,
            rate,
            "+0%",
            pitch,
            BoundaryType::WordBoundary,
        )?;
        comm.stream().await
    }

    pub fn active_word_at_ms(
        timings: &[WordAudioTiming],
        current_position_ms: u64,
    ) -> Option<WordAudioTiming> {
        find_active_word_by_position_ms(timings, current_position_ms).cloned()
    }

    pub fn word_at_char_offset(paragraph: &ParagraphData, char_offset: u32) -> Option<WordSpan> {
        find_word_at_offset(&paragraph.words, char_offset).cloned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chunker::tokenize_paragraph;

    #[test]
    fn test_click_to_read_resolves_word_and_cancels_queue() {
        let text = "Prima frase del paragrafo. Seconda frase del paragrafo. Terza frase del paragrafo. Quarta frase finale!";
        let para = tokenize_paragraph(0, text, 0);
        let session = PrebufferSession::new(None::<&Path>, "it-IT-ElsaNeural", "+0%");
        let gen_before = session.current_generation();

        let (word, chunk_idx, chunks) = session.click_to_read_resolve(&para, 85, 2).unwrap();
        assert_eq!(word.text, "Quarta");
        assert_eq!(chunk_idx, 1);
        assert_eq!(chunks.len(), 2);
        assert_eq!(session.current_generation(), gen_before + 1);
    }
}
