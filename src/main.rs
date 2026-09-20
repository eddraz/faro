//! websearch: multi-engine web search CLI powered by the obscura headless browser.

mod bootstrap;
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

    let selected: Vec<String> = if args.engines.is_empty() {
        [
            EngineKind::Github,
            EngineKind::Duckduckgo,
            EngineKind::Bing,
            EngineKind::Yahoo,
            EngineKind::Wikipedia,
        ]
        .iter()
        .map(|k| k.as_str().to_string())
        .collect()
    } else {
        args.engines.iter().map(|k| k.as_str().to_string()).collect()
    };

    let obscura = bootstrap::ensure_obscura().await?;
    let _ = runner::fetch_html(&obscura, "https://example.com", args.timeout).await;
    println!("engines: {}", selected.join(", "));
    println!("query: {} (limit {})", args.query, args.limit);
    Ok(())
}
