# Changelog

All notable changes to `faro` are documented here.

## 0.4.0 — 2026-11-21

- New `faro fetch`: fetch one or more URLs via `obscura` with bounded
  concurrency and extract clean, boilerplate-free Markdown, plain text, raw
  HTML, or JSON (`--selector`, `--text`, `--raw-html`, `--json`,
  `--concurrency`, `--timeout`, `--allow-private-network`, `-o`/`--output`).
- New `faro gather`: search across engines, optionally validate and deduplicate
  results (`--validate`/`--sagaz`, `--jev`), and fetch clean Markdown, plain
  text, or JSON for the top unique pages (`--max-pages`, `--concurrency`,
  `--selector`, `-o`/`--output`).
- New HTML content extractor (`extract`): strips nav/footer/scripts/aside/form
  boilerplate and ARIA noise, prioritizes main/article containers, resolves
  relative links and images, shifts nested headings, and formats code fences,
  lists, blockquotes, and Markdown tables.
- Extracted shared `apply_validation` helper across `search`, `ask`, and
  `gather`, and updated `README.md` with `fetch` and `gather` documentation.

## 0.3.0 — 2026-09-24

- New `faro ask`: AI-synthesized answer over search results using a local
  llama-server, with a candle local-inference fallback when no server is
  reachable.
- Semantic validation and deduplication for `search` and `ask`:
  - `--validate` (alias `--sagaz`) runs the local `sagaz` binary (Laya / JEV),
    packing all top candidates and duplicate pairs into a single batched pass.
  - `--jev` validates with the TypeSafe JEV cloud System One model
    (`TYPESAFE_API_KEY`), with `criteria` rubrics for relevance and duplicate
    judgments. Takes precedence over `--validate` when both are passed.
  - Both degrade gracefully: on missing binary/key or request failure, results
    are returned unvalidated with a diagnostic on stderr.
- Search reliability and UX: URL canonicalization for deduplication, lazy
  obscura execution, external SearXNG via `--searxng-url` (env
  `FARO_SEARXNG_URL`), markdown output, and per-engine error isolation.

## 0.2.0 — 2026-09-21

- **Breaking:** the CLI now uses subcommands. The bare form `faro "<query>"`
  became `faro search "<query>" [flags]`; every flag keeps its name, default
  and behavior.
- New `faro update`: checks the latest GitHub release and, when newer,
  downloads the platform tarball and atomically replaces the running binary
  (temp file + rename; Linux/macOS, x86_64/aarch64).
- Release workflow now emits assets named `faro-<arch>-<os>.tar.gz` — the
  exact names the self-update path downloads.
- Internal workflow docs (`odd/`) are excluded from the published crate.

## 0.1.0 — 2026-09-21

- Initial release: one query across eight engines — github, duckduckgo, bing,
  yahoo, wikipedia via the obscura headless browser; google, brave, qwant via
  SearXNG only.
- Hybrid mode: a local rootless-podman SearXNG container answers first;
  degraded or sparse engines fall back to obscura with URL dedup.
- First-run bootstrap of `obscura` from its GitHub releases.
- Aligned table or JSON output, per-engine limits, repeatable `--engine`
  filter; fixture-based parser tests.
