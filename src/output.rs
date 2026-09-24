//! Terminal rendering: aligned table or JSON.

use crate::engine::SearchResult;

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
