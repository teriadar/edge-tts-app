use regex::Regex;
use serde::{Deserialize, Serialize};

use crate::tts::constants::DEFAULT_VOICE;
use crate::tts::exceptions::{EdgeTtsError, Result};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum BoundaryType {
    WordBoundary,
    SentenceBoundary,
}

impl BoundaryType {
    pub fn as_str(&self) -> &'static str {
        match self {
            BoundaryType::WordBoundary => "WordBoundary",
            BoundaryType::SentenceBoundary => "SentenceBoundary",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum TtsChunk {
    #[serde(rename = "audio")]
    Audio { data: Vec<u8> },
    #[serde(rename = "WordBoundary")]
    WordBoundary {

        offset: u64,

        duration: u64,

        text: String,
    },
    #[serde(rename = "SentenceBoundary")]
    SentenceBoundary {

        offset: u64,

        duration: u64,

        text: String,
    },
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct VoiceTag {
    #[serde(default)]
    pub content_categories: Vec<String>,
    #[serde(default)]
    pub voice_personalities: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct Voice {
    pub name: String,
    pub short_name: String,
    pub gender: String,
    pub locale: String,
    pub suggested_codec: String,
    pub friendly_name: String,
    pub status: String,
    #[serde(default)]
    pub voice_tag: VoiceTag,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VoicesManagerVoice {
    pub voice: Voice,
    pub language: String,
}

#[derive(Debug, Clone, Default)]
pub struct VoicesManagerFind {
    pub gender: Option<String>,
    pub locale: Option<String>,
    pub language: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub struct CommunicateState {
    pub partial_text: Vec<u8>,
    pub offset_compensation: u64,
    pub last_duration_offset: u64,
    pub stream_was_called: bool,
    pub chunk_audio_bytes: u64,
    pub cumulative_audio_bytes: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TtsConfig {
    pub voice: String,
    pub rate: String,
    pub volume: String,
    pub pitch: String,
    pub boundary: BoundaryType,
}

impl Default for TtsConfig {
    fn default() -> Self {
        Self::new(
            DEFAULT_VOICE,
            "+0%",
            "+0%",
            "+0Hz",
            BoundaryType::SentenceBoundary,
        )
        .expect("default TtsConfig must be valid")
    }
}

impl TtsConfig {

    pub fn new(
        voice: &str,
        rate: &str,
        volume: &str,
        pitch: &str,
        boundary: BoundaryType,
    ) -> Result<Self> {
        let mut normalized_voice = voice.to_string();

        let short_voice_re = Regex::new(r"^([a-z]{2,})-([A-Z]{2,})-(.+Neural)$").unwrap();
        if let Some(caps) = short_voice_re.captures(voice) {
            let lang = &caps[1];
            let mut region = caps[2].to_string();
            let mut name = caps[3].to_string();
            if let Some(dash_idx) = name.find('-') {
                region = format!("{}-{}", region, &name[..dash_idx]);
                name = name[dash_idx + 1..].to_string();
            }
            normalized_voice = format!(
                "Microsoft Server Speech Text to Speech Voice ({}-{}, {})",
                lang, region, name
            );
        }

        Self::validate_string_param(
            "voice",
            &normalized_voice,
            r"^Microsoft Server Speech Text to Speech Voice \(.+,.+\)$",
        )?;
        Self::validate_string_param("rate", rate, r"^[+-]\d+%$")?;
        Self::validate_string_param("volume", volume, r"^[+-]\d+%$")?;
        Self::validate_string_param("pitch", pitch, r"^[+-]\d+Hz$")?;

        Ok(Self {
            voice: normalized_voice,
            rate: rate.to_string(),
            volume: volume.to_string(),
            pitch: pitch.to_string(),
            boundary,
        })
    }

    pub fn validate_string_param<'a>(
        param_name: &str,
        param_value: &'a str,
        pattern: &str,
    ) -> Result<&'a str> {
        let re = Regex::new(pattern).map_err(|e| EdgeTtsError::InvalidConfig(e.to_string()))?;
        if !re.is_match(param_value) {
            return Err(EdgeTtsError::InvalidConfig(format!(
                "Invalid {} '{}'.",
                param_name, param_value
            )));
        }
        Ok(param_value)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tts_config_short_voice_normalization_and_validation() {
        let cfg = TtsConfig::new(
            "en-US-AvaMultilingualNeural",
            "+15%",
            "-10%",
            "-2Hz",
            BoundaryType::WordBoundary,
        )
        .unwrap();
        assert_eq!(
            cfg.voice,
            "Microsoft Server Speech Text to Speech Voice (en-US, AvaMultilingualNeural)"
        );
        assert_eq!(cfg.rate, "+15%");
        assert_eq!(cfg.volume, "-10%");
        assert_eq!(cfg.pitch, "-2Hz");

        assert!(TtsConfig::new("invalid-voice", "+0%", "+0%", "+0Hz", BoundaryType::WordBoundary).is_err());
        assert!(TtsConfig::new("en-US-AvaMultilingualNeural", "15%", "+0%", "+0Hz", BoundaryType::WordBoundary).is_err());
        assert!(TtsConfig::new("en-US-AvaMultilingualNeural", "+0%", "+0%", "2Hz", BoundaryType::WordBoundary).is_err());
    }
}
