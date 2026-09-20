//! Bing web search parser.

use super::{encode_query, text_of, SearchResult};

pub(crate) fn url(query: &str) -> String {
    format!("https://www.bing.com/search?q={}", encode_query(query))
}

pub(crate) fn parse(html: &str) -> Vec<SearchResult> {
    let document = scraper::Html::parse_document(html);
    let container = scraper::Selector::parse("li.b_algo").expect("valid selector");
    let anchor = scraper::Selector::parse("h2 a").expect("valid selector");
    let snippet = scraper::Selector::parse(".b_caption p").expect("valid selector");

    let mut results = Vec::new();
    for block in document.select(&container) {
        let Some(link) = block.select(&anchor).next() else {
            continue;
        };
        let Some(href) = link.value().attr("href") else {
            continue;
        };
        // Bing wraps targets in /ck/a tracking redirects; keep as returned.
        let snippet_text = block
            .select(&snippet)
            .next()
            .map(|element| text_of(element))
            .unwrap_or_default();
        results.push(SearchResult {
            engine: "bing".into(),
            title: text_of(link),
            url: href.to_string(),
            snippet: snippet_text,
        });
    }
    results
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_result_blocks_with_tracking_urls() {
        let html = include_str!("../../tests/fixtures/bing.html");
        let results = parse(html);
        assert!(results.len() >= 2);
        assert_eq!(results[0].title, "Rust Programming Language");
        assert!(results[0].url.starts_with("https://www.bing.com/ck/a"));
        assert!(results.iter().any(|r| !r.snippet.is_empty()));
    }
}
