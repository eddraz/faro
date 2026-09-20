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
4. [ ] README update + evidence + work-unit commits.

## Evidence

(pending)
