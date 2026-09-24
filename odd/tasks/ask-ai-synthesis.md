# Feature: ask-ai-synthesis

Add `faro ask "<QUERY>"` subcommand: search the web across multiple engines and synthesize an AI-generated answer using a local LLM with fallback cascade.

## Decisions

- **Subcommand**: `faro ask "<QUERY>"` queries search engines with a default of 3 results per engine, gathers context, and invokes the tiered LLM synthesis cascade.
- **Tier 1 (llama-server)**: Check if GGUF model file exists (`~/models/LFM2.5-230M-F16.gguf` or `--model`). If present, query local `llama-server` on port 8080 (starting child process if not already running).
- **Tier 2 (Candle fallback)**: If the GGUF file is missing or `llama-server` fails, fall back to in-process Candle inference using `quantized_lfm2`.
- **Pre-inference Weight Verification**: Before invoking Candle, check if model weights and tokenizer exist on disk; if missing, automatically download `LFM2.5-230M-F16.gguf` and `LFM2.5-tokenizer.json` from Hugging Face.
- **Output**: Output the synthesized, cited response followed by formatted references to the web sources.

## Tasks

1. [x] Add Candle and tokenizers dependencies to `Cargo.toml`.
2. [x] Implement `src/ai/model.rs` with model discovery and automatic Hugging Face downloads.
3. [x] Implement `src/ai/llama.rs` with `llama-server` lifecycle management and HTTP chat completions.
4. [x] Implement `src/ai/candle.rs` with in-process quantized LFM2.5 inference.
5. [x] Implement `src/ai/mod.rs` orchestrating the prompt formatting and tiered fallback.
6. [x] Add `Command::Ask` in `src/main.rs` and refactor `fetch_search_results`.
7. [x] Unit tests for prompt building, ChatML formatting, and model discovery.
8. [x] Update `README.md`.
