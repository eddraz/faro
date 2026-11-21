//! Terminal rendering: aligned table, Markdown, or JSON.

use serde::Serialize;

use crate::engine::SearchResult;

/// Full-page document fetched via `faro fetch` or `faro gather`.
#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
pub(crate) struct FetchedDocument {
    pub(crate) url: String,
    pub(crate) title: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) engine: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) snippet: Option<String>,
    pub(crate) content: String,
}

pub(crate) fn format_fetched(docs: &[FetchedDocument], json: bool, plain_text: bool) -> String {
    if json {
        return serde_json::to_string_pretty(docs).unwrap_or_else(|_| "[]".to_string());
    }
    if docs.len() == 1 {
        return docs[0].content.clone();
    }

    let mut sections = Vec::with_capacity(docs.len());
    for doc in docs {
        let heading = if doc.title.is_empty() {
            doc.url.as_str()
        } else {
            doc.title.as_str()
        };
        if plain_text {
            sections.push(format!("=== {heading} ({}) ===\n\n{}", doc.url, doc.content));
        } else {
            sections.push(format!(
                "# {heading}\n\n- URL: {}\n\n{}",
                doc.url, doc.content
            ));
        }
    }
    sections.join("\n\n---\n\n")
}

pub(crate) fn format_gathered(
    query: &str,
    docs: &[FetchedDocument],
    json: bool,
    plain_text: bool,
) -> String {
    if json {
        return serde_json::to_string_pretty(docs).unwrap_or_else(|_| "[]".to_string());
    }
    if docs.is_empty() {
        return if plain_text {
            format!("No results gathered for: {query}")
        } else {
            format!("# Search: {query}\n\n*No results found.*")
        };
    }

    let mut out = if plain_text {
        format!("Search: {query}\n\n")
    } else {
        format!("# Search: {query}\n\n")
    };

    for (idx, doc) in docs.iter().enumerate() {
        let num = idx + 1;
        let title = if doc.title.is_empty() {
            doc.url.as_str()
        } else {
            doc.title.as_str()
        };
        let engine = doc.engine.as_deref().unwrap_or("unknown");

        if plain_text {
            out.push_str(&format!(
                "=== [{num}] {title} ({}) [{engine}] ===\n\n{}\n\n",
                doc.url, doc.content
            ));
        } else {
            out.push_str(&format!(
                "## [{num}] {title}\n\n- URL: {}\n- Engine: {engine}\n\n{}\n\n",
                doc.url, doc.content
            ));
            if num < docs.len() {
                out.push_str("---\n\n");
            }
        }
    }

    out.trim_end().to_string()
}

pub(crate) fn render(results: &[SearchResult], json: bool, markdown: bool, with_snippet: bool) {
    if json {
        let payload = serde_json::to_string_pretty(results).unwrap_or_else(|_| "[]".to_string());
        println!("{payload}");
        return;
    }

    if markdown {
        render_markdown(results, with_snippet);
        return;
    }

    if results.is_empty() {
        println!("no results");
        return;
    }

    println!("{:<11} {:<58} URL", "ENGINE", "TITLE");
    println!("{}", "-".repeat(140));
    for result in results {
        println!(
            "{:<11} {:<58} {}",
            result.engine,
            truncate(&result.title, 56),
            truncate(&result.url, 68)
        );
        if with_snippet && !result.snippet.is_empty() {
            println!("{:11} {:58}", "", truncate(&result.snippet, 120));
        }
    }
}

fn render_markdown(results: &[SearchResult], with_snippet: bool) {
    if results.is_empty() {
        println!("*No results found.*");
        return;
    }

    for (i, result) in results.iter().enumerate() {
        println!(
            "{}. [{}]({}) — *{}*",
            i + 1,
            result.title,
            result.url,
            result.engine
        );
        if with_snippet && !result.snippet.is_empty() {
            println!("   > {}", result.snippet.replace('\n', " "));
        }
    }
}

fn truncate(text: &str, max: usize) -> String {
    if text.chars().count() <= max {
        return text.to_string();
    }
    let cut: String = text.chars().take(max.saturating_sub(1)).collect();
    format!("{cut}\u{2026}")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_doc(title: &str, url: &str, engine: Option<&str>, content: &str) -> FetchedDocument {
        FetchedDocument {
            url: url.to_string(),
            title: title.to_string(),
            engine: engine.map(str::to_string),
            snippet: None,
            content: content.to_string(),
        }
    }

    #[test]
    fn format_fetched_single_returns_content_directly() {
        let docs = vec![sample_doc("Guide", "https://example.com", None, "# Guide\n\nBody")];
        assert_eq!(format_fetched(&docs, false, false), "# Guide\n\nBody");
        let as_json = format_fetched(&docs, true, false);
        assert!(as_json.contains("\"title\": \"Guide\""));
        assert!(!as_json.contains("\"engine\""));
    }

    #[test]
    fn format_fetched_multiple_separates_sections() {
        let docs = vec![
            sample_doc("One", "https://a.test", None, "## Sub A"),
            sample_doc("Two", "https://b.test", None, "## Sub B"),
        ];
        let md = format_fetched(&docs, false, false);
        assert!(md.contains("# One\n\n- URL: https://a.test\n\n## Sub A"));
        assert!(md.contains("\n\n---\n\n# Two\n\n- URL: https://b.test\n\n## Sub B"));
    }

    #[test]
    fn format_gathered_nests_sources_under_query() {
        let docs = vec![sample_doc(
            "Tokio Select",
            "https://tokio.rs",
            Some("github"),
            "### Cancellation\n\nDetails.",
        )];
        let md = format_gathered("tokio cancel safety", &docs, false, false);
        assert!(md.starts_with("# Search: tokio cancel safety\n\n## [1] Tokio Select"));
        assert!(md.contains("- Engine: github"));
        assert!(md.contains("### Cancellation"));
    }
}
