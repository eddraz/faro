# Feature: jev-validation

Add TypeSafe JEV (cloud System One model) as an alternative validation backend
for `faro search` / `faro ask`, behind a `--jev` flag, reusing the existing
sagaz filter logic and thresholds.

## Constraints / decisions

- API: POST https://api.typesafe.ai/v1/systemone, `Authorization: Bearer
  $TYPESAFE_API_KEY`, body `{state, model: "jev-latest", questions}`,
  answers under the same ids with `{type: "noul", noul: f64}`.
- The existing sagaz payload shape already matches the TypeSafe question
  schema (noul/instructions); JEV backend adds `model` + `criteria` rubrics
  (docs recommend criteria for ambiguous boundaries — targets the relevance
  calibration defect found on 2026-09 live testing).
- Graceful degradation: missing/empty `TYPESAFE_API_KEY`, HTTP error, or parse
  failure -> stderr warning + results returned unvalidated (same pattern as
  sagaz without binary).
- `--jev` takes precedence over `--validate`/`--sagaz`; those stay unchanged.
- HTTP client: `ureq` (already in dependencies). No new deps.
- Generated artifacts (code, comments, docs) in English.

## Tasks

- [ ] 1. Create `feat/jev-validation` branch from `feat/sagaz-validation`.
- [ ] 2. Implement `src/ai/jev.rs`: api key lookup, JEV payload builder with
      criteria, runner with graceful degradation, unit tests. Register in
      `src/ai/mod.rs`.
- [ ] 3. Wire `--jev` flag into `Command::Search`/`Command::Ask` in
      `src/main.rs` (precedence over validate/sagaz, threaded through
      `run_search`/`run_ask` like `validate`).
- [ ] 4. README: `--jev` in flags table + TypeSafe JEV section (env var,
      endpoint, no cold start).
- [ ] 5. Verification: `cargo test` green; live graceful-degradation run
      (unset key -> warning + unvalidated results); full live run pending
      user setting `TYPESAFE_API_KEY`.
- [ ] 6. Work-unit commit(s) on `feat/jev-validation`, conventional commits,
      evidence recorded here.

## Evidence

- Worker (gentle-ai-worker): 4 surfaces changed; deviation noted — `send_string` +
  `serde_json::from_str` instead of ureq json helpers (feature missing, matches
  llama.rs/searxng.rs pattern); plus 30s timeout and parity stderr line.
- Independent verify (gentle-ai-verify): cargo test 36 passed / 0 failed;
  live `search --jev` with key unset -> exit 0, exact stderr
  `jev: TYPESAFE_API_KEY not set; skipping validation.`, 5 results printed;
  `--jev` on search+ask help; `--validate`/`--sagaz` byte-identical to HEAD;
  precedence verified by code reading (jev branch before validate).
- Pending: full live validation with a real `TYPESAFE_API_KEY` (env empty).
- Commit: (pending)
