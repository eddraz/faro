//! faro: hybrid multi-engine search CLI (SearXNG first, obscura fallback).

mod ai;
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

        /// Emit results formatted as Markdown links and blockquotes.
        #[arg(long)]
        markdown: bool,

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

        /// External SearXNG instance URL (e.g. "http://localhost:8888").
        /// Skips local container startup.
        #[arg(long, env = "FARO_SEARXNG_URL")]
        searxng_url: Option<String>,
    },
    /// Search the web and synthesize an AI-generated answer using a local LLM.
    Ask {
        /// Question or topic to ask (e.g. "What is Rust ownership?").
        query: String,

        /// Engines to query (repeatable). Defaults to all validated engines.
        #[arg(long = "engine", value_enum)]
        engines: Vec<EngineKind>,

        /// Maximum results per engine to feed into context.
        #[arg(long, default_value_t = 3)]
        limit: usize,

        /// Per-engine fetch timeout in seconds.
        #[arg(long, default_value_t = 60)]
        timeout: u64,

        /// Skip SearXNG entirely: query only the obscura-backed engines.
        #[arg(long)]
        no_searxng: bool,

        /// Local port where the SearXNG container is published.
        #[arg(long, default_value_t = searxng::DEFAULT_PORT)]
        searxng_port: u16,

        /// External SearXNG instance URL (e.g. "http://localhost:8888").
        /// Skips local container startup.
        #[arg(long, env = "FARO_SEARXNG_URL")]
        searxng_url: Option<String>,

        /// Override path to the GGUF model file.
        #[arg(long, env = "FARO_MODEL")]
        model: Option<std::path::PathBuf>,

        /// Override port for llama-server.
        #[arg(long, default_value_t = 8080)]
        llama_port: u16,
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
    markdown: bool,
    timeout: u64,
    with_snippet: bool,
    no_searxng: bool,
    searxng_port: u16,
    searxng_url: Option<String>,
}

impl From<Command> for SearchArgs {
    fn from(command: Command) -> Self {
        match command {
            Command::Search {
                query,
                engines,
                limit,
                json,
                markdown,
                timeout,
                with_snippet,
                no_searxng,
                searxng_port,
                searxng_url,
            } => Self {
                query,
                engines,
                limit,
                json,
                markdown,
                timeout,
                with_snippet,
                no_searxng,
                searxng_port,
                searxng_url,
            },
            Command::Update => unreachable!("update is handled separately"),
            Command::Ask { .. } => unreachable!("ask is handled separately"),
        }
    }
}

async fn fetch_search_results(args: &SearchArgs) -> anyhow::Result<Vec<engine::SearchResult>> {
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

    let mut failures: Vec<(String, String)> = Vec::new();
    let mut searxng_results: Vec<engine::SearchResult> = Vec::new();
    let mut unresponsive: Vec<String> = Vec::new();

    // Cascade phase 1: the SearXNG container or external URL (started on demand,
    // reused when already healthy). Any failure here just degrades to pure obscura.
    if !args.no_searxng {
        let query = args.query.clone();
        let engines = selected.clone();
        let fetch_limit = args.limit * selected.len();
        if let Some(ref searxng_url) = args.searxng_url {
            let base_url = searxng_url.clone();
            match tokio::task::spawn_blocking(move || {
                searxng::search_url(&base_url, &query, Some(&engines), fetch_limit)
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
        } else {
            let port = args.searxng_port;
            match tokio::task::spawn_blocking(move || searxng::ensure_ready(port)).await {
                Ok(Ok(())) => {
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
    }

    // Cascade phase 2 (lazy): obscura engines fill remaining gaps per engine.
    // SearXNG-only engines have no obscura parser and are skipped here.
    // Engines already satisfying the limit through SearXNG are also skipped.
    let searxng_failed = !args.no_searxng
        && searxng_results.is_empty()
        && failures.iter().any(|(n, _)| n == "searxng");
    let mut engines_needing_obscura: Vec<String> = Vec::new();

    for kind in &selected {
        if requires_searxng_name(kind) {
            continue;
        }
        let is_degraded = unresponsive.iter().any(|entry| entry.starts_with(kind.as_str()));
        let count = searxng_results.iter().filter(|r| r.engine == *kind).count();
        if args.no_searxng || searxng_failed || is_degraded || count < args.limit {
            engines_needing_obscura.push(kind.clone());
        }
    }

    let mut obscura_results: HashMap<String, Vec<engine::SearchResult>> = HashMap::new();
    if !engines_needing_obscura.is_empty() {
        let obscura = bootstrap::ensure_obscura().await?;
        let mut handles = Vec::new();
        for kind in &engines_needing_obscura {
            let obscura = obscura.clone();
            let query = args.query.clone();
            let limit = args.limit;
            let timeout = args.timeout;
            let name = kind.clone();
            handles.push((
                name.clone(),
                tokio::spawn(async move {
                    engine::run_engine(&name, &obscura, &query, limit, timeout).await
                }),
            ));
        }

        for (name, handle) in handles {
            match handle.await {
                Ok(Ok(results)) => {
                    obscura_results.insert(name, results);
                }
                Ok(Err((err_name, error))) => failures.push((err_name, error)),
                Err(error) => failures.push((name, error.to_string())),
            }
        }
    }

    for (name, error) in &failures {
        eprintln!("{name}: {error}");
    }

    Ok(engine::cascade_merge(
        &selected,
        args.limit,
        searxng_results,
        &unresponsive,
        obscura_results,
    ))
}

async fn run_search(args: SearchArgs) -> anyhow::Result<()> {
    let json = args.json;
    let markdown = args.markdown;
    let with_snippet = args.with_snippet;
    let results = fetch_search_results(&args).await?;
    output::render(&results, json, markdown, with_snippet);
    Ok(())
}

async fn run_ask(
    query: String,
    args: SearchArgs,
    model: Option<std::path::PathBuf>,
    llama_port: u16,
) -> anyhow::Result<()> {
    eprintln!("faro: searching web across engines for context...");
    let results = fetch_search_results(&args).await?;
    let answer = ai::synthesize(&query, &results, model.as_deref(), llama_port)?;

    println!("\n{answer}\n");

    if !results.is_empty() {
        println!("--- Sources ---");
        for (i, r) in results.iter().enumerate() {
            println!("[{}] {} — {} ({})", i + 1, r.title, r.url, r.engine);
        }
    }
    Ok(())
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args = Args::parse();

    match args.command {
        Command::Search { .. } => run_search(args.command.into()).await,
        Command::Ask {
            query,
            engines,
            limit,
            timeout,
            no_searxng,
            searxng_port,
            searxng_url,
            model,
            llama_port,
        } => {
            let search_args = SearchArgs {
                query: query.clone(),
                engines,
                limit,
                json: false,
                markdown: false,
                timeout,
                with_snippet: true,
                no_searxng,
                searxng_port,
                searxng_url,
            };
            run_ask(query, search_args, model, llama_port).await
        }
        Command::Update => update::run(),
    }
}
