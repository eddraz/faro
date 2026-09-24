# Feature: needle-validation

Replace the sagaz (Laya) validation backend with needle (Needle 3) for
per-result relevance validation in `faro search` / `faro ask`.

Goal: each search result is validated for usefulness; useless results are
discarded before reaching the user.

## Constraints / decisions

- User explicitly chose needle over sagaz for this task (2026-xx conversation).
- Prototype gate: if needle3 cannot emit usable relevance verdicts, report
  back and stop instead of shipping a bad backend.
- Keep standard search fast: validation stays behind `--validate`.
- Keep batch-style evaluation where possible (needle is autoregressive; batch
  all snippets into one generation if the interface allows).

## Tasks

- [x] 1. Fetch the needle JAX engine (`needle fetch`) and confirm `needle run` works with `~/models/needle3.cact`.
  - Installed `cactus-needle[train]` extras (jax, flax, sentencepiece) into the uv tool venv.
  - Compiled engine `needle-engine` runs: 0.16s startup, 78MB RAM, JSON output with calibrated `confidence`, HTTP `--serve` mode.
- [x] 2. Prototype: GO/NO-GO gate. **Result: NO-GO.**
  - Batch test: discarded the best result (rust-lang.org); `reasoning` shows literal extraction from the prompt, not semantic judgment.
  - Per-result test A (good snippet): wrongly called `discard_results` (confidence 0.85).
  - Per-result test B (eBay ad): degenerate output `indexes:[0,0,0]`, repetition loop in reasoning, confidence 0.34.
  - Conclusion: needle3 base (34MB) is a tool-call router (weather/calculator/command style), not a semantic relevance judge. Prompting cannot fix a capacity limitation; would need a dedicated fine-tune (`needle finetune` + `generate-data`).
  - Latency note even if quality were fixed: ~1.3s per result vs Laya ~35ms batched.
- [ ] 3. BLOCKED on user decision: keep sagaz, or start a needle fine-tune project.
- [ ] 4. (dropped with no-go) CLI flags/README changes.
- [ ] 5. (dropped with no-go) Tests/live run.
- [ ] 6. (dropped with no-go) Feature branch commits.

## Evidence

- 3 prototype runs logged in session transcript (2026, `/tmp/needle-proto/`).
- Engine environment fix: `uv pip install --python ~/.local/share/uv/tools/cactus-needle/bin/python "cactus-needle[train]"` + `needle fetch`.
