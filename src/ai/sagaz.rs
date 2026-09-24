//! Sagaz (Laya / JEV) integration for semantic relevance validation and deduplication.

use std::path::PathBuf;

use anyhow::{Context, Result};
use serde_json::{json, Value};

use crate::engine::SearchResult;

pub const DEFAULT_RELEVANCE_THRESHOLD: f64 = 0.40;
pub const DEFAULT_DUPLICATE_THRESHOLD: f64 = 0.55;
pub const MAX_BATCH_SNIPPETS: usize = 6;

/// Locate `sagaz` binary in ~/.local/bin or on PATH.
pub fn find_sagaz() -> Option<PathBuf> {
    if let Some(path) = which("sagaz") {
        return Some(path);
    }

    if let Some(home) = std::env::var_os("HOME") {
        let p = PathBuf::from(&home).join(".local").join("bin").join("sagaz");
        if p.is_file() {
            return Some(p);
        }
    }

    None
}

fn which(program: &str) -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    for dir in std::env::split_paths(&path) {
        let candidate = dir.join(program);
        if candidate.is_file() {
            return Some(candidate);
        }
    }
    None
}

/// Construct batch state and questions payloads for Sagaz.
pub fn build_batch_payload(
    query: &str,
    results: &[SearchResult],
) -> (Value, Value) {
    let mut state_map = serde_json::Map::new();
    state_map.insert("query".to_string(), json!(query));

    let mut questions_map = serde_json::Map::new();

    let count = results.len().min(MAX_BATCH_SNIPPETS);
    for i in 0..count {
        let key = format!("s{i}");
        let text = format!("{}: {}", results[i].title, results[i].snippet);
        state_map.insert(key.clone(), json!(text));

        // Relevance question
        let rel_q_key = format!("s{i}_rel");
        questions_map.insert(
            rel_q_key,
            json!({
                "type": "noul",
                "instructions": format!("¿El fragmento {key} aporta información relevante para la consulta?")
            }),
        );

        // Duplicate check against previous candidates
        for j in 0..i {
            let dup_q_key = format!("s{i}_dup_s{j}");
            questions_map.insert(
                dup_q_key,
                json!({
                    "type": "noul",
                    "instructions": format!("¿El fragmento s{i} repite esencialmente la misma información de s{j}?")
                }),
            );
        }
    }

    (Value::Object(state_map), Value::Object(questions_map))
}

/// Extract boolean score (p_true) from an answer object.
pub fn extract_noul_score(answer_val: &Value) -> Option<f64> {
    if let Some(noul) = answer_val.get("noul").and_then(|v| v.as_f64()) {
        return Some(noul);
    }
    if let Some(p_true) = answer_val.get("p_true").and_then(|v| v.as_f64()) {
        return Some(p_true);
    }
    answer_val.get("confidence").and_then(|v| v.as_f64())
}

/// Filter results based on parsed Sagaz answers.
pub fn filter_results_with_answers(
    results: &[SearchResult],
    answers: &Value,
    relevance_threshold: f64,
    duplicate_threshold: f64,
) -> Vec<SearchResult> {
    let mut accepted_indices: Vec<usize> = Vec::new();
    let mut filtered: Vec<SearchResult> = Vec::new();

    let count = results.len().min(MAX_BATCH_SNIPPETS);

    for i in 0..count {
        let rel_key = format!("s{i}_rel");
        let rel_score = answers
            .get(&rel_key)
            .and_then(extract_noul_score)
            .unwrap_or(0.5);

        if rel_score < relevance_threshold {
            eprintln!(
                "sagaz: [s{i}] \"{}\" dropped (low relevance score: {:.2})",
                results[i].title, rel_score
            );
            continue;
        }

        // Check if duplicate of any already accepted candidate
        let mut is_dup = false;
        for &prev in &accepted_indices {
            let dup_key = format!("s{i}_dup_s{prev}");
            if let Some(dup_score) = answers.get(&dup_key).and_then(extract_noul_score) {
                if dup_score >= duplicate_threshold {
                    eprintln!(
                        "sagaz: [s{i}] \"{}\" dropped (duplicate of s{prev}, score: {:.2})",
                        results[i].title, dup_score
                    );
                    is_dup = true;
                    break;
                }
            }
        }

        if !is_dup {
            accepted_indices.push(i);
            filtered.push(results[i].clone());
        }
    }

    // Append any excess results beyond batch limit untouched
    if results.len() > MAX_BATCH_SNIPPETS {
        for item in &results[MAX_BATCH_SNIPPETS..] {
            filtered.push(item.clone());
        }
    }

    filtered
}

/// Run Sagaz validation and deduplication on search results.
pub fn validate_and_deduplicate(
    query: &str,
    results: &[SearchResult],
    relevance_threshold: f64,
    duplicate_threshold: f64,
) -> Result<Vec<SearchResult>> {
    if results.is_empty() || results.len() == 1 {
        return Ok(results.to_vec());
    }

    let sagaz_bin = match find_sagaz() {
        Some(bin) => bin,
        None => {
            eprintln!("sagaz: binary not found in ~/.local/bin or PATH; skipping validation.");
            return Ok(results.to_vec());
        }
    };

    let count = results.len().min(MAX_BATCH_SNIPPETS);
    eprintln!(
        "sagaz: evaluating top {count} search results for semantic relevance and duplicates..."
    );

    let (state_json, questions_json) = build_batch_payload(query, results);
    let state_str = serde_json::to_string(&state_json)?;
    let questions_str = serde_json::to_string(&questions_json)?;

    let output = std::process::Command::new(&sagaz_bin)
        .args([
            "predict",
            "-s",
            &state_str,
            "-q",
            &questions_str,
            "--json",
        ])
        .output()
        .with_context(|| format!("failed to execute {}", sagaz_bin.display()))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        eprintln!("sagaz execution failed: {stderr}; proceeding with unvalidated results.");
        return Ok(results.to_vec());
    }

    let stdout_str = String::from_utf8_lossy(&output.stdout);
    // Locate the first '{' in case there are warnings printed before the JSON
    let json_start = stdout_str.find('{').unwrap_or(0);
    let json_slice = &stdout_str[json_start..];

    let parsed: Value = serde_json::from_str(json_slice)
        .with_context(|| format!("failed to parse sagaz output JSON: {stdout_str}"))?;

    let answers = parsed.get("answers").cloned().unwrap_or(json!({}));
    let filtered = filter_results_with_answers(
        results,
        &answers,
        relevance_threshold,
        duplicate_threshold,
    );

    eprintln!(
        "sagaz: retained {} of {} results.",
        filtered.len(),
        results.len()
    );

    Ok(filtered)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_batch_payload_creates_expected_keys() {
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

        let (state, questions) = build_batch_payload("what is rust?", &results);
        assert_eq!(state.get("query").unwrap(), "what is rust?");
        assert!(state.get("s0").is_some());
        assert!(state.get("s1").is_some());

        assert!(questions.get("s0_rel").is_some());
        assert!(questions.get("s1_rel").is_some());
        assert!(questions.get("s1_dup_s0").is_some());
    }

    #[test]
    fn filter_results_drops_low_relevance_and_duplicates() {
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
            "s0_rel": { "noul": 0.85 },
            "s1_rel": { "noul": 0.15 },
            "s2_rel": { "noul": 0.90 },
            "s2_dup_s0": { "noul": 0.80 }
        });

        let filtered = filter_results_with_answers(&results, &answers, 0.40, 0.60);
        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].title, "Rust");
    }
}
