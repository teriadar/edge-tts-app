use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use futures_util::SinkExt;
use serde::{Deserialize, Serialize};
use tokio::time::timeout;
use tokio_tungstenite::connect_async;
use tokio_tungstenite::tungstenite::client::IntoClientRequest;
use tokio_tungstenite::tungstenite::http::HeaderValue;
use tokio_tungstenite::tungstenite::protocol::Message;
use tokio_tungstenite::tungstenite::Error as WsError;

use crate::tts::communicate::connect_id;
use crate::tts::constants::{wss_headers, SEC_MS_GEC_VERSION, WSS_URL};
use crate::tts::drm::Drm;
use crate::tts::voices::list_voices;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ServiceStatus {

    Healthy,

    Degraded,

    Offline,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EdgeHealthReport {
    pub status: ServiceStatus,
    pub is_operational: bool,
    pub voices_api_reachable: bool,
    pub websocket_handshake_ok: bool,
    pub http_latency_ms: Option<u64>,
    pub ws_handshake_latency_ms: Option<u64>,
    pub available_voices_count: usize,
    pub english_voices_count: usize,
    pub clock_skew_seconds: f64,
    pub sec_ms_gec_version: String,

    pub message: String,
    pub checked_at_unix_ms: u64,
}

fn current_unix_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

async fn probe_websocket_handshake() -> std::result::Result<u64, String> {
    let start = Instant::now();
    match try_ws_handshake_once().await {
        Ok(()) => Ok(start.elapsed().as_millis() as u64),
        Err((Some(403), server_date, _)) => {
            Drm::handle_client_response_error(server_date.as_deref())
                .map_err(|e| e.to_string())?;
            let retry_start = Instant::now();
            try_ws_handshake_once()
                .await
                .map(|()| retry_start.elapsed().as_millis() as u64)
                .map_err(|(_, _, err)| err)
        }
        Err((_, _, err)) => Err(err),
    }
}

async fn try_ws_handshake_once() -> std::result::Result<(), (Option<u16>, Option<String>, String)> {
    let url = format!(
        "{}&ConnectionId={}&Sec-MS-GEC={}&Sec-MS-GEC-Version={}",
        WSS_URL,
        connect_id(),
        Drm::generate_sec_ms_gec(),
        SEC_MS_GEC_VERSION
    );

    let mut request = url
        .into_client_request()
        .map_err(|e| (None, None, format!("Invalid WS request: {}", e)))?;

    let custom_headers = Drm::headers_with_muid(&wss_headers());
    for (k, v) in custom_headers {
        if let (Ok(header_name), Ok(header_val)) = (
            tokio_tungstenite::tungstenite::http::HeaderName::try_from(k.as_str()),
            HeaderValue::from_str(&v),
        ) {
            request.headers_mut().insert(header_name, header_val);
        }
    }

    match connect_async(request).await {
        Ok((mut ws, _)) => {
            let _ = ws.send(Message::Close(None)).await;
            Ok(())
        }
        Err(WsError::Http(resp)) => {
            let status = resp.status().as_u16();
            let date_hdr = resp
                .headers()
                .get("Date")
                .and_then(|v| v.to_str().ok())
                .map(|s| s.to_string());
            Err((
                Some(status),
                date_hdr,
                format!("HTTP {} during WebSocket handshake", status),
            ))
        }
        Err(e) => Err((None, None, e.to_string())),
    }
}

pub async fn check_edge_health(timeout_ms: u64) -> EdgeHealthReport {
    let limit = Duration::from_millis(timeout_ms.max(1000));

    let http_start = Instant::now();
    let voices_res = timeout(limit, list_voices(None)).await;
    let (voices_ok, http_latency_ms, total_voices, en_voices, http_err) = match voices_res {
        Ok(Ok(voices)) => {
            let elapsed = http_start.elapsed().as_millis() as u64;
            let en_count = voices
                .iter()
                .filter(|v| v.locale.starts_with("en-"))
                .count();
            (true, Some(elapsed), voices.len(), en_count, None)
        }
        Ok(Err(e)) => (false, None, 0, 0, Some(e.to_string())),
        Err(_) => (
            false,
            None,
            0,
            0,
            Some("HTTPS voices endpoint timed out".to_string()),
        ),
    };

    let ws_res = timeout(limit, probe_websocket_handshake()).await;
    let (ws_ok, ws_latency_ms, ws_err) = match ws_res {
        Ok(Ok(ms)) => (true, Some(ms), None),
        Ok(Err(e)) => (false, None, Some(e)),
        Err(_) => (false, None, Some("WebSocket handshake timed out".to_string())),
    };

    let status = if ws_ok && voices_ok {
        let slow = ws_latency_ms.unwrap_or(0) > 1500 || http_latency_ms.unwrap_or(0) > 2500;
        if slow {
            ServiceStatus::Degraded
        } else {
            ServiceStatus::Healthy
        }
    } else if ws_ok {
        ServiceStatus::Degraded
    } else {
        ServiceStatus::Offline
    };

    let message = match status {
        ServiceStatus::Healthy => format!(
            "Edge-TTS service is active (WSS: {}ms, HTTPS: {}ms, {} neural voices online)",
            ws_latency_ms.unwrap_or(0),
            http_latency_ms.unwrap_or(0),
            total_voices
        ),
        ServiceStatus::Degraded => {
            if ws_ok && !voices_ok {
                format!(
                    "WebSocket synthesis is active ({}ms), but voice list endpoint is unreachable ({})",
                    ws_latency_ms.unwrap_or(0),
                    http_err.unwrap_or_default()
                )
            } else {
                format!(
                    "Edge-TTS service is experiencing high latency (WSS: {}ms, HTTPS: {}ms)",
                    ws_latency_ms.unwrap_or(0),
                    http_latency_ms.unwrap_or(0)
                )
            }
        }
        ServiceStatus::Offline => format!(
            "Edge-TTS service is unreachable: {}",
            ws_err
                .or(http_err)
                .unwrap_or_else(|| "Unknown network error".to_string())
        ),
    };

    EdgeHealthReport {
        status,
        is_operational: ws_ok,
        voices_api_reachable: voices_ok,
        websocket_handshake_ok: ws_ok,
        http_latency_ms,
        ws_handshake_latency_ms: ws_latency_ms,
        available_voices_count: total_voices,
        english_voices_count: en_voices,
        clock_skew_seconds: Drm::clock_skew_seconds(),
        sec_ms_gec_version: SEC_MS_GEC_VERSION.to_string(),
        message,
        checked_at_unix_ms: current_unix_ms(),
    }
}

pub async fn quick_ws_ping(timeout_ms: u64) -> EdgeHealthReport {
    let limit = Duration::from_millis(timeout_ms.max(500));
    let ws_res = timeout(limit, probe_websocket_handshake()).await;

    let (ws_ok, ws_latency_ms, ws_err) = match ws_res {
        Ok(Ok(ms)) => (true, Some(ms), None),
        Ok(Err(e)) => (false, None, Some(e)),
        Err(_) => (false, None, Some("WebSocket handshake timed out".to_string())),
    };

    let status = if ws_ok {
        if ws_latency_ms.unwrap_or(0) > 1500 {
            ServiceStatus::Degraded
        } else {
            ServiceStatus::Healthy
        }
    } else {
        ServiceStatus::Offline
    };

    let message = if ws_ok {
        format!(
            "Edge-TTS WebSocket online ({}ms)",
            ws_latency_ms.unwrap_or(0)
        )
    } else {
        format!(
            "Edge-TTS WebSocket offline: {}",
            ws_err.unwrap_or_else(|| "Network error".to_string())
        )
    };

    EdgeHealthReport {
        status,
        is_operational: ws_ok,
        voices_api_reachable: false,
        websocket_handshake_ok: ws_ok,
        http_latency_ms: None,
        ws_handshake_latency_ms: ws_latency_ms,
        available_voices_count: 0,
        english_voices_count: 0,
        clock_skew_seconds: Drm::clock_skew_seconds(),
        sec_ms_gec_version: SEC_MS_GEC_VERSION.to_string(),
        message,
        checked_at_unix_ms: current_unix_ms(),
    }
}
