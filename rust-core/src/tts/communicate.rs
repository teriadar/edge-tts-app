use std::collections::HashMap;
use std::fs::File;
use std::io::{BufWriter, Write};
use std::path::Path;

use chrono::Utc;
use futures_util::{SinkExt, StreamExt};
use serde_json::Value;
use tokio_tungstenite::connect_async;
use tokio_tungstenite::tungstenite::client::IntoClientRequest;
use tokio_tungstenite::tungstenite::http::HeaderValue;
use tokio_tungstenite::tungstenite::protocol::Message;
use tokio_tungstenite::tungstenite::Error as WsError;
use uuid::Uuid;

use crate::tts::constants::{
    wss_headers, DEFAULT_VOICE, MP3_BITRATE_BPS, SEC_MS_GEC_VERSION, TICKS_PER_SECOND, WSS_URL,
};
use crate::tts::drm::Drm;
use crate::tts::exceptions::{EdgeTtsError, Result};
use crate::tts::types::{BoundaryType, CommunicateState, TtsChunk, TtsConfig};

pub fn get_headers_and_data(
    data: &[u8],
    header_length: usize,
) -> (HashMap<Vec<u8>, Vec<u8>>, Vec<u8>) {
    let mut headers = HashMap::new();
    let header_slice = &data[..header_length.min(data.len())];

    for line in header_slice.split(|&b| b == b'\n') {
        let line = line.strip_suffix(b"\r").unwrap_or(line);
        if let Some(colon_pos) = line.iter().position(|&b| b == b':') {
            headers.insert(line[..colon_pos].to_vec(), line[colon_pos + 1..].to_vec());
        }
    }

    let body_start = (header_length + 2).min(data.len());
    (headers, data[body_start..].to_vec())
}

pub fn remove_incompatible_characters(input: &str) -> String {
    input
        .chars()
        .map(|c| {
            let code = c as u32;
            if (code <= 8) || (11..=12).contains(&code) || (14..=31).contains(&code) {
                ' '
            } else {
                c
            }
        })
        .collect()
}

pub fn escape_xml(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    for c in input.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            _ => out.push(c),
        }
    }
    out
}

pub fn unescape_xml(input: &str) -> String {
    input
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&amp;", "&")
}

pub fn connect_id() -> String {
    Uuid::new_v4().simple().to_string()
}

fn find_last_newline_or_space_within_limit(text: &[u8], limit: usize) -> Option<usize> {
    let search_slice = &text[..limit.min(text.len())];
    if let Some(pos) = search_slice.iter().rposition(|&b| b == b'\n') {
        return Some(pos);
    }
    search_slice.iter().rposition(|&b| b == b' ')
}

fn find_safe_utf8_split_point(text_segment: &[u8], limit: usize) -> usize {
    let mut split_at = limit.min(text_segment.len());
    while split_at > 0 {
        if std::str::from_utf8(&text_segment[..split_at]).is_ok() {
            return split_at;
        }
        split_at -= 1;
    }
    0
}

fn adjust_split_point_for_xml_entity(text: &[u8], mut split_at: usize) -> usize {
    while split_at > 0 {
        let slice = &text[..split_at];
        let Some(amp_idx) = slice.iter().rposition(|&b| b == b'&') else {
            break;
        };
        if slice[amp_idx..split_at].contains(&b';') {
            break;
        }
        split_at = amp_idx;
    }
    split_at
}

fn trim_ascii_whitespace(bytes: &[u8]) -> &[u8] {
    let start = bytes
        .iter()
        .position(|b| !b.is_ascii_whitespace())
        .unwrap_or(bytes.len());
    let end = bytes
        .iter()
        .rposition(|b| !b.is_ascii_whitespace())
        .map_or(start, |i| i + 1);
    &bytes[start..end]
}

pub fn split_text_by_byte_length(text: &str, byte_length: usize) -> Result<Vec<Vec<u8>>> {
    if byte_length == 0 {
        return Err(EdgeTtsError::InvalidConfig(
            "byte_length must be greater than 0".to_string(),
        ));
    }

    let mut remaining = text.as_bytes();
    let mut chunks = Vec::new();

    while remaining.len() > byte_length {
        let mut split_at = find_last_newline_or_space_within_limit(remaining, byte_length)
            .unwrap_or_else(|| find_safe_utf8_split_point(remaining, byte_length));

        split_at = adjust_split_point_for_xml_entity(remaining, split_at);

        let chunk = trim_ascii_whitespace(&remaining[..split_at]);
        if !chunk.is_empty() {
            chunks.push(chunk.to_vec());
        }

        let advance = if split_at > 0 { split_at } else { 1 };
        remaining = &remaining[advance..];
    }

    let tail = trim_ascii_whitespace(remaining);
    if !tail.is_empty() {
        chunks.push(tail.to_vec());
    }

    Ok(chunks)
}

pub fn mkssml(tc: &TtsConfig, escaped_text: &[u8]) -> String {
    let text_str = String::from_utf8_lossy(escaped_text);
    let mut parts = tc.voice.split('-');
    let xml_lang = match (parts.next(), parts.next()) {
        (Some(lang), Some(region)) if lang.len() == 2 && region.len() == 2 => {
            format!("{}-{}", lang, region)
        }
        _ => "en-US".to_string(),
    };
    format!(
        "<speak version='1.0' xmlns='http://www.w3.org/2001/10/synthesis' xml:lang='{}'><voice name='{}'><prosody pitch='{}' rate='{}' volume='{}'>{}</prosody></voice></speak>",
        xml_lang, tc.voice, tc.pitch, tc.rate, tc.volume, text_str
    )
}

pub fn date_to_string() -> String {
    Utc::now()
        .format("%a %b %d %Y %H:%M:%S GMT+0000 (Coordinated Universal Time)")
        .to_string()
}

pub fn ssml_headers_plus_data(request_id: &str, timestamp: &str, ssml: &str) -> String {
    format!(
        "X-RequestId:{}\r\nContent-Type:application/ssml+xml\r\nX-Timestamp:{}Z\r\nPath:ssml\r\n\r\n{}",
        request_id, timestamp, ssml
    )
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RetryPolicy {
    pub max_retries: u32,
    pub initial_backoff_ms: u64,
    pub max_backoff_ms: u64,
}

impl Default for RetryPolicy {
    fn default() -> Self {
        Self {
            max_retries: 3,
            initial_backoff_ms: 300,
            max_backoff_ms: 5_000,
        }
    }
}

impl RetryPolicy {

    pub fn backoff_ms_for_attempt(&self, attempt: u32) -> u64 {
        let factor = 1u64.checked_shl(attempt.min(16)).unwrap_or(u64::MAX);
        self.initial_backoff_ms
            .saturating_mul(factor)
            .min(self.max_backoff_ms)
    }
}

pub struct Communicate {
    pub tts_config: TtsConfig,
    pub texts: Vec<Vec<u8>>,
    pub state: CommunicateState,
    pub retry_policy: RetryPolicy,
}

impl Communicate {

    pub fn new(text: &str, voice: Option<&str>) -> Result<Self> {
        Self::with_options(
            text,
            voice.unwrap_or(DEFAULT_VOICE),
            "+0%",
            "+0%",
            "+0Hz",
            BoundaryType::SentenceBoundary,
        )
    }

    pub fn with_options(
        text: &str,
        voice: &str,
        rate: &str,
        volume: &str,
        pitch: &str,
        boundary: BoundaryType,
    ) -> Result<Self> {
        let tts_config = TtsConfig::new(voice, rate, volume, pitch, boundary)?;
        let cleaned = escape_xml(&remove_incompatible_characters(text));
        let texts = split_text_by_byte_length(&cleaned, 4096)?;

        Ok(Self {
            tts_config,
            texts,
            state: CommunicateState::default(),
            retry_policy: RetryPolicy::default(),
        })
    }

    pub fn with_retry_policy(mut self, policy: RetryPolicy) -> Self {
        self.retry_policy = policy;
        self
    }

    fn parse_metadata(&self, data: &[u8]) -> Result<Option<TtsChunk>> {
        let root: Value = serde_json::from_slice(data)?;
        let Some(metadata_arr) = root.get("Metadata").and_then(|v| v.as_array()) else {
            return Err(EdgeTtsError::UnexpectedResponse(
                "Missing 'Metadata' array in audio.metadata payload".to_string(),
            ));
        };

        for meta_obj in metadata_arr {
            let meta_type = meta_obj
                .get("Type")
                .and_then(|v| v.as_str())
                .unwrap_or_default();

            match meta_type {
                "WordBoundary" | "SentenceBoundary" => {
                    let data_obj = meta_obj.get("Data").ok_or_else(|| {
                        EdgeTtsError::UnexpectedResponse("Missing 'Data' in metadata".to_string())
                    })?;
                    let raw_offset = data_obj
                        .get("Offset")
                        .and_then(|v| v.as_u64())
                        .unwrap_or(0);
                    let current_offset = raw_offset + self.state.offset_compensation;
                    let current_duration = data_obj
                        .get("Duration")
                        .and_then(|v| v.as_u64())
                        .unwrap_or(0);
                    let raw_text = data_obj
                        .get("text")
                        .and_then(|t| t.get("Text"))
                        .and_then(|t| t.as_str())
                        .unwrap_or_default();
                    let unescaped = unescape_xml(raw_text);

                    if meta_type == "WordBoundary" {
                        return Ok(Some(TtsChunk::WordBoundary {
                            offset: current_offset,
                            duration: current_duration,
                            text: unescaped,
                        }));
                    } else {
                        return Ok(Some(TtsChunk::SentenceBoundary {
                            offset: current_offset,
                            duration: current_duration,
                            text: unescaped,
                        }));
                    }
                }
                "SessionEnd" => continue,
                other => {
                    return Err(EdgeTtsError::UnknownResponse(format!(
                        "Unknown metadata type: {}",
                        other
                    )));
                }
            }
        }

        Err(EdgeTtsError::UnexpectedResponse(
            "No WordBoundary metadata found".to_string(),
        ))
    }

    fn compensate_offset(&mut self) {
        self.state.cumulative_audio_bytes += self.state.chunk_audio_bytes;
        self.state.offset_compensation = self.state.cumulative_audio_bytes
            * 8
            * TICKS_PER_SECOND
            / MP3_BITRATE_BPS;
        self.state.chunk_audio_bytes = 0;
    }

    async fn stream_single_chunk(
        &mut self,
    ) -> std::result::Result<Vec<TtsChunk>, (Option<u16>, Option<String>, EdgeTtsError)> {
        let url = format!(
            "{}&ConnectionId={}&Sec-MS-GEC={}&Sec-MS-GEC-Version={}",
            WSS_URL,
            connect_id(),
            Drm::generate_sec_ms_gec(),
            SEC_MS_GEC_VERSION
        );

        let mut request = url.into_client_request().map_err(|e| {
            (
                None,
                None,
                EdgeTtsError::WebSocketError(format!("Failed to build WS request: {}", e)),
            )
        })?;

        let custom_headers = Drm::headers_with_muid(&wss_headers());
        for (k, v) in custom_headers {
            if let (Ok(header_name), Ok(header_val)) = (
                tokio_tungstenite::tungstenite::http::HeaderName::try_from(k.as_str()),
                HeaderValue::from_str(&v),
            ) {
                request.headers_mut().insert(header_name, header_val);
            }
        }

        let (mut websocket, _) = match connect_async(request).await {
            Ok(conn) => conn,
            Err(WsError::Http(resp)) => {
                let status = resp.status().as_u16();
                let date_hdr = resp
                    .headers()
                    .get("Date")
                    .and_then(|v| v.to_str().ok())
                    .map(|s| s.to_string());
                return Err((
                    Some(status),
                    date_hdr,
                    EdgeTtsError::WebSocketError(format!("HTTP error during WS handshake: {}", status)),
                ));
            }
            Err(e) => {
                return Err((
                    None,
                    None,
                    EdgeTtsError::WebSocketError(e.to_string()),
                ));
            }
        };

        let word_boundary = self.tts_config.boundary == BoundaryType::WordBoundary;
        let wd = if word_boundary { "true" } else { "false" };
        let sq = if !word_boundary { "true" } else { "false" };
        let config_msg = format!(
            "X-Timestamp:{}\r\nContent-Type:application/json; charset=utf-8\r\nPath:speech.config\r\n\r\n{{\"context\":{{\"synthesis\":{{\"audio\":{{\"metadataoptions\":{{\"sentenceBoundaryEnabled\":\"{}\",\"wordBoundaryEnabled\":\"{}\"}},\"outputFormat\":\"audio-24khz-48kbitrate-mono-mp3\"}}}}}}}}\r\n",
            date_to_string(),
            sq,
            wd
        );
        websocket
            .send(Message::Text(config_msg))
            .await
            .map_err(|e| (None, None, EdgeTtsError::WebSocketError(e.to_string())))?;

        let ssml_msg = ssml_headers_plus_data(
            &connect_id(),
            &date_to_string(),
            &mkssml(&self.tts_config, &self.state.partial_text),
        );
        websocket
            .send(Message::Text(ssml_msg))
            .await
            .map_err(|e| (None, None, EdgeTtsError::WebSocketError(e.to_string())))?;

        let mut audio_was_received = false;
        let mut out_chunks = Vec::new();

        while let Some(msg_res) = websocket.next().await {
            let msg = msg_res.map_err(|e| (None, None, EdgeTtsError::WebSocketError(e.to_string())))?;
            match msg {
                Message::Text(text_data) => {
                    let encoded = text_data.as_bytes();
                    let header_end = encoded
                        .windows(4)
                        .position(|w| w == b"\r\n\r\n")
                        .unwrap_or(encoded.len());
                    let (parameters, data) = get_headers_and_data(encoded, header_end);
                    let path = parameters.get(b"Path".as_slice()).map(|v| v.as_slice());

                    match path {
                        Some(b"audio.metadata") => {
                            if let Some(parsed) = self
                                .parse_metadata(&data)
                                .map_err(|e| (None, None, e))?
                            {
                                match &parsed {
                                    TtsChunk::WordBoundary {
                                        offset, duration, ..
                                    }
                                    | TtsChunk::SentenceBoundary {
                                        offset, duration, ..
                                    } => {
                                        self.state.last_duration_offset = offset + duration;
                                    }
                                    _ => {}
                                }
                                out_chunks.push(parsed);
                            }
                        }
                        Some(b"turn.end") => {
                            self.compensate_offset();
                            break;
                        }
                        Some(b"response") | Some(b"turn.start") => {}
                        _ => {
                            return Err((
                                None,
                                None,
                                EdgeTtsError::UnknownResponse("Unknown path received".to_string()),
                            ));
                        }
                    }
                }
                Message::Binary(bin_data) => {
                    if bin_data.len() < 2 {
                        return Err((
                            None,
                            None,
                            EdgeTtsError::UnexpectedResponse(
                                "Binary message is missing header length".to_string(),
                            ),
                        ));
                    }
                    let header_length = u16::from_be_bytes([bin_data[0], bin_data[1]]) as usize;
                    if header_length > bin_data.len() {
                        return Err((
                            None,
                            None,
                            EdgeTtsError::UnexpectedResponse(
                                "Header length is greater than binary data length".to_string(),
                            ),
                        ));
                    }
                    let (parameters, data) = get_headers_and_data(&bin_data, header_length);
                    if parameters.get(b"Path".as_slice()).map(|v| v.as_slice()) != Some(b"audio") {
                        return Err((
                            None,
                            None,
                            EdgeTtsError::UnexpectedResponse(
                                "Received binary message, but Path is not audio".to_string(),
                            ),
                        ));
                    }

                    let content_type = parameters
                        .get(b"Content-Type".as_slice())
                        .map(|v| v.as_slice());
                    match content_type {
                        Some(b"audio/mpeg") => {
                            if data.is_empty() {
                                return Err((
                                    None,
                                    None,
                                    EdgeTtsError::UnexpectedResponse(
                                        "Received binary audio/mpeg message with empty audio data"
                                            .to_string(),
                                    ),
                                ));
                            }
                            audio_was_received = true;
                            self.state.chunk_audio_bytes += data.len() as u64;
                            out_chunks.push(TtsChunk::Audio { data });
                        }
                        None => {
                            if data.is_empty() {
                                continue;
                            }
                            return Err((
                                None,
                                None,
                                EdgeTtsError::UnexpectedResponse(
                                    "Received binary message with no Content-Type, but with data"
                                        .to_string(),
                                ),
                            ));
                        }
                        _ => {
                            return Err((
                                None,
                                None,
                                EdgeTtsError::UnexpectedResponse(
                                    "Received binary message with unexpected Content-Type"
                                        .to_string(),
                                ),
                            ));
                        }
                    }
                }
                Message::Close(_) => break,
                _ => {}
            }
        }

        if !audio_was_received {
            return Err((
                None,
                None,
                EdgeTtsError::NoAudioReceived(
                    "No audio was received. Please verify that your parameters are correct."
                        .to_string(),
                ),
            ));
        }

        Ok(out_chunks)
    }

    pub async fn stream(&mut self) -> Result<Vec<TtsChunk>> {
        if self.state.stream_was_called {
            return Err(EdgeTtsError::InvalidConfig(
                "stream can only be called once.".to_string(),
            ));
        }
        self.state.stream_was_called = true;

        let texts = self.texts.clone();
        let mut all_chunks = Vec::new();

        for partial in texts {
            self.state.partial_text = partial;
            let mut attempt = 0u32;
            let mut adjusted_skew = false;

            loop {
                self.state.chunk_audio_bytes = 0;
                match self.stream_single_chunk().await {
                    Ok(chunks) => {
                        all_chunks.extend(chunks);
                        break;
                    }
                    Err((Some(403), server_date, _)) if !adjusted_skew => {
                        Drm::handle_client_response_error(server_date.as_deref())?;
                        adjusted_skew = true;
                    }
                    Err((_, _, err)) => {
                        let is_retryable = matches!(
                            err,
                            EdgeTtsError::WebSocketError(_) | EdgeTtsError::NoAudioReceived(_)
                        );
                        if !is_retryable || attempt >= self.retry_policy.max_retries {
                            return Err(err);
                        }
                        let delay_ms = self.retry_policy.backoff_ms_for_attempt(attempt);
                        tokio::time::sleep(std::time::Duration::from_millis(delay_ms)).await;
                        attempt += 1;
                    }
                }
            }
        }

        Ok(all_chunks)
    }

    pub async fn save(
        &mut self,
        audio_path: impl AsRef<Path>,
        metadata_path: Option<&Path>,
    ) -> Result<()> {
        let chunks = self.stream().await?;
        let mut audio_file = BufWriter::new(File::create(audio_path)?);
        let mut meta_file = match metadata_path {
            Some(p) => Some(BufWriter::new(File::create(p)?)),
            None => None,
        };

        for chunk in chunks {
            match &chunk {
                TtsChunk::Audio { data } => {
                    audio_file.write_all(data)?;
                }
                TtsChunk::WordBoundary { .. } | TtsChunk::SentenceBoundary { .. } => {
                    if let Some(ref mut mf) = meta_file {
                        serde_json::to_writer(&mut *mf, &chunk)?;
                        mf.write_all(b"\n")?;
                    }
                }
            }
        }

        audio_file.flush()?;
        if let Some(ref mut mf) = meta_file {
            mf.flush()?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_remove_incompatible_characters() {
        let input = "Hello\x0bWorld\x00Test\nNext";
        assert_eq!(remove_incompatible_characters(input), "Hello World Test\nNext");
    }

    #[test]
    fn test_split_text_preserves_xml_entities_and_utf8() {
        let text = "Hello &amp; world";

        let chunks = split_text_by_byte_length(text, 9).unwrap();
        assert_eq!(chunks[0], b"Hello");
        assert_eq!(chunks[1], b"&amp;");
        assert_eq!(chunks[2], b"world");
    }
}
