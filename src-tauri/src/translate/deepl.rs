//! DeepL REST API (official, BYO key). No dictionary data: alternatives and
//! definitions stay empty and the popup collapses those sections.

use async_trait::async_trait;

use super::TranslationProvider;
use crate::types::{normalize_lang, ProviderError, TranslateRequest, Translation};

const FREE_ENDPOINT: &str = "https://api-free.deepl.com/v2/translate";
const PRO_ENDPOINT: &str = "https://api.deepl.com/v2/translate";

pub struct DeepL {
    http: reqwest::Client,
    api_key: String,
}

impl DeepL {
    pub fn new(http: reqwest::Client, api_key: String) -> Self {
        Self { http, api_key }
    }

    fn endpoint(&self) -> &'static str {
        // Free-tier keys are suffixed ":fx".
        if self.api_key.ends_with(":fx") {
            FREE_ENDPOINT
        } else {
            PRO_ENDPOINT
        }
    }
}

#[async_trait]
impl TranslationProvider for DeepL {
    async fn translate(&self, req: &TranslateRequest) -> Result<Translation, ProviderError> {
        if self.api_key.trim().is_empty() {
            return Err(ProviderError::AuthFailed);
        }

        let mut body = serde_json::json!({
            "text": [req.text],
            "target_lang": to_deepl_lang(&req.tgt),
        });
        if req.src != "auto" {
            body["source_lang"] = serde_json::Value::String(to_deepl_lang(&req.src));
        }

        let response = self
            .http
            .post(self.endpoint())
            .header(
                reqwest::header::AUTHORIZATION,
                format!("DeepL-Auth-Key {}", self.api_key),
            )
            .json(&body)
            .send()
            .await
            .map_err(|e| ProviderError::Network(e.to_string()))?;

        if let Some(error) = map_status(response.status().as_u16()) {
            return Err(error);
        }

        let text = response
            .text()
            .await
            .map_err(|e| ProviderError::Network(e.to_string()))?;
        parse_response(&text, req.request_id)
    }

    fn supported_langs(&self) -> &[&'static str] {
        DEEPL_LANGS
    }

    fn supports_dictionary(&self) -> bool {
        false
    }
}

/// DeepL wants uppercase codes; regional variants keep their primary subtag
/// except Chinese, which DeepL only accepts as "ZH".
fn to_deepl_lang(code: &str) -> String {
    let normalized = normalize_lang(code);
    let primary = normalized.split('-').next().unwrap_or(&normalized);
    if primary == "zh" {
        return "ZH".into();
    }
    primary.to_ascii_uppercase()
}

/// 403 (and 401) map to AuthFailed, 456 is DeepL's quota-exceeded code, 429 is
/// plain rate limiting. Anything else non-2xx is a network-class failure.
fn map_status(status: u16) -> Option<ProviderError> {
    match status {
        200..=299 => None,
        401 | 403 => Some(ProviderError::AuthFailed),
        429 | 456 => Some(ProviderError::RateLimited),
        code => Some(ProviderError::Network(format!("http {code}"))),
    }
}

fn parse_response(body: &str, request_id: u64) -> Result<Translation, ProviderError> {
    #[derive(serde::Deserialize)]
    struct Response {
        translations: Vec<Entry>,
    }
    #[derive(serde::Deserialize)]
    struct Entry {
        detected_source_language: Option<String>,
        text: String,
    }

    let parsed: Response = serde_json::from_str(body).map_err(|_| {
        log::debug!("deepl response unparseable, raw body: {body}");
        ProviderError::Network("unexpected response from translator".into())
    })?;
    let entry = parsed
        .translations
        .into_iter()
        .next()
        .ok_or_else(|| ProviderError::Network("empty translations list".into()))?;

    Ok(Translation {
        request_id,
        primary: entry.text,
        alternatives: vec![],
        definitions: vec![],
        detected_lang: entry.detected_source_language.map(|c| normalize_lang(&c)),
    })
}

static DEEPL_LANGS: &[&str] = &[
    "ar", "bg", "cs", "da", "de", "el", "en", "es", "et", "fi", "fr", "he", "hu", "id", "it",
    "ja", "ko", "lt", "lv", "nb", "nl", "pl", "pt", "ro", "ru", "sk", "sl", "sv", "th", "tr",
    "uk", "vi", "zh-CN",
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_documented_response_shape() {
        let body = r#"{"translations":[{"detected_source_language":"DE","text":"beautiful"}]}"#;
        let t = parse_response(body, 9).unwrap();
        assert_eq!(t.request_id, 9);
        assert_eq!(t.primary, "beautiful");
        assert_eq!(t.detected_lang.as_deref(), Some("de"));
        assert!(t.alternatives.is_empty());
        assert!(t.definitions.is_empty());
    }

    #[test]
    fn garbage_body_is_network_error() {
        assert!(matches!(
            parse_response("<html>oops</html>", 1),
            Err(ProviderError::Network(_))
        ));
        assert!(matches!(
            parse_response(r#"{"translations":[]}"#, 1),
            Err(ProviderError::Network(_))
        ));
    }

    #[test]
    fn status_mapping_matches_deepl_docs() {
        assert!(map_status(200).is_none());
        assert!(matches!(map_status(401), Some(ProviderError::AuthFailed)));
        assert!(matches!(map_status(403), Some(ProviderError::AuthFailed)));
        assert!(matches!(map_status(456), Some(ProviderError::RateLimited)));
        assert!(matches!(map_status(429), Some(ProviderError::RateLimited)));
        assert!(matches!(map_status(500), Some(ProviderError::Network(_))));
    }

    #[test]
    fn retryability_of_mapped_errors() {
        assert!(!map_status(403).unwrap().retryable());
        assert!(map_status(456).unwrap().retryable());
        assert!(map_status(503).unwrap().retryable());
    }

    #[test]
    fn lang_codes_are_uppercased_for_deepl() {
        assert_eq!(to_deepl_lang("en"), "EN");
        assert_eq!(to_deepl_lang("he"), "HE");
        assert_eq!(to_deepl_lang("iw"), "HE");
        assert_eq!(to_deepl_lang("zh-CN"), "ZH");
        assert_eq!(to_deepl_lang("pt"), "PT");
    }
}
