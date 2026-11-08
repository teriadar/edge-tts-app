pub mod chunker;
pub mod epub;
pub mod ffi;
pub mod tts;

pub use chunker::{
    align_word_boundaries, chunk_paragraph_sentences, find_active_word_by_position_ms,
    find_word_at_offset, split_sentences_with_offsets, tokenize_paragraph, ChapterData,
    ParagraphData, SentenceChunk, WordAudioTiming, WordSpan,
};
pub use epub::{
    fetch_book_metadata_online, parse_container_xml, parse_epub_book, parse_epub_file,
    parse_opf_document, parse_xhtml_chapter, EpubBookData, OnlineBookMetadata, OpfPackage,
};
pub use ffi::{NuvolaEngine, PrebufferSession, SynthesizedChunk};
pub use tts::{
    check_edge_health, english_voice_presets, list_english_voices, list_voices, quick_ws_ping,
    sha256_hex, BoundaryType, Communicate, Drm, EdgeHealthReport, EdgeTtsError, ProgressBar,
    RetryPolicy, ServiceStatus, SubMaker, Subtitle, TtsCache, TtsChunk, TtsConfig, Voice,
    VoicePreset, VoicesManager,
};
