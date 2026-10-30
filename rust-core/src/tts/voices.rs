use reqwest::{Client, StatusCode};
use serde::{Deserialize, Serialize};

use crate::tts::constants::{
    voice_headers, ENGLISH_VOICE_PRESETS, SEC_MS_GEC_VERSION, VOICE_LIST,
};
use crate::tts::drm::Drm;
use crate::tts::exceptions::{EdgeTtsError, Result};
use crate::tts::types::{Voice, VoicesManagerFind, VoicesManagerVoice};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VoicePreset {
    pub short_name: String,
    pub display_name: String,
    pub gender: String,
    pub locale: String,
    pub description: String,
}

pub fn english_voice_presets() -> Vec<VoicePreset> {
    ENGLISH_VOICE_PRESETS
        .iter()
        .map(
            |&(short_name, display_name, gender, locale, description)| VoicePreset {
                short_name: short_name.to_string(),
                display_name: display_name.to_string(),
                gender: gender.to_string(),
                locale: locale.to_string(),
                description: description.to_string(),
            },
        )
        .collect()
}

async fn fetch_voices_once(
    client: &Client,
) -> std::result::Result<Vec<Voice>, (Option<StatusCode>, Option<String>, EdgeTtsError)> {
    let url = format!(
        "{}&Sec-MS-GEC={}&Sec-MS-GEC-Version={}",
        VOICE_LIST,
        Drm::generate_sec_ms_gec(),
        SEC_MS_GEC_VERSION
    );

    let headers_map = Drm::headers_with_muid(&voice_headers());
    let mut req = client.get(&url);
    for (k, v) in headers_map {
        req = req.header(k, v);
    }

    let resp = req
        .send()
        .await
        .map_err(|e| (None, None, EdgeTtsError::HttpError(e)))?;
    let status = resp.status();
    if !status.is_success() {
        let server_date = resp
            .headers()
            .get("Date")
            .and_then(|v| v.to_str().ok())
            .map(|s| s.to_string());
        let err = resp.error_for_status().unwrap_err();
        return Err((Some(status), server_date, EdgeTtsError::HttpError(err)));
    }

    let voices: Vec<Voice> = resp
        .json()
        .await
        .map_err(|e| (Some(status), None, EdgeTtsError::HttpError(e)))?;
    Ok(voices)
}

pub async fn list_voices(proxy: Option<&str>) -> Result<Vec<Voice>> {
    let mut builder = Client::builder();
    if let Some(proxy_url) = proxy {
        let p = reqwest::Proxy::all(proxy_url)?;
        builder = builder.proxy(p);
    }
    let client = builder.build()?;

    match fetch_voices_once(&client).await {
        Ok(voices) => Ok(voices),
        Err((Some(StatusCode::FORBIDDEN), server_date, _)) => {
            Drm::handle_client_response_error(server_date.as_deref())?;
            fetch_voices_once(&client)
                .await
                .map_err(|(_, _, err)| err)
        }
        Err((_, _, err)) => Err(err),
    }
}

pub async fn list_english_voices(proxy: Option<&str>) -> Result<Vec<Voice>> {
    let all = list_voices(proxy).await?;
    let mut en_voices: Vec<Voice> = all
        .into_iter()
        .filter(|v| v.locale.starts_with("en-"))
        .collect();
    en_voices.sort_by(|a, b| a.short_name.cmp(&b.short_name));
    Ok(en_voices)
}

#[derive(Debug, Clone, Default)]
pub struct VoicesManager {
    pub voices: Vec<VoicesManagerVoice>,
    pub called_create: bool,
}

impl VoicesManager {
    pub fn new() -> Self {
        Self::default()
    }

    pub async fn create(custom_voices: Option<Vec<Voice>>) -> Result<Self> {
        let raw_voices = match custom_voices {
            Some(v) => v,
            None => list_voices(None).await?,
        };

        let voices = raw_voices
            .into_iter()
            .map(|v| {
                let language = v
                    .locale
                    .split('-')
                    .next()
                    .unwrap_or_default()
                    .to_string();
                VoicesManagerVoice { voice: v, language }
            })
            .collect();

        Ok(Self {
            voices,
            called_create: true,
        })
    }

    pub fn find(&self, filter: &VoicesManagerFind) -> Result<Vec<VoicesManagerVoice>> {
        if !self.called_create {
            return Err(EdgeTtsError::InvalidConfig(
                "VoicesManager::find() called before VoicesManager::create()".to_string(),
            ));
        }

        let matching = self
            .voices
            .iter()
            .filter(|entry| {
                if let Some(ref g) = filter.gender {
                    if &entry.voice.gender != g {
                        return false;
                    }
                }
                if let Some(ref l) = filter.locale {
                    if &entry.voice.locale != l {
                        return false;
                    }
                }
                if let Some(ref lang) = filter.language {
                    if &entry.language != lang {
                        return false;
                    }
                }
                true
            })
            .cloned()
            .collect();

        Ok(matching)
    }
}
