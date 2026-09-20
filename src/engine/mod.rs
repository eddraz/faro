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

#[cfg(test)]
mod tests {
    use super::encode_query;

    #[test]
    fn encodes_spaces_as_plus_and_keeps_safe_characters() {
        assert_eq!(encode_query("rust programming"), "rust+programming");
        assert_eq!(encode_query("c++ tips"), "c%2B%2B+tips");
        assert_eq!(encode_query("a_b-c.d"), "a_b-c.d");
    }
}
