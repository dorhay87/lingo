//! Unofficial Google endpoint. The response is a positional JSON array where
//! every index may be null or absent, so all parsing here is defensive and
//! exercised by recorded fixtures in fixtures/google/.

use crate::types::{
    normalize_lang, Definition, DictEntry, ProviderError, ScoredTerm, TranslateRequest,
    Translation,
};

const ENDPOINT: &str = "https://translate.googleapis.com/translate_a/single";
const USER_AGENT: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/126.0.0.0 Safari/537.36";

pub async fn translate(
    http: reqwest::Client,
    req: &TranslateRequest,
) -> Result<Translation, ProviderError> {
    let mut params = vec![
        ("client", "gtx"),
        ("sl", req.src.as_str()),
        ("tl", req.tgt.as_str()),
        ("dt", "t"),
    ];
    if req.want_dictionary {
        params.extend([("dt", "bd"), ("dt", "md"), ("dt", "at")]);
    }
    params.push(("q", req.text.as_str()));

    let response = http
        .get(ENDPOINT)
        .header(reqwest::header::USER_AGENT, USER_AGENT)
        .query(&params)
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

    let body = response
        .text()
        .await
        .map_err(|e| ProviderError::Network(e.to_string()))?;
    parse_response(&body, req.request_id)
}

fn parse_failure(body: &str) -> ProviderError {
    log::debug!("google response unparseable, raw body: {body}");
    ProviderError::Network("unexpected response from translator".into())
}

/// Parse the positional array. `[0]` sentence segments, `[1]` dictionary,
/// `[2]` detected language; definitions appear at a variable trailing index
/// and are located by shape.
pub fn parse_response(body: &str, request_id: u64) -> Result<Translation, ProviderError> {
    let value: serde_json::Value =
        serde_json::from_str(body).map_err(|_| parse_failure(body))?;
    let root = value.as_array().ok_or_else(|| parse_failure(body))?;

    let primary: String = root
        .first()
        .and_then(|v| v.as_array())
        .map(|segments| {
            segments
                .iter()
                .filter_map(|seg| seg.get(0).and_then(|t| t.as_str()))
                .collect()
        })
        .unwrap_or_default();
    if primary.is_empty() {
        return Err(parse_failure(body));
    }

    let alternatives = parse_dictionary(root.get(1));
    let detected_lang = root
        .get(2)
        .and_then(|v| v.as_str())
        .map(normalize_lang);
    let definitions = locate_definitions(root);

    Ok(Translation {
        request_id,
        primary,
        alternatives,
        definitions,
        detected_lang,
    })
}

/// `bd` block: array of `[pos, [plain terms...], [[term, [back-translations],
/// _, score?], ...], ...]`. The scored list can omit scores on tail entries.
fn parse_dictionary(value: Option<&serde_json::Value>) -> Vec<DictEntry> {
    let Some(groups) = value.and_then(|v| v.as_array()) else {
        return vec![];
    };
    groups
        .iter()
        .filter_map(|group| {
            let pos = group.get(0)?.as_str()?.to_string();
            let terms = match group.get(2).and_then(|v| v.as_array()) {
                Some(scored) => scored
                    .iter()
                    .filter_map(|entry| {
                        Some(ScoredTerm {
                            term: entry.get(0)?.as_str()?.to_string(),
                            score: entry
                                .get(3)
                                .and_then(|s| s.as_f64())
                                .unwrap_or(0.0) as f32,
                        })
                    })
                    .collect::<Vec<_>>(),
                // Fall back to the plain term list when the scored one is absent.
                None => group
                    .get(1)
                    .and_then(|v| v.as_array())
                    .map(|terms| {
                        terms
                            .iter()
                            .filter_map(|t| {
                                Some(ScoredTerm {
                                    term: t.as_str()?.to_string(),
                                    score: 0.0,
                                })
                            })
                            .collect()
                    })
                    .unwrap_or_default(),
            };
            (!terms.is_empty()).then_some(DictEntry { pos, terms })
        })
        .collect()
}

/// `md` block: array of `[pos, [[definition, id, ...], ...], ...]`. Its index
/// varies by response, so find it by shape among the trailing elements. The
/// discriminator against the `bd` block (whose groups carry a string list at
/// index 1) is that each group's index-1 list contains arrays whose first
/// element is a string.
fn locate_definitions(root: &[serde_json::Value]) -> Vec<Definition> {
    for value in root.iter().skip(3) {
        let Some(groups) = value.as_array() else {
            continue;
        };
        if groups.is_empty() {
            continue;
        }
        let all_match = groups.iter().all(|group| {
            group.get(0).is_some_and(|p| p.is_string())
                && group
                    .get(1)
                    .and_then(|d| d.as_array())
                    .and_then(|defs| defs.first())
                    .and_then(|first| first.as_array())
                    .and_then(|first| first.first())
                    .is_some_and(|text| text.is_string())
        });
        if !all_match {
            continue;
        }
        return groups
            .iter()
            .flat_map(|group| {
                let pos = group
                    .get(0)
                    .and_then(|p| p.as_str())
                    .unwrap_or_default()
                    .to_string();
                group
                    .get(1)
                    .and_then(|d| d.as_array())
                    .map(|defs| {
                        defs.iter()
                            .filter_map(|def| {
                                Some(Definition {
                                    pos: pos.clone(),
                                    text: def.get(0)?.as_str()?.to_string(),
                                })
                            })
                            .collect::<Vec<_>>()
                    })
                    .unwrap_or_default()
            })
            .collect();
    }
    vec![]
}

#[cfg(test)]
mod tests {
    use super::*;

    const WORD_DE_EN: &str = include_str!("../../fixtures/google/word_de_en.json");
    const WORD_EN_HE: &str = include_str!("../../fixtures/google/word_en_he.json");
    const SENTENCE_NL_EN: &str = include_str!("../../fixtures/google/sentence_nl_en.json");
    const HEBREW_HE_EN: &str = include_str!("../../fixtures/google/hebrew_he_en.json");
    const WORD_EN_EN: &str = include_str!("../../fixtures/google/word_en_en_detected.json");
    const ERROR_HTML: &str = include_str!("../../fixtures/google/error_ratelimit.html");

    #[test]
    fn word_with_dictionary() {
        let t = parse_response(WORD_DE_EN, 7).unwrap();
        assert_eq!(t.request_id, 7);
        assert_eq!(t.primary, "nice");
        assert_eq!(t.detected_lang.as_deref(), Some("de"));

        assert_eq!(t.alternatives.len(), 2);
        let adjective = &t.alternatives[0];
        assert_eq!(adjective.pos, "adjective");
        assert_eq!(adjective.terms[0].term, "beautiful");
        assert!((adjective.terms[0].score - 0.4437473).abs() < 1e-5);
        // Tail entry without a score parses with score 0.
        let last = adjective.terms.last().unwrap();
        assert_eq!(last.term, "nice-looking");
        assert_eq!(last.score, 0.0);
        assert_eq!(t.alternatives[1].pos, "adverb");

        // This response carries no md block.
        assert!(t.definitions.is_empty());
    }

    #[test]
    fn word_with_definitions_and_hebrew_terms() {
        let t = parse_response(WORD_EN_HE, 1).unwrap();
        assert_eq!(t.primary, "לְקַווֹת");
        assert_eq!(t.detected_lang.as_deref(), Some("en"));

        let poses: Vec<&str> = t.alternatives.iter().map(|d| d.pos.as_str()).collect();
        assert_eq!(poses, vec!["verb", "noun"]);
        assert_eq!(t.alternatives[1].terms[0].term, "תִקוָה");

        // md located by shape at its variable trailing index.
        assert_eq!(t.definitions.len(), 3);
        assert_eq!(t.definitions[0].pos, "noun");
        assert!(t.definitions[0]
            .text
            .starts_with("a feeling of expectation"));
        assert_eq!(t.definitions[2].pos, "verb");
    }

    #[test]
    fn sentence_has_no_dictionary_data() {
        let t = parse_response(SENTENCE_NL_EN, 2).unwrap();
        assert_eq!(
            t.primary,
            "The art is not difficult, but it takes patience and practice."
        );
        assert_eq!(t.detected_lang.as_deref(), Some("nl"));
        assert!(t.alternatives.is_empty());
        assert!(t.definitions.is_empty());
    }

    #[test]
    fn dutch_word_parses_full_tail() {
        // Trimming happens in the orchestrator; the parser keeps everything.
        let t = parse_response(
            include_str!("../../fixtures/google/word_nl_en.json"),
            8,
        )
        .unwrap();
        assert_eq!(t.primary, "boy");
        assert_eq!(t.detected_lang.as_deref(), Some("nl"));
        assert_eq!(t.alternatives[0].terms.len(), 7);
        assert_eq!(t.alternatives[0].terms.last().unwrap().term, "get");
    }

    #[test]
    fn hebrew_detected_lang_is_normalized() {
        let t = parse_response(HEBREW_HE_EN, 3).unwrap();
        assert_eq!(t.primary, "Hope is a thing with feathers");
        // Endpoint reports legacy "iw"; we normalize to "he".
        assert_eq!(t.detected_lang.as_deref(), Some("he"));
    }

    #[test]
    fn null_dictionary_with_definitions_present() {
        // "hello" en->en: [1] is null, definitions still located by shape.
        let t = parse_response(WORD_EN_EN, 4).unwrap();
        assert_eq!(t.primary, "hello");
        assert_eq!(t.detected_lang.as_deref(), Some("en"));
        assert!(t.alternatives.is_empty());
        assert_eq!(t.definitions.len(), 3);
        assert_eq!(t.definitions[0].pos, "exclamation");
    }

    #[test]
    fn html_error_body_maps_to_network_error() {
        let err = parse_response(ERROR_HTML, 5).unwrap_err();
        assert!(matches!(err, ProviderError::Network(_)));
        assert!(err.retryable());
    }

    #[test]
    fn garbage_and_wrong_shapes_map_to_network_error() {
        assert!(matches!(
            parse_response("", 1),
            Err(ProviderError::Network(_))
        ));
        assert!(matches!(
            parse_response("{\"not\":\"an array\"}", 1),
            Err(ProviderError::Network(_))
        ));
        assert!(matches!(
            parse_response("[null,null,null]", 1),
            Err(ProviderError::Network(_))
        ));
    }
}
