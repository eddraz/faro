//! DuckDuckGo parser (html.duckduckgo.com non-JS endpoint).

use percent_encoding::percent_decode_str;

use super::{encode_query, text_of, SearchResult};

pub(crate) fn url(query: &str) -> String {
    // The public duckduckgo.com URL meta-refreshes here anyway; go direct.
    format!(
        "https://html.duckduckgo.com/html/?q={}",
        encode_query(query)
    )
}

pub(crate) fn parse(html: &str) -> Vec<SearchResult> {
    let document = scraper::Html::parse_document(html);
    let container = scraper::Selector::parse("div.result").expect("valid selector");
    let anchor = scraper::Selector::parse("a.result__a").expect("valid selector");
    let snippet = scraper::Selector::parse(".result__snippet").expect("valid selector");

    let mut results = Vec::new();
    for block in document.select(&container) {
        let Some(link) = block.select(&anchor).next() else {
            continue;
        };
        let Some(href) = link.value().attr("href") else {
            continue;
        };
        let Some(url) = result_url(href) else {
            continue;
        };
        let snippet_text = block
            .select(&snippet)
            .next()
            .map(|element| text_of(element))
            .unwrap_or_default();
        results.push(SearchResult {
            engine: "duckduckgo".into(),
            title: text_of(link),
            url,
            snippet: snippet_text,
        });
    }
    results
}

/// Unwrap DuckDuckGo redirect links: the real target rides in `uddg=`.
fn result_url(href: &str) -> Option<String> {
    let index = href.find("uddg=")? + "uddg=".len();
    let rest = &href[index..];
    let end = rest.find('&').unwrap_or(rest.len());
    let decoded = percent_decode_str(&rest[..end]).decode_utf8().ok()?;
    let url = decoded.to_string();
    if url.is_empty() {
        None
    } else if url.starts_with("//") {
        Some(format!("https:{url}"))
    } else {
        Some(url)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_ads_and_organic_results_with_decoded_urls() {
        let html = include_str!("../../tests/fixtures/duckduckgo.html");
        let results = parse(html);
        assert!(
            results.len() >= 3,
            "fixture carries ads plus organic results"
        );
        assert!(results.iter().any(|r| r.url.contains("rust-lang.org")));
        assert!(
            results.iter().all(|r| r.url.starts_with("http")),
            "uddg links must decode to absolute urls"
        );
        assert!(results.iter().any(|r| !r.snippet.is_empty()));
    }

    #[test]
    fn result_url_decodes_redirects() {
        assert_eq!(
            result_url("//duckduckgo.com/l/?uddg=https%3A%2F%2Frust%2Dlang.org%2F&rut=x"),
            Some("https://rust-lang.org/".to_string())
        );
        assert_eq!(result_url("/no/redirect/here"), None);
    }
}
