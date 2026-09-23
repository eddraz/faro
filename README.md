# faro

Hybrid multi-engine web search CLI powered by a local [SearXNG](https://docs.searxng.org/) container and the [obscura](https://github.com/h4ckf0r0day/obscura) headless browser.

Run one query across eight search engines and print aggregated results as a table or JSON.

## Engines

| Engine | Search URL | Notes |
|---|---|---|
| `github` | `github.com/search?q={query}&type=repositories` | `owner/repo` extraction |
| `duckduckgo` | `html.duckduckgo.com/html/?q={query}` | non-JS endpoint; decodes `uddg=` redirects |
| `bing` | `www.bing.com/search?q={query}` | keeps `/ck/a` tracking URLs |
| `yahoo` | `search.yahoo.com/search?p={query}` | keeps `r.search.yahoo.com` redirects |
| `wikipedia` | `en.wikipedia.org/w/index.php?search={query}` | MediaWiki full-text search |
| `google` | via SearXNG only | blocks headless browsers; served through the container |
| `brave` | via SearXNG only | same |
| `qwant` | via SearXNG only | same; no obscura fallback |

Ecosia, Yandex and Startpage remain excluded: they CAPTCHA-block both paths or render results through async APIs that never land in the DOM.

## Install

From crates.io (requires a Rust toolchain):

```bash
cargo install faro
```

Or grab a prebuilt binary from the [latest release](https://github.com/eddraz/faro/releases/latest) — `faro-x86_64-linux.tar.gz`, `faro-aarch64-linux.tar.gz`, `faro-x86_64-macos.tar.gz` or `faro-aarch64-macos.tar.gz` — and put the `faro` binary on your `PATH`. Installed binaries can upgrade themselves with `faro update`.

Build from source:

```bash
cargo build --release
```

On first run the CLI looks for `obscura` on `PATH` (then `~/.local/bin`); if missing, it downloads the matching release tarball from obscura's GitHub releases and installs it there. Supported platforms: Linux x86_64/aarch64, macOS x86_64/aarch64.

## Usage

```bash
# Query every engine (default limit: 10 per engine)
faro search "rust programming"

# Only some engines, tighter limit
faro search "rust programming" --engine github --engine wikipedia --limit 3

# JSON output
faro search "rust programming" --json

# Table with snippets
faro search "rust programming" --with-snippet

# Per-engine fetch timeout
faro search "rust programming" --timeout 30
```

> Note: faro now uses subcommands. The old bare form `faro "query"` became
> `faro search "query"`.

Options (all under `search`):

| Flag | Effect |
|---|---|
| `--engine <NAME>` | repeatable filter: `github`, `duckduckgo`, `bing`, `yahoo`, `wikipedia`, `google`, `brave`, `qwant` |
| `--limit <N>` | max results per engine (default 10) |
| `--json` | machine-readable JSON (always includes snippets) |
| `--markdown` | emit results formatted as Markdown links and blockquotes |
| `--with-snippet` | include snippets in table output |
| `--timeout <SECS>` | per-engine fetch timeout (default 60) |
| `--no-searxng` | skip SearXNG entirely: pure obscura path |
| `--searxng-port <PORT>` | local port for the SearXNG container (default 8888) |
| `--searxng-url <URL>` | external SearXNG instance URL (env `FARO_SEARXNG_URL`); skips local container |
| `--validate`, `--sagaz` | validate semantic relevance and filter duplicate results using local `sagaz` (Laya / JEV) |

Engines that fail (network, blockpage, timeout) print one error line to stderr and never sink the run.

## AI Synthesis: `faro ask`

```bash
# Ask a question: searches the web and synthesizes an answer with citations using LFM2.5
faro ask "How does ownership work in Rust?"

# With semantic validation and deduplication filter
faro ask "How does ownership work in Rust?" --validate

# Custom GGUF model or llama-server port
faro ask "Explain quantum computing" --model ~/models/LFM2.5-230M-F16.gguf --llama-port 43211
```

`faro ask` executes a smart two-tier LLM inference cascade using **LFM2.5**:

1. **Tier 1 (`llama-server` / `llama-serve`)**:
   - First verifies if `llama-server` or `llama-serve` exists on the system.
   - Probes the dedicated **LFM2.5 port 43211** to avoid duplicate server instances:
     - If port 43211 is already active, queries it directly without re-spawning ("not called twice").
     - If inactive, verifies if the GGUF model exists in `~/models` (`LFM2.5-230M-F16.gguf` by default) and starts a background `llama-server` instance.
2. **Tier 2 (`candle` fallback)**:
   - If the GGUF model does not exist or `llama-server` is unavailable/fails, inference falls back to in-process Candle execution.
   - **Automatic Weight Verification**: Before invoking Candle, verifies if model weights and tokenizer exist on disk; if missing, automatically downloads them from Hugging Face into `~/models`.

Options (under `ask`):

| Flag | Effect |
|---|---|
| `--limit <N>` | max search results per engine for context (default 3) |
| `--model <PATH>` | override path to GGUF model file (env `FARO_MODEL`) |
| `--llama-port <PORT>` | port for `llama-server` (defaults to 43211 for LFM2.5) |
| `--engine <NAME>` | repeatable engine filter (same as `search`) |
| `--no-searxng` | skip SearXNG entirely (pure obscura path) |
| `--searxng-url <URL>` | external SearXNG instance URL |
| `--validate`, `--sagaz` | validate semantic relevance and deduplicate context candidates with `sagaz` before synthesis |

## Semantic Validation & Deduplication: `sagaz` (Laya / JEV)

When running `faro search` or `faro ask` with `--validate` (or `--sagaz`), Faro hooks into your local [sagaz-cli](https://github.com/NandhaKishorM/laya) binary (`~/.local/bin/sagaz`) running the non-autoregressive decision model Laya (JEV):

- **Query Relevance Verification**: Verifies whether each retrieved search snippet truly answers the user's query and drops irrelevant noise or low-quality crawler hits.
- **Pairwise Deduplication**: Detects semantic equivalence across different search engines and drops duplicate snippets even when phrased slightly differently.
- **Batched Execution**: Packs state and all typed questions into a single `sagaz predict -s ... -q ... --json` batch invocation, paying startup costs once and computing all decisions in parallel.
- **Graceful Fallback**: If `sagaz` is missing or fails, Faro prints a diagnostic message and proceeds with unvalidated search results without crashing.

## Updating

```bash
faro update
```

Checks the latest release of [faro](https://github.com/eddraz/faro) on GitHub, compares it against the running version, and if newer downloads the matching prebuilt tarball and atomically replaces the running binary (temp file + rename, safe on Linux/macOS). Reports "up to date" and exits 0 when the latest release is not newer.

Requirements:

- Releases must ship a tarball named after the platform: `faro-x86_64-linux.tar.gz`, `faro-aarch64-linux.tar.gz`, `faro-x86_64-macos.tar.gz`, `faro-aarch64-macos.tar.gz`, each containing a `faro` binary.
- Supported platforms: Linux x86_64/aarch64, macOS x86_64/aarch64.

## Hybrid mode: SearXNG first, obscura fallback

By default the CLI queries a local [SearXNG](https://docs.searxng.org/) container (rootless podman or docker) and asks it first: one HTTP call covers every engine with normalized JSON. Obscura is executed **lazily**: headless browser processes are only launched for engines that SearXNG reported as degraded (`unresponsive_engines`) or that returned fewer results than `--limit`. URL deduplication canonicalizes URLs, automatically stripping tracking query parameters (`utm_*`, `fbclid`, etc.).

Alternatively, point directly to an existing SearXNG instance without running a local container using `--searxng-url https://my-searxng.example.com` or the `FARO_SEARXNG_URL` environment variable.

Container lifecycle (only when the local SearXNG container path is taken):

- healthy check on `http://127.0.0.1:8888/healthz` — reuse if OK
- stopped container: `podman start searxng` (or `docker start searxng`)
- missing container: image pull, `settings.yml` written once (JSON API enabled, limiter off), container created bound to `127.0.0.1:8888`
- prefers `podman`, falling back to `docker` if present; if neither exists, podman is auto-installed with visible sudo (apt/dnf); container networking prefers pasta, falls back to slirp4netns, or installs pasta (package `passt`) as a last resort

Flags: `--no-searxng` (pure obscura), `--searxng-port <PORT>` (default 8888), `--searxng-url <URL>`.

## How it works

SearXNG answers through its JSON API; obscura-backed engines are fetched by spawning `obscura fetch <url> --stealth --dump html` and parsed with CSS selectors (`scraper` crate). Parser correctness is covered by fixture-based unit tests built from real captured pages (`cargo test`).

## Tests

```bash
cargo test
```
