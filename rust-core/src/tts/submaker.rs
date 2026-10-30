use std::fmt;
use std::time::Duration;

use crate::tts::exceptions::{EdgeTtsError, Result};
use crate::tts::srt_composer::{compose, Subtitle};
use crate::tts::types::{BoundaryType, TtsChunk};

#[derive(Debug, Clone, Default)]
pub struct SubMaker {
    pub cues: Vec<Subtitle>,
    pub boundary_type: Option<BoundaryType>,
}

impl SubMaker {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn feed(&mut self, msg: &TtsChunk) -> Result<()> {
        let (msg_type, offset, duration, text) = match msg {
            TtsChunk::WordBoundary {
                offset,
                duration,
                text,
            } => (BoundaryType::WordBoundary, *offset, *duration, text.as_str()),
            TtsChunk::SentenceBoundary {
                offset,
                duration,
                text,
            } => (
                BoundaryType::SentenceBoundary,
                *offset,
                *duration,
                text.as_str(),
            ),
            TtsChunk::Audio { .. } => {
                return Err(EdgeTtsError::InvalidConfig(
                    "Invalid message type, expected 'WordBoundary' or 'SentenceBoundary'."
                        .to_string(),
                ));
            }
        };

        match self.boundary_type {
            None => self.boundary_type = Some(msg_type),
            Some(existing) if existing != msg_type => {
                return Err(EdgeTtsError::InvalidConfig(format!(
                    "Expected message type '{}', but got '{}'.",
                    existing.as_str(),
                    msg_type.as_str()
                )));
            }
            _ => {}
        }

        let start = Duration::from_nanos(offset.saturating_mul(100));
        let end = Duration::from_nanos(offset.saturating_add(duration).saturating_mul(100));

        self.cues.push(Subtitle::new(
            Some(self.cues.len() + 1),
            start,
            end,
            text.to_string(),
        ));
        Ok(())
    }

    pub fn get_srt(&self) -> String {
        compose(&self.cues, true, 1, None)
    }
}

impl fmt::Display for SubMaker {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.get_srt())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_submaker_generates_srt_and_rejects_mixed_boundaries() {
        let mut sm = SubMaker::new();
        sm.feed(&TtsChunk::WordBoundary {
            offset: 10_000_000,
            duration: 5_000_000,
            text: "First".to_string(),
        })
        .unwrap();
        sm.feed(&TtsChunk::WordBoundary {
            offset: 16_000_000,
            duration: 4_000_000,
            text: "Second".to_string(),
        })
        .unwrap();

        let srt = sm.get_srt();
        assert!(srt.contains("00:00:01,000 --> 00:00:01,500"));
        assert!(srt.contains("First"));
        assert!(srt.contains("00:00:01,600 --> 00:00:02,000"));
        assert!(srt.contains("Second"));

        let mixed_err = sm.feed(&TtsChunk::SentenceBoundary {
            offset: 20_000_000,
            duration: 5_000_000,
            text: "Mixed".to_string(),
        });
        assert!(mixed_err.is_err());

        let audio_err = sm.feed(&TtsChunk::Audio {
            data: vec![1, 2, 3],
        });
        assert!(audio_err.is_err());
    }
}
