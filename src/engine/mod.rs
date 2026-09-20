//! Search engine abstraction: one module per validated engine.

pub(crate) mod bing;
pub(crate) mod duckduckgo;
pub(crate) mod github;
pub(crate) mod wikipedia;
pub(crate) mod yahoo;

use std::path::Path;

use percent_encoding::{utf8_percent_encode, AsciiSet, NON_ALPHANUMERIC};
use serde::Serialize;

use crate::runner;

/// One search result, engine-agnostic.
#[derive(Clone, Debug, Serialize)]
pub(crate) struct SearchResult {
    pub(crate) engine: String,
    pub(crate) title: String,
    pub(crate) url: String,
    pub(crate) snippet: String,
}

/// Form-style query encoding: space becomes `+`; characters the engines
/// treat as reserved get percent-encoded. `-`, `_` and `.` stay readable.
const QUERY_SET: &AsciiSet = &NON_ALPHANUMERIC.remove(b'-').remove(b'_').remove(b'.');

pub(crate) fn encode_query(query: &str) -> String {
    utf8_percent_encode(query, QUERY_SET)
        .to_string()
        .replace("%20", "+")
}

/// Collapse an element's text into single-spaced content.
pub(crate) fn text_of(element: scraper::ElementRef) -> String {
    element
        .text()
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

/// Fetch, parse and bound the results of one engine. Errors are returned as
/// (engine name, message) so a single broken engine never sinks the run.
pub(crate) async fn run_engine(
    name: &str,
    obscura: &Path,
    query: &str,
    limit: usize,
    timeout_secs: u64,
) -> Result<Vec<SearchResult>, (String, String)> {
    let url = match name {
        "github" => github::url(query),
        "duckduckgo" => duckduckgo::url(query),
        "bing" => bing::url(query),
        "yahoo" => yahoo::url(query),
        "wikipedia" => wikipedia::url(query),
        other => return Err((other.to_string(), format!("unknown engine {other:?}"))),
    };
    let parser: fn(&str) -> Vec<SearchResult> = match name {
        "github" => github::parse,
        "duckduckgo" => duckduckgo::parse,
        "bing" => bing::parse,
        "yahoo" => yahoo::parse,
        _ => wikipedia::parse,
    };

    let html = runner::fetch_html(obscura, &url, timeout_secs)
        .await
        .map_err(|error| (name.to_string(), error.to_string()))?;
    let mut results = parser(&html);
    results.truncate(limit);
    Ok(results)
}

/// Cascade merge (the hybrid): per requested engine, SearXNG results come
/// first; obscura results fill the gap up to `limit`, deduplicating URLs.
/// An engine listed in `unresponsive` (searxng reported it rate-limited or
/// captcha-blocked) ignores searxng results entirely and goes pure obscura.
pub(crate) fn cascade_merge(
    selected: &[String],
    limit: usize,
    searxng: Vec<SearchResult>,
    unresponsive: &[String],
    mut obscura: std::collections::HashMap<String, Vec<SearchResult>>,
) -> Vec<SearchResult> {
    fn degraded(engine: &str, unresponsive: &[String]) -> bool {
        // Entries may carry a suffix like "bing (HTTP error 429)".
        unresponsive.iter().any(|entry| entry.starts_with(engine))
    }

    let mut out: Vec<SearchResult> = Vec::new();
    let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();

    let mut per_engine: std::collections::HashMap<String, Vec<SearchResult>> =
        std::collections::HashMap::new();
    for result in searxng {
        per_engine
            .entry(result.engine.clone())
            .or_default()
            .push(result);
    }

    for engine in selected {
        let mut count = 0usize;
        if !degraded(engine, unresponsive) {
            if let Some(list) = per_engine.get_mut(engine) {
                for result in list.drain(..) {
                    if count >= limit {
                        break;
                    }
                    if seen.insert(result.url.clone()) {
                        out.push(result);
                        count += 1;
                    }
                }
            }
        }
        if count < limit {
            if let Some(list) = obscura.get_mut(engine) {
                for result in list.drain(..) {
                    if count >= limit {
                        break;
                    }
                    if seen.insert(result.url.clone()) {
                        out.push(result);
                        count += 1;
                    }
                }
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::{cascade_merge, encode_query, SearchResult};
    use std::collections::HashMap;

    fn result(engine: &str, title: &str, url: &str) -> SearchResult {
        SearchResult {
            engine: engine.into(),
            title: title.into(),
            url: url.into(),
            snippet: String::new(),
        }
    }

    #[test]
    fn cascade_tolerates_searxng_only_engines() {
        let selected = vec!["google".to_string()];
        // Healthy: searxng results survive with no obscura fallback present.
        let merged = cascade_merge(
            &selected,
            3,
            vec![result("google", "g", "https://google/r1")],
            &[],
            HashMap::new(),
        );
        assert_eq!(merged.len(), 1);
        // Degraded: zero results and no panic.
        let merged = cascade_merge(
            &selected,
            3,
            Vec::new(),
            &["google (CAPTCHA)".to_string()],
            HashMap::new(),
        );
        assert!(merged.is_empty());
    }

    #[test]
    fn cascade_prefers_searxng_fills_with_obscura_and_drops_degraded() {
        let selected = vec!["github".to_string(), "bing".to_string()];
        let searxng = vec![
            result("github", "g1", "https://g/1"),
            result("github", "g2", "https://g/2"),
            result("bing", "b-stale", "https://b/stale"),
        ];
        let mut obscura = HashMap::new();
        obscura.insert(
            "github".to_string(),
            vec![
                result("github", "g1-dup", "https://g/1"),
                result("github", "g3", "https://g/3"),
            ],
        );
        obscura.insert(
            "bing".to_string(),
            vec![
                result("bing", "b1", "https://b/1"),
                result("bing", "b2", "https://b/2"),
            ],
        );
        let unresponsive = vec!["bing (HTTP error 429)".to_string()];
        let merged = cascade_merge(&selected, 3, searxng, &unresponsive, obscura);
        let urls: Vec<&str> = merged.iter().map(|r| r.url.as_str()).collect();
        // github: 2 de searxng + 1 de relleno (dup descartado)
        // bing: degradado -> resultados solo de obscura, el stale de searxng se descarta
        assert_eq!(
            urls,
            vec![
                "https://g/1",
                "https://g/2",
                "https://g/3",
                "https://b/1",
                "https://b/2"
            ]
        );
    }

    #[test]
    fn encodes_spaces_as_plus_and_keeps_safe_characters() {
        assert_eq!(encode_query("rust programming"), "rust+programming");
        assert_eq!(encode_query("c++ tips"), "c%2B%2B+tips");
        assert_eq!(encode_query("a_b-c.d"), "a_b-c.d");
    }
}
