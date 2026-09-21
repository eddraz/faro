//! faro: hybrid multi-engine search CLI (SearXNG first, obscura fallback).

mod bootstrap;
mod engine;
mod output;
mod runner;
mod searxng;
mod update;

use std::collections::HashMap;

use clap::{Parser, Subcommand, ValueEnum};

/// Hybrid multi-engine web search CLI: a local SearXNG container answers
/// first, obscura-backed engines fill the gaps and cover degraded engines.
#[derive(Parser)]
#[command(name = "faro", version, about)]
struct Args {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Search the web across multiple engines.
    Search {
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

        /// Skip SearXNG entirely: query only the obscura-backed engines.
        #[arg(long)]
        no_searxng: bool,

        /// Local port where the SearXNG container is published.
        #[arg(long, default_value_t = searxng::DEFAULT_PORT)]
        searxng_port: u16,
    },
    /// Update faro to the latest release.
    Update,
}

#[derive(Clone, Copy, Debug, ValueEnum)]
enum EngineKind {
    Github,
    Duckduckgo,
    Bing,
    Yahoo,
    Wikipedia,
    /// SearXNG-only: upstream engines that block headless browsers, served
    /// through the local SearXNG container with no obscura fallback.
    Google,
    Brave,
    Qwant,
}

impl EngineKind {
    fn as_str(self) -> &'static str {
        match self {
            EngineKind::Github => "github",
            EngineKind::Duckduckgo => "duckduckgo",
            EngineKind::Bing => "bing",
            EngineKind::Yahoo => "yahoo",
            EngineKind::Wikipedia => "wikipedia",
            EngineKind::Google => "google",
            EngineKind::Brave => "brave",
            EngineKind::Qwant => "qwant",
        }
    }

    /// True for engines the obscura path cannot serve at all.
    fn requires_searxng(self) -> bool {
        matches!(
            self,
            EngineKind::Google | EngineKind::Brave | EngineKind::Qwant
        )
    }
}

/// String-level mirror of EngineKind::requires_searxng for selected names.
fn requires_searxng_name(engine: &str) -> bool {
    matches!(engine, "google" | "brave" | "qwant")
}

#[derive(Clone)]
struct SearchArgs {
    query: String,
    engines: Vec<EngineKind>,
    limit: usize,
    json: bool,
    timeout: u64,
    with_snippet: bool,
    no_searxng: bool,
    searxng_port: u16,
}

impl From<Command> for SearchArgs {
    fn from(command: Command) -> Self {
        match command {
            Command::Search {
                query,
                engines,
                limit,
                json,
                timeout,
                with_snippet,
                no_searxng,
                searxng_port,
            } => Self {
                query,
                engines,
                limit,
                json,
                timeout,
                with_snippet,
                no_searxng,
                searxng_port,
            },
            Command::Update => unreachable!("update is handled separately"),
        }
    }
}

async fn run_search(args: SearchArgs) -> anyhow::Result<()> {
    // Default display order: web results first, github repos last.
    let mut selected: Vec<String> = if args.engines.is_empty() {
        [
            EngineKind::Duckduckgo,
            EngineKind::Bing,
            EngineKind::Yahoo,
            EngineKind::Wikipedia,
            EngineKind::Google,
            EngineKind::Brave,
            EngineKind::Qwant,
            EngineKind::Github,
        ]
        .iter()
        .map(|kind| kind.as_str().to_string())
        .collect()
    } else {
        args.engines
            .iter()
            .map(|kind| kind.as_str().to_string())
            .collect()
    };

    // SearXNG-only engines cannot run without the container: warn and skip
    // them instead of failing the whole run.
    if args.no_searxng {
        let searxng_only: Vec<String> = selected
            .iter()
            .filter(|engine| {
                args.engines
                    .iter()
                    .find(|kind| kind.as_str() == *engine)
                    .is_some_and(|kind| kind.requires_searxng())
            })
            .cloned()
            .collect();
        if !searxng_only.is_empty() {
            eprintln!(
                "{} require the SearXNG container; skipping (drop --no-searxng to use them)",
                searxng_only.join(", ")
            );
            selected.retain(|engine| !searxng_only.contains(engine));
        }
    }

    let obscura = bootstrap::ensure_obscura().await?;
    let mut failures: Vec<(String, String)> = Vec::new();
    let mut searxng_results: Vec<engine::SearchResult> = Vec::new();
    let mut unresponsive: Vec<String> = Vec::new();

    // Cascade phase 1: the SearXNG container (started on demand, reused when
    // already healthy). Any failure here just degrades to pure obscura.
    if !args.no_searxng {
        let port = args.searxng_port;
        match tokio::task::spawn_blocking(move || searxng::ensure_ready(port)).await {
            Ok(Ok(())) => {
                let query = args.query.clone();
                let engines = selected.clone();
                let fetch_limit = args.limit * selected.len();
                let port = args.searxng_port;
                match tokio::task::spawn_blocking(move || {
                    searxng::search(port, &query, Some(&engines), fetch_limit)
                })
                .await
                {
                    Ok(Ok(search)) => {
                        if search.unresponsive_engines.is_empty() {
                            eprintln!("searxng: {} results", search.results.len());
                        } else {
                            eprintln!(
                                "searxng: {} results; degraded upstream: {}",
                                search.results.len(),
                                search.unresponsive_engines.join(", ")
                            );
                        }
                        searxng_results = search.results;
                        unresponsive = search.unresponsive_engines;
                    }
                    Ok(Err(error)) => failures.push(("searxng".into(), error.to_string())),
                    Err(error) => failures.push(("searxng".into(), error.to_string())),
                }
            }
            Ok(Err(error)) => failures.push(("searxng".into(), error.to_string())),
            Err(error) => failures.push(("searxng".into(), error.to_string())),
        }
    }

    // Cascade phase 2: obscura engines fill the remaining gaps per engine.
    // SearXNG-only engines have no obscura parser and are skipped here.
    let mut handles = Vec::new();
    for kind in &selected {
        if requires_searxng_name(kind) {
            continue;
        }
        let obscura = obscura.clone();
        let query = args.query.clone();
        let limit = args.limit;
        let timeout = args.timeout;
        let name = kind.as_str().to_string();
        handles.push(tokio::spawn(async move {
            engine::run_engine(&name, &obscura, &query, limit, timeout).await
        }));
    }

    let mut obscura_results: HashMap<String, Vec<engine::SearchResult>> = HashMap::new();
    for (kind, handle) in selected.iter().zip(handles) {
        match handle.await {
            Ok(Ok(results)) => {
                obscura_results.insert(kind.clone(), results);
            }
            Ok(Err((name, error))) => failures.push((name, error)),
            Err(error) => failures.push(("engine".into(), error.to_string())),
        }
    }
    for (name, error) in &failures {
        eprintln!("{name}: {error}");
    }

    let merged = engine::cascade_merge(
        &selected,
        args.limit,
        searxng_results,
        &unresponsive,
        obscura_results,
    );
    output::render(&merged, args.json, args.with_snippet);
    Ok(())
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args = Args::parse();

    match args.command {
        Command::Search { .. } => run_search(args.command.into()).await,
        Command::Update => update::run(),
    }
}
