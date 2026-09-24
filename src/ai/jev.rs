//! TypeSafe JEV (cloud System One model) integration for semantic relevance validation and deduplication.

use std::time::Duration;

use anyhow::Result;
use serde_json::{json, Value};

use crate::engine::SearchResult;

use super::sagaz::MAX_BATCH_SNIPPETS;

const SYSTEMONE_URL: &str = "https://api.typesafe.ai/v1/systemone";
const MODEL: &str = "jev-latest";

/// Locate the TypeSafe API key in the `TYPESAFE_API_KEY` env var.
pub fn find_api_key() -> Option<String> {
    std::env::var("TYPESAFE_API_KEY")
        .ok()
        .map(|v| v.trim().to_string())
        .filter(|s| !s.is_empty())
}

/// Construct state and questions payloads for the TypeSafe System One API.
///
/// State layout matches sagaz (`query`, `s0`, `s1`, ...). Questions carry
/// `criteria` rubrics so the cloud model applies consistent relevance and
/// duplicate judgments.
pub fn build_jev_payload(query: &str, results: &[SearchResult]) -> (Value, Value) {
    let mut state_map = serde_json::Map::new();
    state_map.insert("query".to_string(), json!(query));

    let mut questions_map = serde_json::Map::new();

    let count = results.len().min(MAX_BATCH_SNIPPETS);
    for i in 0..count {
        let key = format!("s{i}");
        let text = format!("{}: {}", results[i].title, results[i].snippet);
        state_map.insert(key, json!(text));

        // Relevance question with criteria rubric.
        let rel_q_key = format!("s{i}_rel");
        questions_map.insert(
            rel_q_key,
            json!({
                "type": "noul",
                "instructions": format!("Is the snippet `s{i}` relevant to the query `query`?"),
                "criteria": {
                    "true": "The snippet is directly about the query topic and offers substantive information (facts, guides, explanations, official resources).",
                    "false": "The snippet is an advertisement, product listing, spam, off-topic, or only tangentially related to the query."
                }
            }),
        );

        // Duplicate check against previous candidates.
        for j in 0..i {
            let dup_q_key = format!("s{i}_dup_s{j}");
            questions_map.insert(
                dup_q_key,
                json!({
                    "type": "noul",
                    "instructions": format!("Does the snippet `s{i}` repeat essentially the same information as `s{j}`?"),
                    "criteria": {
                        "true": "The snippet repeats the same core information so reading one makes the other redundant.",
                        "false": "The snippets offer different information, angle, or additional detail."
                    }
                }),
            );
        }
    }

    (Value::Object(state_map), Value::Object(questions_map))
}

/// Run TypeSafe JEV validation and deduplication on search results.
pub fn validate_and_deduplicate(
    query: &str,
    results: &[SearchResult],
    relevance_threshold: f64,
    duplicate_threshold: f64,
) -> Result<Vec<SearchResult>> {
    if results.len() <= 1 {
        return Ok(results.to_vec());
    }

    let api_key = match find_api_key() {
        Some(key) => key,
        None => {
            eprintln!("jev: TYPESAFE_API_KEY not set; skipping validation.");
            return Ok(results.to_vec());
        }
    };

    let count = results.len().min(MAX_BATCH_SNIPPETS);
    eprintln!(
        "jev: evaluating top {count} search results for semantic relevance and duplicates..."
    );

    let (state_json, questions_json) = build_jev_payload(query, results);
    let body = json!({
        "state": state_json,
        "model": MODEL,
        "questions": questions_json,
    });

    let json_body = match serde_json::to_string(&body) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("jev: failed to encode request: {e}; proceeding with unvalidated results.");
            return Ok(results.to_vec());
        }
    };

    let response = match ureq::post(SYSTEMONE_URL)
        .set("Authorization", &format!("Bearer {api_key}"))
        .set("Content-Type", "application/json")
        .timeout(Duration::from_secs(30))
        .send_string(&json_body)
    {
        Ok(response) => response,
        Err(e) => {
            eprintln!("jev: request failed: {e}; proceeding with unvalidated results.");
            return Ok(results.to_vec());
        }
    };

    let body_str = match response.into_string() {
        Ok(s) => s,
        Err(e) => {
            eprintln!("jev: failed to read response: {e}; proceeding with unvalidated results.");
            return Ok(results.to_vec());
        }
    };

    let parsed: Value = match serde_json::from_str(&body_str) {
        Ok(value) => value,
        Err(e) => {
            eprintln!("jev: failed to parse response: {e}; proceeding with unvalidated results.");
            return Ok(results.to_vec());
        }
    };

    let answers = parsed.get("answers").cloned().unwrap_or(json!({}));
    let filtered = super::sagaz::filter_results_with_answers(
        results,
        &answers,
        relevance_threshold,
        duplicate_threshold,
    );

    eprintln!(
        "jev: retained {} of {} results.",
        filtered.len(),
        results.len()
    );

    Ok(filtered)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_jev_payload_creates_expected_keys_and_criteria() {
        let results = vec![
            SearchResult {
                engine: "wikipedia".into(),
                title: "Rust Language".into(),
                url: "https://rust-lang.org".into(),
                snippet: "Fast, memory-safe language.".into(),
            },
            SearchResult {
                engine: "duckduckgo".into(),
                title: "Rust Lang Duplicate".into(),
                url: "https://other.org/rust".into(),
                snippet: "Fast and memory-safe programming.".into(),
            },
        ];

        let (state, questions) = build_jev_payload("what is rust?", &results);
        assert_eq!(state.get("query").unwrap(), "what is rust?");
        assert!(state.get("s0").is_some());
        assert!(state.get("s1").is_some());

        let rel = questions.get("s0_rel").expect("s0_rel question");
        assert_eq!(rel.get("type").unwrap(), "noul");
        let instructions = rel.get("instructions").unwrap().as_str().unwrap();
        assert!(instructions.contains("`query`"));
        assert!(instructions.contains("`s0`"));
        let criteria = rel.get("criteria").expect("relevance criteria");
        assert!(criteria.get("true").unwrap().as_str().unwrap().contains("directly about the query topic"));
        assert!(criteria.get("false").unwrap().as_str().unwrap().contains("tangentially related"));

        assert!(questions.get("s1_rel").is_some());
        let dup = questions.get("s1_dup_s0").expect("s1_dup_s0 question");
        assert_eq!(dup.get("type").unwrap(), "noul");
        let dup_criteria = dup.get("criteria").expect("duplicate criteria");
        assert!(dup_criteria.get("true").unwrap().as_str().unwrap().contains("redundant"));
        assert!(dup_criteria.get("false").unwrap().as_str().unwrap().contains("different information"));
    }

    #[test]
    fn filter_jev_answers_drops_low_relevance_and_duplicates() {
        let results = vec![
            SearchResult {
                engine: "wikipedia".into(),
                title: "Rust".into(),
                url: "https://rust-lang.org".into(),
                snippet: "Systems programming language.".into(),
            },
            SearchResult {
                engine: "google".into(),
                title: "Irrelevant Topic".into(),
                url: "https://random.org".into(),
                snippet: "Unrelated text.".into(),
            },
            SearchResult {
                engine: "bing".into(),
                title: "Rust Copy".into(),
                url: "https://copy.org".into(),
                snippet: "Systems programming language.".into(),
            },
        ];

        let answers = json!({
            "s0_rel": { "type": "noul", "noul": 0.85 },
            "s1_rel": { "type": "noul", "noul": 0.15 },
            "s2_rel": { "type": "noul", "noul": 0.90 },
            "s2_dup_s0": { "type": "noul", "noul": 0.80 }
        });

        let filtered =
            crate::ai::sagaz::filter_results_with_answers(&results, &answers, 0.40, 0.60);
        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].title, "Rust");
    }
}
