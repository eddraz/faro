# Feature: self-update-command

`faro update` subcommand that self-updates the faro binary from GitHub
Releases, mirroring the bootstrap pattern already used for obscura.

## Decisions

- User-confirmed scope (2026-07-18): update **faro itself**, via **release
  binaries** (not cargo builds).
- CLI restructures to clap subcommands: `faro search <QUERY> [flags]`
  (all current behavior and flags) + `faro update`. Breaking change vs the
  bare positional query; README updated accordingly.
- Release metadata: `GET https://api.github.com/repos/eddraz/faro/releases/latest`,
  read `tag_name` (strip optional leading `v`), compare against
  `env!("CARGO_PKG_VERSION")`. Equal or older remote → "already up to date".
- Asset naming mirrors `bootstrap::release_asset`:
  `faro-x86_64-linux.tar.gz`, `faro-aarch64-linux.tar.gz`,
  `faro-x86_64-macos.tar.gz`, `faro-aarch64-macos.tar.gz`; download base
  `https://github.com/eddraz/faro/releases/latest/download/<asset>`.
- Replace strategy: write extracted `faro` to a temp file next to
  `current_exe()`, chmod 755 (unix), atomic `rename` over the running binary
  (safe on Linux/macOS: running process keeps the old inode).
- Platforms: linux x86_64/aarch64 + macOS x86_64/aarch64 (same as bootstrap).
  Unsupported platform → clear error.
- Out of scope: release CI/workflow that publishes those assets (separate
  user decision). `faro update` end-to-end can only be exercised once a real
  release exists; unit tests cover the pure parts (asset mapping, tag/version
  parsing and comparison).

## Tasks

1. [x] Restructure `src/main.rs` to clap subcommands (`search`, `update`);
       search keeps every existing flag and the default engine order; smoke
       `cargo build` + existing test suite green.
2. [x] New `src/update.rs`: pure helpers (asset mapping, tag parse, version
       comparison) with unit tests; network flow (fetch latest tag, download
       asset, extract, atomic replace of `current_exe`); wire into the
       `update` subcommand.
3. [x] README: subcommand usage + `faro update` section (release asset naming
       requirement); evidence recorded here; work-unit commits on
       `feature/self-update-command`.

## Commit plan (work units)

- WU1: subcommand restructure + update module + tests (tasks 1, 2)
- WU2: README + docs (task 3)

## Evidence

- Implementation: delegated to gentle-ai-worker (surfaces: src/main.rs, src/update.rs). clap restructure to `Subcommand` (search keeps all flags/defaults and engine order; update dispatches to `update::run`).
- Verification: independent gentle-ai-verify pass — `cargo build` green; `cargo test` 24 passed / 0 failed (4 new: release_assets_cover_supported_platforms, parses_version_tags, rejects_invalid_version_tags, compares_versions_correctly); `search --help` / `update --help` correct; diff review clean (no unsafe, no shell exec, bootstrap.rs untouched); error-path smoke exits 1 with clean anyhow message.
- Unverified by sandbox limitation: live GitHub fetch (no network in environment) and the specific no-releases-yet 404 message; requires a networked run once a real release is published.
- Native review disposition (user decision D, 2026-09-21): lineage review-63468667e1619d7c (tier high, base main) closed by user with 3/4 lenses captured and admitted (risk, resilience, reliability); review-readability capture failed 3x on relay transport (400 MissingSessionID, Console Go route requires x-opencode-session — infrastructure, not candidate). RDD switch disabled for this clone (`gentle-ai review mode disable --scope clone`); delivery follows ordinary repository policy. Push/PR = user decision.
