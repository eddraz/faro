# Feature: engine-and-ux-improvements

Comprehensive improvements to faro: fix obscura fallback handle zip bug, lazy obscura execution, smart URL canonicalization & deduplication, external SearXNG URL support, Docker container fallback, and Markdown output format.

## Decisions

- **P0 Bug Fix**: In `src/main.rs`, spawn handles only for engines needing obscura fallback and store `(name, handle)` pairs, eliminating the index misalignment bug where `selected.zip(handles)` assigned github results to google.
- **P1 Lazy Obscura**: Obscura is only spawned for an engine if SearXNG is disabled (`--no-searxng`), SearXNG failed, the engine was reported degraded in `unresponsive_engines`, or SearXNG returned fewer than `limit` results for that engine. If all engines are satisfied by SearXNG, obscura is neither ensured nor spawned, reducing execution time and CPU/RAM usage.
- **P2 URL Canonicalization**: Normalize URLs before deduplication in `cascade_merge` by stripping marketing/tracking query parameters (`utm_*`, `fbclid`, `gclid`, etc.), lowercasing host, and normalizing trailing slashes.
- **P2 External SearXNG URL**: Allow `--searxng-url <URL>` (or env `FARO_SEARXNG_URL`) to connect directly to an existing SearXNG instance without checking or starting local podman/docker containers.
- **P2 Container Engine Fallback**: If `podman` is absent, check for `docker` before failing or prompting for sudo install.
- **P3 Markdown Output**: Add `--markdown` flag to emit GitHub-flavored markdown links and snippets.

## Tasks

1. [x] Fix zip misalignment in `src/main.rs` obscura handles loop.
2. [x] Implement lazy obscura execution in `src/main.rs`: determine missing/degraded engines before invoking obscura.
3. [x] Implement `canonicalize_url` in `src/engine/mod.rs` and update `cascade_merge` with tests.
4. [x] Support `--searxng-url` in `src/main.rs` and `src/searxng.rs`.
5. [x] Support Docker fallback when Podman is missing in `src/searxng.rs`.
6. [x] Add `--markdown` output support in `src/output.rs` and `src/main.rs`.
7. [x] Update unit tests and README.
