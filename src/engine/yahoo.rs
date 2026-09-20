//! Yahoo web search parser.

use super::{encode_query, text_of, SearchResult};

pub(crate) fn url(query: &str) -> String {
    format!("https://search.yahoo.com/search?p={}", encode_query(query))
}

pub(crate) fn parse(html: &str) -> Vec<SearchResult> {
    let document = scraper::Html::parse_document(html);
    let container = scraper::Selector::parse("div.algo").expect("valid selector");
    let title_anchor = scraper::Selector::parse(".compTitle a, h3 a").expect("valid selector");
    let snippet = scraper::Selector::parse(".compText, p").expect("valid selector");

    let h3 = scraper::Selector::parse("h3").expect("valid selector");

    let mut results = Vec::new();
    for block in document.select(&container) {
        let Some(link) = block.select(&title_anchor).next() else {
            continue;
        };
        let Some(href) = link.value().attr("href") else {
            continue;
        };
        // The anchor wraps the h3 plus breadcrumb noise; prefer the h3 text.
        let title = link
            .select(&h3)
            .next()
            .map(text_of)
            .unwrap_or_else(|| text_of(link));
        // Yahoo wraps targets in r.search.yahoo.com redirects; keep as returned.
        let snippet_text = block
            .select(&snippet)
            .next()
            .map(|element| text_of(element))
            .unwrap_or_default();
        results.push(SearchResult {
            engine: "yahoo".into(),
            title,
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
    fn parses_algo_containers() {
        let html = include_str!("../../tests/fixtures/yahoo.html");
        let results = parse(html);
        assert!(!results.is_empty());
        assert!(results[0].title.contains("Rust"));
        assert!(!results[0].url.is_empty());
    }
}
