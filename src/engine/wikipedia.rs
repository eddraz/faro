//! Wikipedia (MediaWiki) full-text search parser.

use super::{encode_query, text_of, SearchResult};

pub(crate) fn url(query: &str) -> String {
    format!(
        "https://en.wikipedia.org/w/index.php?search={}",
        encode_query(query)
    )
}

pub(crate) fn parse(html: &str) -> Vec<SearchResult> {
    let document = scraper::Html::parse_document(html);
    let container = scraper::Selector::parse("li.mw-search-result").expect("valid selector");
    let heading = scraper::Selector::parse(".mw-search-result-heading a").expect("valid selector");
    let snippet = scraper::Selector::parse(".searchresult").expect("valid selector");

    let mut results = Vec::new();
    for block in document.select(&container) {
        let Some(link) = block.select(&heading).next() else {
            continue;
        };
        let Some(href) = link.value().attr("href") else {
            continue;
        };
        // MediaWiki titles ride in the `title` attribute; fall back to text.
        let title = link
            .value()
            .attr("title")
            .map(str::to_string)
            .unwrap_or_else(|| text_of(link));
        let url = if href.starts_with('/') {
            format!("https://en.wikipedia.org{href}")
        } else {
            href.to_string()
        };
        let snippet_text = block
            .select(&snippet)
            .next()
            .map(|element| text_of(element))
            .unwrap_or_default();
        results.push(SearchResult {
            engine: "wikipedia".into(),
            title,
            url,
            snippet: snippet_text,
        });
    }
    results
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_results_with_titles_snippets_and_absolute_urls() {
        let html = include_str!("../../tests/fixtures/wikipedia.html");
        let results = parse(html);
        assert!(results.len() >= 3);
        assert_eq!(results[0].title, "Rust (programming language)");
        assert_eq!(
            results[0].url,
            "https://en.wikipedia.org/wiki/Rust_(programming_language)"
        );
        assert!(results
            .iter()
            .all(|r| r.url.starts_with("https://en.wikipedia.org/")));
        assert!(results.iter().any(|r| !r.snippet.is_empty()));
    }
}
