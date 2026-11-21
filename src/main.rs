//! faro: hybrid multi-engine search CLI (SearXNG first, obscura fallback).

mod ai;
mod bootstrap;
mod engine;
mod extract;
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

        /// Validate relevance and deduplicate using local sagaz (Laya / JEV).
        #[arg(long, aliases = ["sagaz"])]
        validate: bool,

        /// Validate relevance and deduplicate using TypeSafe JEV (cloud System One model).
        #[arg(long)]
        jev: bool,
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

        /// Override port for llama-server (defaults to 43211 for LFM2.5 or 43212 for K2).
        #[arg(long)]
        llama_port: Option<u16>,

        /// Validate relevance and deduplicate using local sagaz (Laya / JEV) before AI synthesis.
        #[arg(long, aliases = ["sagaz"])]
        validate: bool,

        /// Validate relevance and deduplicate using TypeSafe JEV (cloud System One model).
        #[arg(long)]
        jev: bool,
    },
    /// Fetch one or more URLs via obscura and emit clean Markdown or text.
    Fetch {
        /// One or more HTTP(S) URLs to fetch.
        #[arg(required = true)]
        urls: Vec<String>,

        /// CSS selector to narrow content extraction (e.g. "main", "article.post").
        #[arg(long)]
        selector: Option<String>,

        /// Emit plain text instead of Markdown.
        #[arg(long)]
        text: bool,

        /// Emit raw HTML fetched by obscura without conversion.
        #[arg(long, conflicts_with_all = ["text", "json"])]
        raw_html: bool,

        /// Emit machine-readable JSON instead of Markdown.
        #[arg(long, conflicts_with = "text")]
        json: bool,

        /// Per-URL fetch timeout in seconds.
        #[arg(long, default_value_t = 60)]
        timeout: u64,

        /// Maximum number of concurrent URL fetches.
        #[arg(long, default_value_t = 4)]
        concurrency: usize,

        /// Permit fetching loopback / RFC1918 / link-local URLs.
        #[arg(long)]
        allow_private_network: bool,

        /// Write output to a file instead of stdout.
        #[arg(short = 'o', long)]
        output: Option<std::path::PathBuf>,
    },
    /// Search across engines, optionally validate results, and fetch clean Markdown for top pages.
    Gather {
        /// Search query (e.g. "rust tokio cancel safety").
        query: String,

        /// Engines to query (repeatable). Defaults to all validated engines.
        #[arg(long = "engine", value_enum)]
        engines: Vec<EngineKind>,

        /// Maximum search results per engine.
        #[arg(long, default_value_t = 3)]
        limit: usize,

        /// Maximum number of unique result pages to fetch.
        #[arg(long, default_value_t = 5)]
        max_pages: usize,

        /// Per-engine and per-page fetch timeout in seconds.
        #[arg(long, default_value_t = 60)]
        timeout: u64,

        /// Maximum number of concurrent page fetches.
        #[arg(long, default_value_t = 3)]
        concurrency: usize,

        /// CSS selector to narrow content extraction.
        #[arg(long)]
        selector: Option<String>,

        /// Emit plain text instead of Markdown.
        #[arg(long)]
        text: bool,

        /// Emit machine-readable JSON instead of Markdown.
        #[arg(long, conflicts_with = "text")]
        json: bool,

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

        /// Validate relevance and deduplicate using local sagaz (Laya / JEV) before fetching pages.
        #[arg(long, aliases = ["sagaz"])]
        validate: bool,

        /// Validate relevance and deduplicate using TypeSafe JEV before fetching pages.
        #[arg(long)]
        jev: bool,

        /// Write output to a file instead of stdout.
        #[arg(short = 'o', long)]
        output: Option<std::path::PathBuf>,
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
    validate: bool,
    jev: bool,
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
                validate,
                jev,
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
                validate,
                jev,
            },
            Command::Update | Command::Ask { .. } | Command::Fetch { .. } | Command::Gather { .. } => {
                unreachable!("non-search command handled separately")
            }
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

async fn apply_validation(
    query: &str,
    results: Vec<engine::SearchResult>,
    validate: bool,
    use_jev: bool,
) -> anyhow::Result<Vec<engine::SearchResult>> {
    if !use_jev && !validate {
        return Ok(results);
    }

    let query_clone = query.to_string();
    tokio::task::spawn_blocking(move || {
        if use_jev {
            ai::jev::validate_and_deduplicate(
                &query_clone,
                &results,
                ai::sagaz::DEFAULT_RELEVANCE_THRESHOLD,
                ai::sagaz::DEFAULT_DUPLICATE_THRESHOLD,
            )
        } else {
            ai::sagaz::validate_and_deduplicate(
                &query_clone,
                &results,
                ai::sagaz::DEFAULT_RELEVANCE_THRESHOLD,
                ai::sagaz::DEFAULT_DUPLICATE_THRESHOLD,
            )
        }
    })
    .await?
}

async fn run_search(args: SearchArgs) -> anyhow::Result<()> {
    let json = args.json;
    let markdown = args.markdown;
    let with_snippet = args.with_snippet;
    let results = fetch_search_results(&args).await?;
    let results = apply_validation(&args.query, results, args.validate, args.jev).await?;

    output::render(&results, json, markdown, with_snippet);
    Ok(())
}

async fn run_ask(
    query: String,
    args: SearchArgs,
    model: Option<std::path::PathBuf>,
    llama_port: Option<u16>,
) -> anyhow::Result<()> {
    eprintln!("faro: searching web across engines for context...");
    let results = fetch_search_results(&args).await?;
    let results = apply_validation(&query, results, args.validate, args.jev).await?;

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

struct FetchTarget {
    url: String,
    fallback_title: String,
    engine: Option<String>,
    snippet: Option<String>,
}

#[derive(Clone)]
struct FetchOptions {
    format: extract::ExtractFormat,
    selector: Option<String>,
    heading_offset: u8,
    raw_html: bool,
    timeout: u64,
    concurrency: usize,
    allow_private_network: bool,
}

fn validate_http_url(url: &str) -> anyhow::Result<()> {
    let lower = url.to_ascii_lowercase();
    if lower.starts_with("http://") || lower.starts_with("https://") {
        return Ok(());
    }
    anyhow::bail!("unsupported URL {url:?}: expected http:// or https://")
}

async fn fetch_and_extract_one(
    obscura: &std::path::Path,
    target: FetchTarget,
    opts: &FetchOptions,
) -> anyhow::Result<output::FetchedDocument> {
    validate_http_url(&target.url)?;
    let html =
        runner::fetch_html_opts(obscura, &target.url, opts.timeout, opts.allow_private_network)
            .await?;
    if opts.raw_html {
        return Ok(output::FetchedDocument {
            url: target.url,
            title: target.fallback_title,
            engine: target.engine,
            snippet: target.snippet,
            content: html,
        });
    }

    let page = extract::extract_page(
        &html,
        &target.url,
        opts.format,
        opts.selector.as_deref(),
        opts.heading_offset,
    )?;
    if page.content.is_empty() {
        anyhow::bail!("extracted empty content");
    }

    let title = if page.title.is_empty() {
        target.fallback_title
    } else {
        page.title
    };

    Ok(output::FetchedDocument {
        url: target.url,
        title,
        engine: target.engine,
        snippet: target.snippet,
        content: page.content,
    })
}

fn spawn_fetch(
    set: &mut tokio::task::JoinSet<(usize, String, anyhow::Result<output::FetchedDocument>)>,
    obscura: &std::path::Path,
    idx: usize,
    target: FetchTarget,
    opts: &std::sync::Arc<FetchOptions>,
) {
    let obscura = obscura.to_path_buf();
    let opts = std::sync::Arc::clone(opts);
    let url_for_err = target.url.clone();
    set.spawn(async move {
        let res = fetch_and_extract_one(&obscura, target, &opts).await;
        (idx, url_for_err, res)
    });
}

async fn fetch_pages_bounded(
    targets: Vec<FetchTarget>,
    opts: FetchOptions,
) -> anyhow::Result<Vec<output::FetchedDocument>> {
    if targets.is_empty() {
        return Ok(Vec::new());
    }

    let obscura = bootstrap::ensure_obscura().await?;
    let max_concurrent = opts.concurrency.max(1);
    let opts = std::sync::Arc::new(opts);
    let mut pending = targets.into_iter().enumerate();
    let mut set = tokio::task::JoinSet::new();

    for _ in 0..max_concurrent {
        let Some((idx, target)) = pending.next() else {
            break;
        };
        spawn_fetch(&mut set, &obscura, idx, target, &opts);
    }

    let mut collected: Vec<(usize, output::FetchedDocument)> = Vec::new();
    while let Some(joined) = set.join_next().await {
        match joined {
            Ok((idx, _url, Ok(doc))) => collected.push((idx, doc)),
            Ok((_idx, url, Err(err))) => eprintln!("fetch: {url}: {err}"),
            Err(err) => eprintln!("fetch task failed: {err}"),
        }

        if let Some((idx, target)) = pending.next() {
            spawn_fetch(&mut set, &obscura, idx, target, &opts);
        }
    }

    collected.sort_by_key(|(idx, _)| *idx);
    Ok(collected.into_iter().map(|(_, doc)| doc).collect())
}

fn write_output(rendered: &str, output_path: Option<&std::path::Path>) -> anyhow::Result<()> {
    if let Some(path) = output_path {
        if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(path, format!("{rendered}\n"))?;
    } else {
        println!("{rendered}");
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
async fn run_fetch(
    urls: Vec<String>,
    selector: Option<String>,
    text: bool,
    raw_html: bool,
    json: bool,
    timeout: u64,
    concurrency: usize,
    allow_private_network: bool,
    output_path: Option<std::path::PathBuf>,
) -> anyhow::Result<()> {
    for url in &urls {
        validate_http_url(url)?;
    }

    let heading_offset = if urls.len() > 1 { 1 } else { 0 };
    let targets: Vec<FetchTarget> = urls
        .into_iter()
        .map(|url| FetchTarget {
            url,
            fallback_title: String::new(),
            engine: None,
            snippet: None,
        })
        .collect();

    let opts = FetchOptions {
        format: if text {
            extract::ExtractFormat::Text
        } else {
            extract::ExtractFormat::Markdown
        },
        selector,
        heading_offset,
        raw_html,
        timeout,
        concurrency,
        allow_private_network,
    };

    let docs = fetch_pages_bounded(targets, opts).await?;
    if docs.is_empty() {
        anyhow::bail!("failed to fetch content from any requested URL");
    }

    let rendered = if raw_html {
        docs.iter()
            .map(|d| d.content.as_str())
            .collect::<Vec<_>>()
            .join("\n")
    } else {
        output::format_fetched(&docs, json, text)
    };
    write_output(&rendered, output_path.as_deref())
}

#[allow(clippy::too_many_arguments)]
async fn run_gather(
    query: String,
    args: SearchArgs,
    max_pages: usize,
    concurrency: usize,
    selector: Option<String>,
    text: bool,
    json: bool,
    output_path: Option<std::path::PathBuf>,
) -> anyhow::Result<()> {
    eprintln!("faro: searching web across engines...");
    let results = fetch_search_results(&args).await?;
    let mut results = apply_validation(&query, results, args.validate, args.jev).await?;
    results.truncate(max_pages);

    if results.is_empty() {
        let rendered = output::format_gathered(&query, &[], json, text);
        return write_output(&rendered, output_path.as_deref());
    }

    let total_candidates = results.len();
    eprintln!("faro: fetching clean content for {total_candidates} page(s)...");
    let targets: Vec<FetchTarget> = results
        .into_iter()
        .map(|r| FetchTarget {
            url: r.url,
            fallback_title: r.title,
            engine: Some(r.engine),
            snippet: Some(r.snippet),
        })
        .collect();

    let opts = FetchOptions {
        format: if text {
            extract::ExtractFormat::Text
        } else {
            extract::ExtractFormat::Markdown
        },
        selector,
        heading_offset: 2,
        raw_html: false,
        timeout: args.timeout,
        concurrency,
        allow_private_network: false,
    };

    let docs = fetch_pages_bounded(targets, opts).await?;
    if docs.is_empty() {
        anyhow::bail!("all {total_candidates} search result page fetches failed");
    }

    let rendered = output::format_gathered(&query, &docs, json, text);
    write_output(&rendered, output_path.as_deref())
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
            validate,
            jev,
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
                validate,
                jev,
            };
            run_ask(query, search_args, model, llama_port).await
        }
        Command::Fetch {
            urls,
            selector,
            text,
            raw_html,
            json,
            timeout,
            concurrency,
            allow_private_network,
            output,
        } => {
            run_fetch(
                urls,
                selector,
                text,
                raw_html,
                json,
                timeout,
                concurrency,
                allow_private_network,
                output,
            )
            .await
        }
        Command::Gather {
            query,
            engines,
            limit,
            max_pages,
            timeout,
            concurrency,
            selector,
            text,
            json,
            no_searxng,
            searxng_port,
            searxng_url,
            validate,
            jev,
            output,
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
                validate,
                jev,
            };
            run_gather(
                query,
                search_args,
                max_pages,
                concurrency,
                selector,
                text,
                json,
                output,
            )
            .await
        }
        Command::Update => update::run(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validate_http_url_accepts_http_and_https_only() {
        assert!(validate_http_url("https://example.com/docs").is_ok());
        assert!(validate_http_url("http://127.0.0.1:8080/page").is_ok());
        assert!(validate_http_url("file:///etc/passwd").is_err());
        assert!(validate_http_url("example.com").is_err());
    }

    #[test]
    fn cli_parses_fetch_and_gather_subcommands() {
        let fetch = Args::try_parse_from([
            "faro",
            "fetch",
            "https://tokio.rs/tokio/tutorial/select",
            "--selector",
            "main",
            "--allow-private-network",
        ])
        .unwrap();
        assert!(matches!(fetch.command, Command::Fetch { .. }));

        let gather = Args::try_parse_from([
            "faro",
            "gather",
            "rust cancel safety",
            "--max-pages",
            "3",
            "--validate",
        ])
        .unwrap();
        assert!(matches!(gather.command, Command::Gather { .. }));
    }
}
