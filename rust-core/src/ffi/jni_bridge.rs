use std::path::PathBuf;
use std::sync::{OnceLock, RwLock};

use jni::objects::{JClass, JString};
use jni::sys::{jint, jlong, jstring};
use jni::JNIEnv;
use serde::{Deserialize, Serialize};
use tokio::runtime::{Builder, Runtime};

use crate::chunker::{ParagraphData, SentenceChunk, WordAudioTiming, WordSpan};
use crate::ffi::{NuvolaEngine, PrebufferSession, SynthesizedChunk};
use crate::tts::constants::DEFAULT_VOICE;
use crate::tts::{EdgeHealthReport, TtsCache, VoicePreset};

static TOKIO_RUNTIME: OnceLock<Runtime> = OnceLock::new();

static ACTIVE_SESSION: OnceLock<RwLock<PrebufferSession>> = OnceLock::new();

fn runtime() -> &'static Runtime {
    TOKIO_RUNTIME.get_or_init(|| {
        Builder::new_multi_thread()
            .worker_threads(2)
            .enable_all()
            .thread_name("nuvola-tokio-jni")
            .build()
            .expect("Failed to initialize Nuvola Tokio runtime")
    })
}

fn active_session() -> &'static RwLock<PrebufferSession> {
    ACTIVE_SESSION.get_or_init(|| {
        RwLock::new(PrebufferSession::new(
            None::<PathBuf>,
            DEFAULT_VOICE,
            "+0%",
        ))
    })
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BridgeResponse<T> {
    pub ok: bool,
    pub data: Option<T>,
    pub error: Option<String>,
}

impl<T: Serialize> BridgeResponse<T> {
    pub fn success(data: T) -> String {
        serde_json::to_string(&Self {
            ok: true,
            data: Some(data),
            error: None,
        })
        .unwrap_or_else(|e| {
            format!(
                r#"{{"ok":false,"data":null,"error":"Serialization error: {}"}}"#,
                e
            )
        })
    }
}

pub fn bridge_error_json(msg: impl Into<String>) -> String {
    let resp: BridgeResponse<()> = BridgeResponse {
        ok: false,
        data: None,
        error: Some(msg.into()),
    };
    serde_json::to_string(&resp)
        .unwrap_or_else(|_| r#"{"ok":false,"data":null,"error":"Bridge error"}"#.to_string())
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JniSynthesizedChunkDto {
    pub paragraph_id: u32,
    pub chunk_index: u32,
    pub cache_key_sha256: String,
    pub audio_file_path: Option<String>,
    pub audio_mp3: Vec<u8>,
    pub word_timings: Vec<WordAudioTiming>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JniClickToReadDto {
    pub tapped_word: WordSpan,
    pub target_chunk_index: usize,
    pub chunks: Vec<SentenceChunk>,
    pub generation: u64,
}

pub fn bridge_init_session(cache_dir: Option<&str>, voice: &str, rate: &str) -> String {
    let dir_path = cache_dir
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(PathBuf::from);
    let voice_trimmed = if voice.trim().is_empty() {
        DEFAULT_VOICE
    } else {
        voice.trim()
    };
    let rate_trimmed = if rate.trim().is_empty() {
        "+0%"
    } else {
        rate.trim()
    };

    let new_session = PrebufferSession::new(dir_path, voice_trimmed, rate_trimmed);
    match active_session().write() {
        Ok(mut guard) => {
            *guard = new_session;
            BridgeResponse::success(true)
        }
        Err(e) => bridge_error_json(format!("Failed to acquire session write lock: {}", e)),
    }
}

pub fn bridge_cancel_pending_queue() -> String {
    match active_session().read() {
        Ok(guard) => {
            let gen_id = guard.cancel_pending_queue();
            BridgeResponse::success(gen_id)
        }
        Err(e) => bridge_error_json(format!("Failed to acquire session lock: {}", e)),
    }
}

pub fn bridge_check_health_json(timeout_ms: u64) -> String {
    let report: EdgeHealthReport = runtime().block_on(NuvolaEngine::check_health(timeout_ms));
    BridgeResponse::success(report)
}

pub fn bridge_quick_ping_json(timeout_ms: u64) -> String {
    let report: EdgeHealthReport = runtime().block_on(NuvolaEngine::quick_ping(timeout_ms));
    BridgeResponse::success(report)
}

pub fn bridge_get_english_presets_json() -> String {
    let presets: Vec<VoicePreset> = NuvolaEngine::get_english_voice_presets();
    BridgeResponse::success(presets)
}

pub fn bridge_fetch_english_voices_json() -> String {
    match runtime().block_on(NuvolaEngine::fetch_english_voices()) {
        Ok(voices) => BridgeResponse::success(voices),
        Err(e) => bridge_error_json(e.to_string()),
    }
}

pub fn bridge_load_epub_json(path: &str) -> String {
    match NuvolaEngine::load_epub(path) {
        Ok(chapters) => BridgeResponse::success(chapters),
        Err(e) => bridge_error_json(e.to_string()),
    }
}

pub fn bridge_load_epub_book_json(path: &str, cover_output_dir: Option<&str>) -> String {
    let cover_dir = cover_output_dir
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(PathBuf::from);
    match NuvolaEngine::load_epub_book(path, cover_dir.as_deref()) {
        Ok(book) => BridgeResponse::success(book),
        Err(e) => bridge_error_json(e.to_string()),
    }
}

pub fn bridge_fetch_book_metadata_online_json(
    title: &str,
    author: Option<&str>,
    isbn: Option<&str>,
    cover_output_dir: Option<&str>,
) -> String {
    let cover_dir = cover_output_dir
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(PathBuf::from);
    let clean_author = author.map(str::trim).filter(|s| !s.is_empty());
    let clean_isbn = isbn.map(str::trim).filter(|s| !s.is_empty());

    match runtime().block_on(NuvolaEngine::fetch_book_metadata_online(
        title,
        clean_author,
        clean_isbn,
        cover_dir.as_deref(),
    )) {
        Ok(meta) => BridgeResponse::success(meta),
        Err(e) => bridge_error_json(e.to_string()),
    }
}

pub fn bridge_get_cache_stats_json() -> String {
    match active_session().read() {
        Ok(guard) => BridgeResponse::success(guard.cache().stats()),
        Err(e) => bridge_error_json(format!("Session lock poisoned: {}", e)),
    }
}

pub fn bridge_clear_cache_json() -> String {
    match active_session().read() {
        Ok(guard) => match guard.cache().clear() {
            Ok(()) => BridgeResponse::success(true),
            Err(e) => bridge_error_json(e.to_string()),
        },
        Err(e) => bridge_error_json(format!("Session lock poisoned: {}", e)),
    }
}

pub fn bridge_prune_cache_json(max_bytes: u64) -> String {
    match active_session().read() {
        Ok(guard) => match guard.cache().prune_disk(max_bytes) {
            Ok(evicted) => BridgeResponse::success(evicted),
            Err(e) => bridge_error_json(e.to_string()),
        },
        Err(e) => bridge_error_json(format!("Session lock poisoned: {}", e)),
    }
}

pub fn bridge_chunk_paragraph_json(paragraph_json: &str, sentences_per_chunk: usize) -> String {
    let paragraph: ParagraphData = match serde_json::from_str(paragraph_json) {
        Ok(p) => p,
        Err(e) => return bridge_error_json(format!("Invalid ParagraphData JSON: {}", e)),
    };
    let chunks = NuvolaEngine::chunk_paragraph(&paragraph, sentences_per_chunk);
    BridgeResponse::success(chunks)
}

pub fn bridge_synthesize_with_prebuffer_json(
    paragraph_json: &str,
    chunks_json: &str,
    active_idx: usize,
) -> String {
    bridge_synthesize_with_prebuffer_ext_json(paragraph_json, chunks_json, active_idx, None)
}

pub fn bridge_synthesize_with_prebuffer_ext_json(
    paragraph_json: &str,
    chunks_json: &str,
    active_idx: usize,
    next_paragraph_json: Option<&str>,
) -> String {
    let paragraph: ParagraphData = match serde_json::from_str(paragraph_json) {
        Ok(p) => p,
        Err(e) => return bridge_error_json(format!("Invalid ParagraphData JSON: {}", e)),
    };
    let chunks: Vec<SentenceChunk> = match serde_json::from_str(chunks_json) {
        Ok(c) => c,
        Err(e) => return bridge_error_json(format!("Invalid SentenceChunk[] JSON: {}", e)),
    };
    let next_paragraph: Option<ParagraphData> = next_paragraph_json
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .and_then(|s| serde_json::from_str(s).ok());

    let session = match active_session().read() {
        Ok(guard) => guard.clone(),
        Err(e) => return bridge_error_json(format!("Session lock poisoned: {}", e)),
    };

    let Some(target_chunk) = chunks.get(active_idx) else {
        return bridge_error_json(format!("Chunk index {} out of bounds", active_idx));
    };

    let key = TtsCache::compute_key(
        &target_chunk.text,
        session.voice(),
        session.rate(),
        "+0%",
        "+0Hz",
    );

    match runtime().block_on(session.synthesize_with_prebuffer_and_next_paragraph(
        &paragraph,
        &chunks,
        active_idx,
        next_paragraph.as_ref(),
    )) {
        Ok(SynthesizedChunk {
            paragraph_id,
            chunk_index,
            audio_mp3,
            word_timings,
        }) => {
            let audio_file_path = session
                .cache()
                .cache_dir()
                .map(|dir| dir.join(format!("{}.mp3", key)).to_string_lossy().into_owned());

            BridgeResponse::success(JniSynthesizedChunkDto {
                paragraph_id,
                chunk_index,
                cache_key_sha256: key,
                audio_file_path,
                audio_mp3,
                word_timings,
            })
        }
        Err(e) => bridge_error_json(e.to_string()),
    }
}

pub fn bridge_click_to_read_json(
    paragraph_json: &str,
    char_offset: u32,
    sentences_per_chunk: usize,
) -> String {
    let paragraph: ParagraphData = match serde_json::from_str(paragraph_json) {
        Ok(p) => p,
        Err(e) => return bridge_error_json(format!("Invalid ParagraphData JSON: {}", e)),
    };

    let session = match active_session().read() {
        Ok(guard) => guard.clone(),
        Err(e) => return bridge_error_json(format!("Session lock poisoned: {}", e)),
    };

    match session.click_to_read_resolve(&paragraph, char_offset, sentences_per_chunk) {
        Some((tapped_word, target_chunk_index, chunks)) => BridgeResponse::success(JniClickToReadDto {
            tapped_word,
            target_chunk_index,
            chunks,
            generation: session.current_generation(),
        }),
        None => bridge_error_json(format!(
            "No word found near character offset {}",
            char_offset
        )),
    }
}

pub fn bridge_active_word_at_ms_json(timings_json: &str, current_position_ms: u64) -> String {
    let timings: Vec<WordAudioTiming> = match serde_json::from_str(timings_json) {
        Ok(t) => t,
        Err(e) => return bridge_error_json(format!("Invalid WordAudioTiming[] JSON: {}", e)),
    };
    let active = NuvolaEngine::active_word_at_ms(&timings, current_position_ms);
    BridgeResponse::success(active)
}

fn jstring_to_rust(env: &mut JNIEnv, input: &JString) -> Result<String, String> {
    if input.is_null() {
        return Ok(String::new());
    }
    env.get_string(input)
        .map(Into::into)
        .map_err(|e| format!("JNI string conversion failed: {}", e))
}

fn rust_to_jstring(env: &mut JNIEnv, output: &str) -> jstring {
    env.new_string(output)
        .map(|s| s.into_raw())
        .unwrap_or(std::ptr::null_mut())
}

#[no_mangle]
pub extern "system" fn Java_com_nuvola_reader_bridge_NuvolaNativeBridge_nativeInitSession(
    mut env: JNIEnv,
    _class: JClass,
    cache_dir: JString,
    voice: JString,
    rate: JString,
) -> jstring {
    let dir_str = jstring_to_rust(&mut env, &cache_dir).unwrap_or_default();
    let voice_str = jstring_to_rust(&mut env, &voice).unwrap_or_default();
    let rate_str = jstring_to_rust(&mut env, &rate).unwrap_or_default();

    let opt_dir = if dir_str.trim().is_empty() {
        None
    } else {
        Some(dir_str.as_str())
    };
    let json = bridge_init_session(opt_dir, &voice_str, &rate_str);
    rust_to_jstring(&mut env, &json)
}

#[no_mangle]
pub extern "system" fn Java_com_nuvola_reader_bridge_NuvolaNativeBridge_nativeCancelPendingQueue(
    mut env: JNIEnv,
    _class: JClass,
) -> jstring {
    let json = bridge_cancel_pending_queue();
    rust_to_jstring(&mut env, &json)
}

#[no_mangle]
pub extern "system" fn Java_com_nuvola_reader_bridge_NuvolaNativeBridge_nativeCheckHealth(
    mut env: JNIEnv,
    _class: JClass,
    timeout_ms: jlong,
) -> jstring {
    let timeout = if timeout_ms <= 0 { 5000 } else { timeout_ms as u64 };
    let json = bridge_check_health_json(timeout);
    rust_to_jstring(&mut env, &json)
}

#[no_mangle]
pub extern "system" fn Java_com_nuvola_reader_bridge_NuvolaNativeBridge_nativeQuickPing(
    mut env: JNIEnv,
    _class: JClass,
    timeout_ms: jlong,
) -> jstring {
    let timeout = if timeout_ms <= 0 { 4000 } else { timeout_ms as u64 };
    let json = bridge_quick_ping_json(timeout);
    rust_to_jstring(&mut env, &json)
}

#[no_mangle]
pub extern "system" fn Java_com_nuvola_reader_bridge_NuvolaNativeBridge_nativeGetEnglishVoicePresets(
    mut env: JNIEnv,
    _class: JClass,
) -> jstring {
    let json = bridge_get_english_presets_json();
    rust_to_jstring(&mut env, &json)
}

#[no_mangle]
pub extern "system" fn Java_com_nuvola_reader_bridge_NuvolaNativeBridge_nativeFetchEnglishVoices(
    mut env: JNIEnv,
    _class: JClass,
) -> jstring {
    let json = bridge_fetch_english_voices_json();
    rust_to_jstring(&mut env, &json)
}

#[no_mangle]
pub extern "system" fn Java_com_nuvola_reader_bridge_NuvolaNativeBridge_nativeLoadEpub(
    mut env: JNIEnv,
    _class: JClass,
    epub_path: JString,
) -> jstring {
    let path = match jstring_to_rust(&mut env, &epub_path) {
        Ok(p) => p,
        Err(e) => return rust_to_jstring(&mut env, &bridge_error_json(e)),
    };
    let json = bridge_load_epub_json(&path);
    rust_to_jstring(&mut env, &json)
}

#[no_mangle]
pub extern "system" fn Java_com_nuvola_reader_bridge_NuvolaNativeBridge_nativeLoadEpubBook(
    mut env: JNIEnv,
    _class: JClass,
    epub_path: JString,
    cover_output_dir: JString,
) -> jstring {
    let path = match jstring_to_rust(&mut env, &epub_path) {
        Ok(p) => p,
        Err(e) => return rust_to_jstring(&mut env, &bridge_error_json(e)),
    };
    let cover_dir = jstring_to_rust(&mut env, &cover_output_dir).ok();
    let json = bridge_load_epub_book_json(&path, cover_dir.as_deref());
    rust_to_jstring(&mut env, &json)
}

#[no_mangle]
pub extern "system" fn Java_com_nuvola_reader_bridge_NuvolaNativeBridge_nativeFetchBookMetadataOnline(
    mut env: JNIEnv,
    _class: JClass,
    title: JString,
    author: JString,
    isbn: JString,
    cover_output_dir: JString,
) -> jstring {
    let title_str = jstring_to_rust(&mut env, &title).unwrap_or_default();
    let author_str = jstring_to_rust(&mut env, &author).ok();
    let isbn_str = jstring_to_rust(&mut env, &isbn).ok();
    let cover_dir = jstring_to_rust(&mut env, &cover_output_dir).ok();
    let json = bridge_fetch_book_metadata_online_json(
        &title_str,
        author_str.as_deref(),
        isbn_str.as_deref(),
        cover_dir.as_deref(),
    );
    rust_to_jstring(&mut env, &json)
}

#[no_mangle]
pub extern "system" fn Java_com_nuvola_reader_bridge_NuvolaNativeBridge_nativeGetCacheStats(
    mut env: JNIEnv,
    _class: JClass,
) -> jstring {
    let json = bridge_get_cache_stats_json();
    rust_to_jstring(&mut env, &json)
}

#[no_mangle]
pub extern "system" fn Java_com_nuvola_reader_bridge_NuvolaNativeBridge_nativeClearCache(
    mut env: JNIEnv,
    _class: JClass,
) -> jstring {
    let json = bridge_clear_cache_json();
    rust_to_jstring(&mut env, &json)
}

#[no_mangle]
pub extern "system" fn Java_com_nuvola_reader_bridge_NuvolaNativeBridge_nativePruneCache(
    mut env: JNIEnv,
    _class: JClass,
    max_bytes: jlong,
) -> jstring {
    let limit = if max_bytes <= 0 {
        150 * 1024 * 1024
    } else {
        max_bytes as u64
    };
    let json = bridge_prune_cache_json(limit);
    rust_to_jstring(&mut env, &json)
}

#[no_mangle]
pub extern "system" fn Java_com_nuvola_reader_bridge_NuvolaNativeBridge_nativeChunkParagraph(
    mut env: JNIEnv,
    _class: JClass,
    paragraph_json: JString,
    sentences_per_chunk: jint,
) -> jstring {
    let para_str = match jstring_to_rust(&mut env, &paragraph_json) {
        Ok(s) => s,
        Err(e) => return rust_to_jstring(&mut env, &bridge_error_json(e)),
    };
    let spc = if sentences_per_chunk <= 0 {
        4
    } else {
        sentences_per_chunk as usize
    };
    let json = bridge_chunk_paragraph_json(&para_str, spc);
    rust_to_jstring(&mut env, &json)
}

#[no_mangle]
pub extern "system" fn Java_com_nuvola_reader_bridge_NuvolaNativeBridge_nativeSynthesizeWithPrebuffer(
    mut env: JNIEnv,
    _class: JClass,
    paragraph_json: JString,
    chunks_json: JString,
    active_idx: jint,
) -> jstring {
    let para_str = match jstring_to_rust(&mut env, &paragraph_json) {
        Ok(s) => s,
        Err(e) => return rust_to_jstring(&mut env, &bridge_error_json(e)),
    };
    let chunks_str = match jstring_to_rust(&mut env, &chunks_json) {
        Ok(s) => s,
        Err(e) => return rust_to_jstring(&mut env, &bridge_error_json(e)),
    };
    let idx = if active_idx < 0 { 0 } else { active_idx as usize };
    let json = bridge_synthesize_with_prebuffer_json(&para_str, &chunks_str, idx);
    rust_to_jstring(&mut env, &json)
}

#[no_mangle]
pub extern "system" fn Java_com_nuvola_reader_bridge_NuvolaNativeBridge_nativeSynthesizeWithCrossParagraphPrebuffer(
    mut env: JNIEnv,
    _class: JClass,
    paragraph_json: JString,
    chunks_json: JString,
    active_idx: jint,
    next_paragraph_json: JString,
) -> jstring {
    let para_str = match jstring_to_rust(&mut env, &paragraph_json) {
        Ok(s) => s,
        Err(e) => return rust_to_jstring(&mut env, &bridge_error_json(e)),
    };
    let chunks_str = match jstring_to_rust(&mut env, &chunks_json) {
        Ok(s) => s,
        Err(e) => return rust_to_jstring(&mut env, &bridge_error_json(e)),
    };
    let next_para_str = jstring_to_rust(&mut env, &next_paragraph_json).ok();
    let idx = if active_idx < 0 { 0 } else { active_idx as usize };
    let json = bridge_synthesize_with_prebuffer_ext_json(
        &para_str,
        &chunks_str,
        idx,
        next_para_str.as_deref(),
    );
    rust_to_jstring(&mut env, &json)
}

#[no_mangle]
pub extern "system" fn Java_com_nuvola_reader_bridge_NuvolaNativeBridge_nativeClickToReadResolve(
    mut env: JNIEnv,
    _class: JClass,
    paragraph_json: JString,
    char_offset: jint,
    sentences_per_chunk: jint,
) -> jstring {
    let para_str = match jstring_to_rust(&mut env, &paragraph_json) {
        Ok(s) => s,
        Err(e) => return rust_to_jstring(&mut env, &bridge_error_json(e)),
    };
    let offset = if char_offset < 0 { 0 } else { char_offset as u32 };
    let spc = if sentences_per_chunk <= 0 {
        4
    } else {
        sentences_per_chunk as usize
    };
    let json = bridge_click_to_read_json(&para_str, offset, spc);
    rust_to_jstring(&mut env, &json)
}

#[no_mangle]
pub extern "system" fn Java_com_nuvola_reader_bridge_NuvolaNativeBridge_nativeActiveWordAtMs(
    mut env: JNIEnv,
    _class: JClass,
    timings_json: JString,
    position_ms: jlong,
) -> jstring {
    let timings_str = match jstring_to_rust(&mut env, &timings_json) {
        Ok(s) => s,
        Err(e) => return rust_to_jstring(&mut env, &bridge_error_json(e)),
    };
    let pos = if position_ms < 0 { 0 } else { position_ms as u64 };
    let json = bridge_active_word_at_ms_json(&timings_str, pos);
    rust_to_jstring(&mut env, &json)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chunker::tokenize_paragraph;

    #[test]
    fn test_bridge_english_presets_and_click_to_read_json() {
        let init_resp: BridgeResponse<bool> =
            serde_json::from_str(&bridge_init_session(None, "en-US-AriaNeural", "-10%")).unwrap();
        assert!(init_resp.ok);

        let presets_resp: BridgeResponse<Vec<VoicePreset>> =
            serde_json::from_str(&bridge_get_english_presets_json()).unwrap();
        assert!(presets_resp.ok);
        let presets = presets_resp.data.unwrap();
        assert_eq!(presets.len(), 8);
        assert_eq!(presets[0].short_name, "it-IT-IsabellaNeural");

        let para = tokenize_paragraph(
            0,
            "First sentence of the chapter. Second sentence follows here. Third sentence ends it.",
            0,
        );
        let para_json = serde_json::to_string(&para).unwrap();

        let click_resp: BridgeResponse<JniClickToReadDto> =
            serde_json::from_str(&bridge_click_to_read_json(&para_json, 32, 2)).unwrap();
        assert!(click_resp.ok);
        let dto = click_resp.data.unwrap();
        assert_eq!(dto.tapped_word.text, "Second");
        assert_eq!(dto.target_chunk_index, 0);
    }

    #[test]
    fn test_kotlin_json_roundtrip_compatibility() {

        let kotlin_timings_json = r#"[
            {
                "word_id": 0,
                "paragraph_id": 0,
                "text": "Once",
                "char_start": 0,
                "char_end": 4,
                "audio_offset_ms": 50,
                "audio_duration_ms": 200,
                "audio_end_ms": 250
            },
            {
                "word_id": 1,
                "text": "upon",
                "char_start": 5,
                "char_end": 9,
                "audio_start_ms": 260,
                "audio_end_ms": 480
            }
        ]"#;

        let active_resp: BridgeResponse<Option<WordAudioTiming>> =
            serde_json::from_str(&bridge_active_word_at_ms_json(kotlin_timings_json, 300)).unwrap();
        assert!(active_resp.ok);
        let active_word = active_resp.data.unwrap().unwrap();
        assert_eq!(active_word.word_id, 1);
        assert_eq!(active_word.text, "upon");
        assert_eq!(active_word.audio_offset_ms, 260);
    }

    #[test]
    fn test_bridge_load_epub_book_and_cache_stats() {
        let epub_path = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/paradise_lost.epub");
        let resp: BridgeResponse<crate::epub::EpubBookData> =
            serde_json::from_str(&bridge_load_epub_book_json(epub_path, None)).unwrap();
        assert!(resp.ok);
        let book = resp.data.unwrap();
        assert!(!book.title.is_empty());
        assert!(!book.chapters.is_empty());
        assert_eq!(book.chapters[0].index, 0);

        let stats_resp: BridgeResponse<crate::tts::TtsCacheStats> =
            serde_json::from_str(&bridge_get_cache_stats_json()).unwrap();
        assert!(stats_resp.ok);

        let clear_resp: BridgeResponse<bool> =
            serde_json::from_str(&bridge_clear_cache_json()).unwrap();
        assert!(clear_resp.ok);
    }
}
