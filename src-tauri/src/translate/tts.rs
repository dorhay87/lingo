//! Unofficial Google text-to-speech endpoint; returns MP3, capped at
//! MAX_TTS_CHARS per request.

use crate::types::{normalize_lang, ProviderError};

const ENDPOINT: &str = "https://translate.google.com/translate_tts";
const USER_AGENT: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/126.0.0.0 Safari/537.36";
pub const MAX_TTS_CHARS: usize = 200;

pub fn clip_text(text: &str) -> &str {
    match text.char_indices().nth(MAX_TTS_CHARS) {
        Some((byte_index, _)) => &text[..byte_index],
        None => text,
    }
}

pub fn query_params(text: &str, lang: &str) -> Vec<(&'static str, String)> {
    vec![
        ("ie", "UTF-8".into()),
        ("client", "tw-ob".into()),
        ("tl", normalize_lang(lang)),
        ("q", clip_text(text).to_string()),
    ]
}

pub async fn fetch_speech(
    http: reqwest::Client,
    text: &str,
    lang: &str,
) -> Result<Vec<u8>, ProviderError> {
    let response = http
        .get(ENDPOINT)
        .header(reqwest::header::USER_AGENT, USER_AGENT)
        .query(&query_params(text, lang))
        .send()
        .await
        .map_err(|e| ProviderError::Network(e.to_string()))?;

    let status = response.status();
    if status.as_u16() == 429 {
        return Err(ProviderError::RateLimited);
    }
    if !status.is_success() {
        return Err(ProviderError::Network(format!("http {status}")));
    }

    let bytes = response
        .bytes()
        .await
        .map_err(|e| ProviderError::Network(e.to_string()))?;
    if bytes.is_empty() {
        return Err(ProviderError::Network("empty audio response".into()));
    }
    Ok(bytes.to_vec())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn params_normalize_lang_and_carry_text() {
        let params = query_params("shalom", "iw");
        assert!(params.contains(&("tl", "he".to_string())));
        assert!(params.contains(&("q", "shalom".to_string())));
        assert!(params.contains(&("client", "tw-ob".to_string())));
    }

    #[test]
    fn clips_to_budget_on_char_boundary() {
        let long = "a".repeat(300);
        assert_eq!(clip_text(&long).chars().count(), MAX_TTS_CHARS);

        let hebrew = "\u{05D0}".repeat(300);
        assert_eq!(clip_text(&hebrew).chars().count(), MAX_TTS_CHARS);

        assert_eq!(clip_text("hello"), "hello");
    }
}
