use std::collections::HashMap;

pub const BASE_URL: &str = "speech.platform.bing.com/consumer/speech/synthesize/readaloud";
pub const TRUSTED_CLIENT_TOKEN: &str = "6A5AA1D4EAFF4E9FB37E23D68491D6F4";

pub const WSS_URL: &str = "wss://speech.platform.bing.com/consumer/speech/synthesize/readaloud/edge/v1?TrustedClientToken=6A5AA1D4EAFF4E9FB37E23D68491D6F4";
pub const VOICE_LIST: &str = "https://speech.platform.bing.com/consumer/speech/synthesize/readaloud/voices/list?trustedclienttoken=6A5AA1D4EAFF4E9FB37E23D68491D6F4";

pub const DEFAULT_VOICE: &str = "en-US-EmmaMultilingualNeural";

pub const ENGLISH_VOICE_PRESETS: &[(&str, &str, &str, &str, &str)] = &[
    (
        "it-IT-IsabellaNeural",
        "Isabella (Italiano)",
        "Female",
        "it-IT",
        "Voce femminile calda, espressiva e naturale per romanzi",
    ),
    (
        "it-IT-DiegoNeural",
        "Diego (Italiano)",
        "Male",
        "it-IT",
        "Voce maschile profonda e chiara per narrativa e saggistica",
    ),
    (
        "it-IT-ElsaNeural",
        "Elsa (Italiano)",
        "Female",
        "it-IT",
        "Voce femminile nitida, scorrevole e articolata",
    ),
    (
        "it-IT-GiuseppeMultilingualNeural",
        "Giuseppe (Italiano Multilingual)",
        "Male",
        "it-IT",
        "Voce maschile naturale multilingua (Italiano / Inglese)",
    ),
    (
        "en-US-AvaMultilingualNeural",
        "Ava (English US)",
        "Female",
        "en-US",
        "Expressive, warm American storytelling voice",
    ),
    (
        "en-US-AndrewMultilingualNeural",
        "Andrew (English US)",
        "Male",
        "en-US",
        "Warm, confident American narrator voice",
    ),
    (
        "en-GB-SoniaNeural",
        "Sonia (English UK)",
        "Female",
        "en-GB",
        "Clear, classic British female voice",
    ),
    (
        "en-GB-RyanNeural",
        "Ryan (English UK)",
        "Male",
        "en-GB",
        "Natural British male narrator voice",
    ),
];

pub const CHROMIUM_FULL_VERSION: &str = "143.0.3650.75";
pub const CHROMIUM_MAJOR_VERSION: &str = "143";
pub const SEC_MS_GEC_VERSION: &str = "2-143.0.3650.75";

pub const TICKS_PER_SECOND: u64 = 10_000_000;
pub const MP3_BITRATE_BPS: u64 = 48_000;

pub fn base_headers() -> HashMap<String, String> {
    let mut headers = HashMap::new();
    headers.insert(
        "User-Agent".to_string(),
        format!(
            "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/{major}.0.0.0 Safari/537.36 Edg/{major}.0.0.0",
            major = CHROMIUM_MAJOR_VERSION
        ),
    );
    headers.insert(
        "Accept-Encoding".to_string(),
        "gzip, deflate, br, zstd".to_string(),
    );
    headers.insert("Accept-Language".to_string(), "en-US,en;q=0.9".to_string());
    headers
}

pub fn wss_headers() -> HashMap<String, String> {
    let mut headers = base_headers();
    headers.insert("Pragma".to_string(), "no-cache".to_string());
    headers.insert("Cache-Control".to_string(), "no-cache".to_string());
    headers.insert(
        "Origin".to_string(),
        "chrome-extension://jdiccldimpdaibmpdkjnbmckianbfold".to_string(),
    );
    headers.insert("Sec-WebSocket-Version".to_string(), "13".to_string());
    headers
}

pub fn voice_headers() -> HashMap<String, String> {
    let mut headers = base_headers();
    headers.insert(
        "Authority".to_string(),
        "speech.platform.bing.com".to_string(),
    );
    headers.insert(
        "Sec-CH-UA".to_string(),
        format!(
            "\" Not;A Brand\";v=\"99\", \"Microsoft Edge\";v=\"{major}\", \"Chromium\";v=\"{major}\"",
            major = CHROMIUM_MAJOR_VERSION
        ),
    );
    headers.insert("Sec-CH-UA-Mobile".to_string(), "?0".to_string());
    headers.insert("Accept".to_string(), "*/*".to_string());
    headers.insert("Sec-Fetch-Site".to_string(), "none".to_string());
    headers.insert("Sec-Fetch-Mode".to_string(), "cors".to_string());
    headers.insert("Sec-Fetch-Dest".to_string(), "empty".to_string());
    headers
}
