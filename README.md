# websearch

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

Google, Brave, Ecosia, Yandex, Startpage and Qwant are intentionally excluded: they CAPTCHA-block headless traffic or render results through async APIs that never land in the DOM.

## Install

On first run the CLI looks for `obscura` on `PATH` (then `~/.local/bin`); if missing, it downloads the matching release tarball from obscura's GitHub releases and installs it there. Supported platforms: Linux x86_64/aarch64, macOS x86_64/aarch64.

Build from source:

```bash
cargo build --release
```

## Usage

```bash
# Query every engine (default limit: 10 per engine)
websearch "rust programming"

# Only some engines, tighter limit
websearch "rust programming" --engine github --engine wikipedia --limit 3

# JSON output
websearch "rust programming" --json

# Table with snippets
websearch "rust programming" --with-snippet

# Per-engine fetch timeout
websearch "rust programming" --timeout 30
```

Options:

| Flag | Effect |
|---|---|
| `--engine <NAME>` | repeatable filter: `github`, `duckduckgo`, `bing`, `yahoo`, `wikipedia` |
| `--limit <N>` | max results per engine (default 10) |
| `--json` | machine-readable JSON (always includes snippets) |
| `--with-snippet` | include snippets in table output |
| `--timeout <SECS>` | per-engine fetch timeout (default 60) |

Engines that fail (network, blockpage, timeout) print one error line to stderr and never sink the run.

## How it works

Each engine is fetched by spawning `obscura fetch <url> --stealth --dump html` and parsed with CSS selectors (`scraper` crate). Parser correctness is covered by fixture-based unit tests built from real captured pages (`cargo test`).

## Tests

```bash
cargo test
```
