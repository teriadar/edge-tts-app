use thiserror::Error;

#[derive(Error, Debug)]
pub enum EdgeTtsError {

    #[error("Unknown response from server: {0}")]
    UnknownResponse(String),

    #[error("Unexpected response from server: {0}")]
    UnexpectedResponse(String),

    #[error("No audio was received from the server: {0}")]
    NoAudioReceived(String),

    #[error("WebSocket error: {0}")]
    WebSocketError(String),

    #[error("Clock skew adjustment error: {0}")]
    SkewAdjustmentError(String),

    #[error("Invalid parameter or configuration: {0}")]
    InvalidConfig(String),

    #[error("HTTP error: {0}")]
    HttpError(#[from] reqwest::Error),

    #[error("I/O error: {0}")]
    IoError(#[from] std::io::Error),

    #[error("JSON error: {0}")]
    JsonError(#[from] serde_json::Error),
}

pub type Result<T> = std::result::Result<T, EdgeTtsError>;
