# Feature: websearch-cli

CLI in Rust that wraps the `obscura` headless browser to run a query across
validated search engines and print aggregated results.

## Decisions

- Engine list (validated with obscura 0.2.2 `--stealth`, 2026-09-20):
  - `github` — https://github.com/search?q={query}&type=repositories
  - `duckduckgo` — https://html.duckduckgo.com/html/?q={query} (the public
    duckduckgo.com URL meta-refreshes here; go direct)
  - `bing` — https://www.bing.com/search?q={query}
  - `yahoo` — https://search.yahoo.com/search?p={query}
  - `wikipedia` — https://en.wikipedia.org/w/index.php?search={query}
- Discarded: Google, Brave, Ecosia, Yandex, Startpage, Qwant (captcha/async
  blocks), GitHub users (dropped by user decision; needs login).
- Integration model: subprocess (`obscura fetch <url> --stealth --dump html`),
  never embed as a library. `--wait` flag of obscura hangs; do not use.
- Bootstrap: on first run, if `obscura` is not on PATH, download the release
  tarball for the current OS/arch from GitHub releases, extract, install to
  `~/.local/bin`, verify `obscura --version`. Linux x86_64/aarch64 + macOS.
- HTML parsing with the `scraper` crate (CSS selectors), not regex.
- Output: aligned table by default, `--json` for JSON, `--engine` repeatable
  filter, `--limit` per engine (default 10).

## Tasks

1. [ ] Scaffold crate: `Cargo.toml` (clap, tokio, scraper, serde, serde_json,
       anyhow, tar, flate2, ureq), `src/main.rs` CLI definition, module layout,
       parser fixture tests under `tests/fixtures/` built from real captures in
       `/tmp/obscura-test/results/` (gh_repos.html, bing.html, yahoo.html,
       ddg_html.html, wikipedia.html).
2. [ ] Bootstrap module: PATH detection, release download for OS/arch,
       extraction, install, version verification. Unit-testable URL derivation.
3. [ ] Obscura runner: spawn subprocess with timeout, capture HTML stdout,
       map stderr logs to debug output.
4. [ ] Engine trait + five engines with parsers and fixture-based unit tests:
       github (`href="/owner/repo"` filter), duckduckgo (`result__a`,
       `uddg=` redirect decode), bing (`h2 > a` inside `.b_algo`), yahoo
       (`.algo h3 > a`), wikipedia (`mw-search-result-heading > a`).
5. [ ] Output module: table + JSON rendering, engine filter, limit.
6. [ ] Build green + end-to-end smoke test with a real query through all five
       engines via `cargo run -- "rust programming" --limit 3`.

## Commit plan (work units, on branch feature/websearch-cli)

- WU1: scaffold + bootstrap (task 1 partial + 2)
- WU2: runner + engines + parser tests (tasks 3, 4)
- WU3: output + smoke test fixes (tasks 5, 6)
- WU4: README + docs

## Evidence

- Implementation route deviation: harness subagent worktree binding was broken (session born while the repo had zero commits, bound to the parent /home/eddraz clone); all subagent_run attempts failed. User authorized inline implementation (documented deviation), executed through the serena MCP editing surface.
- Task 1-5: all source files under src/, fixtures under tests/fixtures/ built from real captures. `cargo build` green, `cargo test` 12/12 passed (parser fixtures per engine, bootstrap asset mapping, query encoding, runner fail-fast).
- Task 6: end-to-end smoke `cargo run -- "rust programming" --limit 3` returned results from all five engines; `--json` and `--with-snippet` verified.
- Commits: sliced for review budget: a8a5e18 (bootstrap+runner), 9727fc2 (engines+parsers+output), this commit (docs/chores).
