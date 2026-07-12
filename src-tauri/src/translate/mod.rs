//! Provider trait, request orchestration, cancellation, auto-swap, and the
//! dictionary heuristic. Commands submit here; results leave as events.

pub mod deepl;
pub mod google;
pub mod tts;

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;

use async_trait::async_trait;
use serde_json::json;
use tauri::{AppHandle, Emitter, Manager};

use crate::config::Config;
use crate::types::{DictEntry, Lang, ProviderError, ProviderKind, TranslateRequest, Translation};

#[async_trait]
pub trait TranslationProvider: Send + Sync {
    async fn translate(&self, req: &TranslateRequest) -> Result<Translation, ProviderError>;
    fn supported_langs(&self) -> &[&'static str];
    fn supports_dictionary(&self) -> bool;
}

pub fn provider_for(config: &Config, http: reqwest::Client) -> Box<dyn TranslationProvider> {
    match config.provider {
        ProviderKind::GoogleFree => Box::new(google::GoogleFree::new(http)),
        ProviderKind::DeepL => Box::new(deepl::DeepL::new(
            http,
            config
                .api_keys
                .get(&ProviderKind::DeepL)
                .cloned()
                .unwrap_or_default(),
        )),
    }
}

/// Owns the monotonic request counter and the in-flight task. A new request
/// aborts the previous provider call; the frontend additionally drops stale
/// results by request_id, so both halves of the race are covered.
pub struct Orchestrator {
    counter: AtomicU64,
    inflight: Mutex<Option<tauri::async_runtime::JoinHandle<()>>>,
}

impl Default for Orchestrator {
    fn default() -> Self {
        Self {
            counter: AtomicU64::new(0),
            inflight: Mutex::new(None),
        }
    }
}

impl Orchestrator {
    pub fn submit(&self, app: &AppHandle, text: String, src: Lang, tgt: Lang) -> u64 {
        let request_id = self.counter.fetch_add(1, Ordering::SeqCst) + 1;
        log::debug!(
            "translate #{request_id}: {} chars, {src} -> {tgt}",
            text.len()
        );

        if let Some(previous) = self
            .inflight
            .lock()
            .expect("inflight lock poisoned")
            .take()
        {
            log::debug!("translate #{request_id}: aborting previous in-flight request");
            previous.abort();
        }

        let _ = app.emit("translation:pending", json!({ "request_id": request_id }));

        let app = app.clone();
        let handle = tauri::async_runtime::spawn(async move {
            run_request(app, request_id, text, src, tgt).await;
        });
        *self.inflight.lock().expect("inflight lock poisoned") = Some(handle);
        request_id
    }
}

async fn run_request(app: AppHandle, request_id: u64, text: String, src: Lang, tgt: Lang) {
    let state = app.state::<crate::AppState>();
    let config = state
        .config
        .read()
        .expect("config lock poisoned")
        .clone();
    let provider = provider_for(&config, state.http.clone());
    let want_dictionary = is_dictionary_query(&text) && provider.supports_dictionary();

    let req = TranslateRequest {
        request_id,
        text,
        src,
        tgt,
        want_dictionary,
    };

    match provider.translate(&req).await {
        Ok(mut translation) => {
            translation.request_id = request_id;
            if !want_dictionary {
                translation.alternatives.clear();
                translation.definitions.clear();
            }
            trim_alternatives(&mut translation.alternatives);
            log::debug!(
                "translate #{request_id}: ok, detected {:?}",
                translation.detected_lang
            );
            {
                let mut session = state.session.lock().expect("session lock poisoned");
                session.last_primary = Some(translation.primary.clone());
                session.last_detected = translation.detected_lang.clone();
            }
            let _ = app.emit(
                "translation:result",
                json!({
                    "translation": translation,
                    "provider": config.provider,
                }),
            );
        }
        Err(error) => {
            log::debug!("translation {request_id} failed: {error}");
            let _ = app.emit(
                "translation:error",
                json!({
                    "request_id": request_id,
                    "message": error.user_message(),
                    "retryable": error.retryable(),
                    "kind": error.kind(),
                }),
            );
        }
    }
}

/// Providers return the full long tail of dictionary senses; Google's list
/// spans five orders of magnitude in frequency and ends in archaic curios
/// ("get" as a noun for animal offspring). Keep a term only when it is within
/// 1/2000 of its group's best score, capped per group. Groups without any
/// scores (some providers omit them) are kept as-is, capped.
const MIN_RELATIVE_SCORE: f32 = 5e-4;
const MAX_TERMS_PER_POS: usize = 6;

pub fn trim_alternatives(alternatives: &mut Vec<DictEntry>) {
    for entry in alternatives.iter_mut() {
        let best = entry
            .terms
            .iter()
            .map(|t| t.score)
            .fold(0.0_f32, f32::max);
        if best > 0.0 {
            entry
                .terms
                .retain(|t| t.score >= best * MIN_RELATIVE_SCORE);
        }
        entry.terms.truncate(MAX_TERMS_PER_POS);
    }
    alternatives.retain(|entry| !entry.terms.is_empty());
}

/// A query gets dictionary treatment when it looks like a word or short
/// phrase: at most two whitespace tokens and no sentence punctuation.
pub fn is_dictionary_query(text: &str) -> bool {
    let trimmed = text.trim();
    !trimmed.is_empty()
        && trimmed.split_whitespace().count() <= 2
        && !trimmed.chars().any(|c| ".?!;:".contains(c))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dictionary_heuristic() {
        assert!(is_dictionary_query("schön"));
        assert!(is_dictionary_query("  hello world  "));
        assert!(is_dictionary_query("להתראות"));
        assert!(!is_dictionary_query(""));
        assert!(!is_dictionary_query("   "));
        assert!(!is_dictionary_query("one two three"));
        assert!(!is_dictionary_query("hello."));
        assert!(!is_dictionary_query("what?"));
        assert!(!is_dictionary_query("a;b"));
        assert!(!is_dictionary_query("note: this"));
    }

    #[test]
    fn trims_long_tail_terms_from_recorded_response() {
        // "jongen" nl->en: noun tail holds curios like "get" and "loon" at
        // ~1e-6 vs "boy" at 0.68; the verb group's scores are mutually close.
        let fixture = include_str!("../../fixtures/google/word_nl_en.json");
        let mut t = google::parse_response(fixture, 1).unwrap();
        trim_alternatives(&mut t.alternatives);

        let noun: Vec<&str> = t.alternatives[0]
            .terms
            .iter()
            .map(|s| s.term.as_str())
            .collect();
        assert_eq!(noun, vec!["boy", "lad", "pup"]);
        let verb: Vec<&str> = t.alternatives[1]
            .terms
            .iter()
            .map(|s| s.term.as_str())
            .collect();
        assert_eq!(verb, vec!["breed", "kitten", "whelp"]);
    }

    #[test]
    fn trim_caps_group_size_and_keeps_unscored_groups() {
        use crate::types::ScoredTerm;
        let mut alternatives = vec![DictEntry {
            pos: "noun".into(),
            terms: (0..10)
                .map(|i| ScoredTerm {
                    term: format!("t{i}"),
                    score: 0.0,
                })
                .collect(),
        }];
        trim_alternatives(&mut alternatives);
        // No scores at all: nothing to rank by, just cap the list.
        assert_eq!(alternatives[0].terms.len(), MAX_TERMS_PER_POS);
    }

    #[test]
    fn trim_drops_groups_left_empty() {
        use crate::types::ScoredTerm;
        let mut alternatives = vec![DictEntry {
            pos: "noun".into(),
            terms: vec![
                ScoredTerm {
                    term: "top".into(),
                    score: 1.0,
                },
                ScoredTerm {
                    term: "dust".into(),
                    score: 1e-7,
                },
            ],
        }];
        trim_alternatives(&mut alternatives);
        assert_eq!(alternatives[0].terms.len(), 1);
        assert_eq!(alternatives[0].terms[0].term, "top");
    }
}
