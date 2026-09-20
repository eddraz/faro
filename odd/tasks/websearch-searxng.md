# Feature: websearch-searxng

Hybrid search: SearXNG (podman container, localhost) as first source per engine,
obscura per-engine fallback filling the gap. Cascade semantics chosen by the user:
try one source, then the next, obscura last.

## Decisions

- Cascade chain per requested engine: [searxng(engine) -> obscura(engine)].
- SearXNG engine names match ours exactly: github, duckduckgo, bing, yahoo, wikipedia.
- Fill rule: if searxng returned >= limit results for an engine, skip obscura for it;
  otherwise append obscura results (dedup by URL) up to the limit.
- Container lifecycle (lazy, only when searxng path is taken):
  - health first: GET http://127.0.0.1:8888/healthz (300ms timeout) -> OK means skip everything
  - if down: podman start searxng (container exists) or podman run -d --name searxng
    -p 127.0.0.1:8888:8080 -v ~/apps/searxng/config:/etc/searxng
    -v ~/apps/searxng/data:/var/cache/searxng docker.io/searxng/searxng:latest
  - wait healthy up to 30s (poll 500ms)
  - image pull only if missing (podman image exists)
  - settings.yml written once (search.formats: [html, json], limiter: false,
    public_instance: false, random secret_key)
- podman bootstrap safety net: detect on PATH; if missing, auto-install via detected
  package manager (apt/dnf) with sudo, printing the command first (user decision).
- New flags: --no-searxng (pure obscura), --searxng-port (default 8888).
- SearXNG JSON result fields mapped: url, title, content(snippet), engines(source).
- Security: container binds 127.0.0.1 only.

## Tasks

1. [ ] src/searxng.rs: podman detection + sudo install path, settings.yml generation,
   image/container lifecycle, health wait, JSON client + parser (serde), unit tests
   (arg builders, settings template, JSON fixture parse).
2. [ ] Hybrid cascade in engine dispatch + main.rs: searxng-first per engine,
   obscura fill with URL dedup, --no-searxng and --searxng-port flags.
3. [ ] Real smoke: container up via podman, query through the hybrid path, verify
   results and fallback (stop container -> verify autostart; --no-searxng path).
4. [x] README update + evidence + work-unit commits.
5. [ ] SearXNG-only engines: add google, brave, qwant to EngineKind; searxng
       phase passes the requested engine set to the API (engines= param);
       obscura phase skips them; --no-searxng warns and skips them.
6. [ ] Cascade test for searxng-only engines (healthy + degraded), real smoke
       with --engine google,brave,qwant and default run coverage.
7. [x] README/feature-doc evidence + work-unit commits.

## Evidence (v2: searxng-only engines)

- 18/18 tests green incl. cascade tolerance for searxng-only engines (healthy + degraded).
- Real smoke: `--engine google --engine brave --engine qwant` returned google and brave
  results through the container; qwant degraded (CAPTCHA) and correctly yielded nothing
  (no obscura fallback exists for it). `--no-searxng --engine google` warns and skips.
- Default engine set now includes google/brave/qwant; with `--no-searxng` they are
  skipped with a warning, keeping pure-obscura runs intact.

## Evidence

- `cargo test` 17/17 green (searxng settings template, run args with/without network backend, JSON fixture incl. mixed unresponsive shapes, cascade merge semantics, obscura parsers regression).
- Real smoke (podman 5.4.2, Debian 13): image pulled, settings.yml written once, container created on 127.0.0.1:8888. First run surfaced missing pasta (exit 127) and the cascade fell back to obscura live; resolved by preferring slirp4netns and then pasta (package `passt`) installed by the user.
- Verified: full 5-engine run served by one searxng call (2 results each); `podman stop` -> auto-start on next query reporting `degraded upstream: duckduckgo (CAPTCHA), wikidata (timeout)` with obscura fill; `--no-searxng` pure obscura; `--json` includes searxng results.
- Commits: this feature lands as work units on feature/websearch-cli (see git log: feat(searxng), fix(searxng), docs).
