use serde::{Deserialize, Serialize};

pub type Lang = String;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ProviderKind {
    GoogleFree,
    DeepL,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Theme {
    System,
    Light,
    Dark,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ScoredTerm {
    pub term: String,
    /// Relative frequency 0..1; 0 when the provider gave no score.
    pub score: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DictEntry {
    pub pos: String,
    pub terms: Vec<ScoredTerm>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Definition {
    pub pos: String,
    pub text: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Translation {
    pub request_id: u64,
    pub primary: String,
    pub alternatives: Vec<DictEntry>,
    pub definitions: Vec<Definition>,
    pub detected_lang: Option<Lang>,
}

#[derive(Debug, Clone)]
pub struct TranslateRequest {
    pub request_id: u64,
    pub text: String,
    /// "auto" or a language code.
    pub src: Lang,
    pub tgt: Lang,
    /// When false, providers may skip dictionary work entirely.
    pub want_dictionary: bool,
}

#[derive(Debug, thiserror::Error)]
pub enum ProviderError {
    #[error("couldn't reach translator: {0}")]
    Network(String),
    #[error("provider rate limit hit")]
    RateLimited,
    #[error("provider rejected the API key")]
    AuthFailed,
}

impl ProviderError {
    pub fn retryable(&self) -> bool {
        matches!(self, ProviderError::Network(_) | ProviderError::RateLimited)
    }

    /// Stable machine-readable kind for the frontend.
    pub fn kind(&self) -> &'static str {
        match self {
            ProviderError::Network(_) => "network",
            ProviderError::RateLimited => "rate_limited",
            ProviderError::AuthFailed => "auth_failed",
        }
    }

    /// Short human message rendered in the popup.
    pub fn user_message(&self) -> String {
        match self {
            ProviderError::Network(_) => "Couldn't reach translator".into(),
            ProviderError::RateLimited => "Rate limited, try again in a moment".into(),
            ProviderError::AuthFailed => "API key was rejected".into(),
        }
    }
}

impl Serialize for ProviderError {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeStruct;
        let mut s = serializer.serialize_struct("ProviderError", 2)?;
        s.serialize_field("kind", self.kind())?;
        s.serialize_field("message", &self.user_message())?;
        s.end()
    }
}

/// Normalize a provider-reported language code to the canonical form used in
/// config. Google still reports legacy ISO codes for a few languages.
pub fn normalize_lang(code: &str) -> String {
    let lower = code.trim().to_ascii_lowercase();
    match lower.as_str() {
        "iw" => "he".into(),
        "ji" => "yi".into(),
        "in" => "id".into(),
        _ => lower,
    }
}

pub fn langs_equal(a: &str, b: &str) -> bool {
    normalize_lang(a) == normalize_lang(b)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_legacy_codes() {
        assert_eq!(normalize_lang("iw"), "he");
        assert_eq!(normalize_lang("IW"), "he");
        assert_eq!(normalize_lang("in"), "id");
        assert_eq!(normalize_lang("ji"), "yi");
        assert_eq!(normalize_lang("EN"), "en");
        assert_eq!(normalize_lang(" nl "), "nl");
    }

    #[test]
    fn lang_equality_ignores_case_and_aliases() {
        assert!(langs_equal("he", "iw"));
        assert!(langs_equal("EN", "en"));
        assert!(!langs_equal("en", "nl"));
    }

    #[test]
    fn provider_error_retryability() {
        assert!(ProviderError::Network("x".into()).retryable());
        assert!(ProviderError::RateLimited.retryable());
        assert!(!ProviderError::AuthFailed.retryable());
    }
}
