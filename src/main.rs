//! websearch: multi-engine web search CLI powered by the obscura headless browser.

mod bootstrap;
mod engine;
mod output;
mod runner;

use clap::{Parser, ValueEnum};

/// Multi-engine web search CLI powered by the obscura headless browser.
#[derive(Parser)]
#[command(name = "websearch", version, about)]
struct Args {
    /// Search query (e.g. "rust programming").
    query: String,

    /// Engines to query (repeatable). Defaults to all validated engines.
    #[arg(long = "engine", value_enum)]
    engines: Vec<EngineKind>,

    /// Maximum results per engine.
    #[arg(long, default_value_t = 10)]
    limit: usize,

    /// Emit machine-readable JSON instead of a table.
    #[arg(long)]
    json: bool,

    /// Per-engine fetch timeout in seconds.
    #[arg(long, default_value_t = 60)]
    timeout: u64,

    /// Include snippets in table output (always present in JSON).
    #[arg(long)]
    with_snippet: bool,
}

#[derive(Clone, Copy, Debug, ValueEnum)]
enum EngineKind {
    Github,
    Duckduckgo,
    Bing,
    Yahoo,
    Wikipedia,
}

impl EngineKind {
    fn as_str(self) -> &'static str {
        match self {
            EngineKind::Github => "github",
            EngineKind::Duckduckgo => "duckduckgo",
            EngineKind::Bing => "bing",
            EngineKind::Yahoo => "yahoo",
            EngineKind::Wikipedia => "wikipedia",
        }
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args = Args::parse();

    let selected: Vec<EngineKind> = if args.engines.is_empty() {
        vec![
            EngineKind::Github,
            EngineKind::Duckduckgo,
            EngineKind::Bing,
            EngineKind::Yahoo,
            EngineKind::Wikipedia,
        ]
    } else {
        args.engines.clone()
    };

    let obscura = bootstrap::ensure_obscura().await?;

    let mut handles = Vec::new();
    for kind in selected {
        let obscura = obscura.clone();
        let query = args.query.clone();
        let limit = args.limit;
        let timeout = args.timeout;
        handles.push(tokio::spawn(async move {
            engine::run_engine(kind.as_str(), &obscura, &query, limit, timeout).await
        }));
    }

    let mut all_results: Vec<engine::SearchResult> = Vec::new();
    let mut failures: Vec<(String, String)> = Vec::new();
    for handle in handles {
        match handle.await {
            Ok(Ok(results)) => all_results.extend(results),
            Ok(Err((name, error))) => failures.push((name, error)),
            Err(join_error) => failures.push(("engine".into(), join_error.to_string())),
        }
    }
    for (name, error) in &failures {
        eprintln!("{name}: {error}");
    }

    output::render(&all_results, args.json, args.with_snippet);
    Ok(())
}
