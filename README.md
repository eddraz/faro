# faro

Multi-engine web search CLI powered by the [obscura](https://github.com/h4ckf0r0day/obscura) headless browser.

Run one query across five validated search engines and print aggregated results as a table or JSON.

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

On first run the CLI looks for `obscura` on `PATH` (then `~/.local/bin`); if missing, it downloads the matching release tarball from obscura's GitHub releases and installs it there. Supported platforms: Linux x86_64/aarch64, macOS x86_64/aarch64.

Build from source:

```bash
cargo build --release
```

## Usage

```bash
# Query every engine (default limit: 10 per engine)
faro "rust programming"

# Only some engines, tighter limit
faro "rust programming" --engine github --engine wikipedia --limit 3

# JSON output
faro "rust programming" --json

# Table with snippets
faro "rust programming" --with-snippet

# Per-engine fetch timeout
faro "rust programming" --timeout 30
```

Options:

| Flag | Effect |
|---|---|
| `--engine <NAME>` | repeatable filter: `github`, `duckduckgo`, `bing`, `yahoo`, `wikipedia`, `google`, `brave`, `qwant` |
| `--limit <N>` | max results per engine (default 10) |
| `--json` | machine-readable JSON (always includes snippets) |
| `--with-snippet` | include snippets in table output |
| `--timeout <SECS>` | per-engine fetch timeout (default 60) |

Engines that fail (network, blockpage, timeout) print one error line to stderr and never sink the run.

## Hybrid mode: SearXNG first, obscura fallback

By default the CLI runs a local [SearXNG](https://docs.searxng.org/) container (rootless podman) and asks it first: one HTTP call covers every engine with normalized JSON. When SearXNG reports an upstream engine as unresponsive (rate limit, CAPTCHA — it lists them in `unresponsive_engines`), that engine skips SearXNG and its results come from the obscura path instead. Engines that return too few results get obscura fill-up with URL dedup.

Container lifecycle (only when the SearXNG path is taken):

- healthy check on `http://127.0.0.1:8888/healthz` — reuse if OK
- stopped container: `podman start searxng`
- missing container: image pull, `settings.yml` written once (JSON API enabled, limiter off), container created bound to `127.0.0.1:8888`
- podman itself is auto-installed with visible sudo if missing (apt/dnf); container networking prefers pasta, falls back to slirp4netns, or installs pasta (package `passt`) as a last resort

Flags: `--no-searxng` (pure obscura), `--searxng-port <PORT>` (default 8888).

## How it works

SearXNG answers through its JSON API; obscura-backed engines are fetched by spawning `obscura fetch <url> --stealth --dump html` and parsed with CSS selectors (`scraper` crate). Parser correctness is covered by fixture-based unit tests built from real captured pages (`cargo test`).

## Tests

```bash
cargo test
```
