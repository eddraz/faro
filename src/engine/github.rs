//! GitHub repository search parser.

use super::{encode_query, SearchResult};

pub(crate) fn url(query: &str) -> String {
    format!(
        "https://github.com/search?q={}&type=repositories",
        encode_query(query)
    )
}

/// Top-level GitHub routes that show up as links but are not repositories.
const INTERNAL_ROUTES: [&str; 26] = [
    "about",
    "contact",
    "collections",
    "copilot",
    "customer-stories",
    "enterprise",
    "events",
    "explore",
    "features",
    "issues",
    "login",
    "marketplace",
    "notifications",
    "orgs",
    "pricing",
    "pulls",
    "readme",
    "search",
    "security",
    "settings",
    "signup",
    "sponsors",
    "team",
    "topics",
    "trending",
    "readies",
];

const ASSET_EXTENSIONS: [&str; 6] = [".svg", ".css", ".js", ".png", ".ico", ".json"];

pub(crate) fn parse(html: &str) -> Vec<SearchResult> {
    let document = scraper::Html::parse_document(html);
    let anchor = scraper::Selector::parse("a").expect("valid selector");
    let mut seen = std::collections::HashSet::new();
    let mut results = Vec::new();

    for element in document.select(&anchor) {
        let Some(href) = element.value().attr("href") else {
            continue;
        };
        let Some((owner, repo)) = repository_path(href) else {
            continue;
        };
        if !seen.insert(format!("{owner}/{repo}")) {
            continue;
        }
        results.push(SearchResult {
            engine: "github".into(),
            title: format!("{owner}/{repo}"),
            url: format!("https://github.com/{owner}/{repo}"),
            snippet: String::new(),
        });
    }
    results
}

/// Recognize `/{owner}/{repo}` hrefs, rejecting internal routes and assets.
fn repository_path(href: &str) -> Option<(&str, &str)> {
    let path = href.strip_prefix('/')?;
    let path = path.split(['?', '#']).next()?;
    let (owner, repo) = path.split_once('/')?;
    if owner.is_empty() || repo.is_empty() || repo.contains('/') {
        return None;
    }
    if INTERNAL_ROUTES.contains(&owner) {
        return None;
    }
    if ASSET_EXTENSIONS.iter().any(|ext| repo.ends_with(ext)) {
        return None;
    }
    Some((owner, repo))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_repositories_and_skips_internal_routes_and_assets() {
        let html = include_str!("../../tests/fixtures/github.html");
        let results = parse(html);
        assert!(results.iter().any(|r| r.title == "rust-lang/book"));
        assert!(results
            .iter()
            .all(|r| !ASSET_EXTENSIONS.iter().any(|ext| r.title.ends_with(ext))));
        assert!(results.iter().all(|r| {
            r.title
                .split('/')
                .next()
                .is_some_and(|owner| !INTERNAL_ROUTES.contains(&owner))
        }));
    }

    #[test]
    fn repository_path_rejects_junk() {
        assert_eq!(
            repository_path("/rust-lang/book"),
            Some(("rust-lang", "book"))
        );
        assert_eq!(
            repository_path("/rust-lang/book?after=1"),
            Some(("rust-lang", "book"))
        );
        assert_eq!(repository_path("/features/copilot"), None);
        assert_eq!(repository_path("/rust-lang/book.svg"), None);
        assert_eq!(repository_path("https://github.com/rust-lang/book"), None);
        assert_eq!(repository_path("/onlyone"), None);
        assert_eq!(repository_path("/a/b/c"), None);
    }
}
