# Changelog

All notable changes to `faro` are documented here.

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
