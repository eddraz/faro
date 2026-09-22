//! AI answer synthesis module.
//!
//! Cascade:
//! 1. Try llama-server with local GGUF.
//! 2. If GGUF is missing or llama-server fails, fall back to in-process Candle.
//! 3. Before running Candle, verify if model weights exist; download if missing.
//! 4. Return the synthesized answer to the user.

pub mod candle;
pub mod llama;
pub mod model;

use std::path::Path;

use anyhow::Result;

use crate::engine::SearchResult;

const SYSTEM_PROMPT: &str = "\
You are Faro, a fast, reliable web search and synthesis assistant. \
Answer the user's question directly, accurately, and concisely based strictly on the provided web search context. \
Always cite your sources inline using [1], [2], etc., matching the references from the context. \
If the provided search results do not contain enough information to answer completely, acknowledge what is known and clarify what is missing. \
Respond in the same language as the user query.";

const DEFAULT_MAX_TOKENS: usize = 512;

/// Format search results into a numbered context for the LLM.
pub fn build_context_prompt(query: &str, results: &[SearchResult]) -> String {
    let mut context = String::new();
    context.push_str("Web Search Results:\n\n");

    for (i, r) in results.iter().enumerate() {
        let index = i + 1;
        context.push_str(&format!(
            "[{index}] Title: {}\n    URL: {}\n    Engine: {}\n",
            r.title, r.url, r.engine
        ));
        if !r.snippet.is_empty() {
            context.push_str(&format!("    Snippet: {}\n", r.snippet));
        }
        context.push('\n');
    }

    context.push_str(&format!("Question: {query}\n\nAnswer:"));
    context
}

/// Synthesize search results into a cited answer following the tiered model pipeline.
pub fn synthesize(
    query: &str,
    results: &[SearchResult],
    custom_model: Option<&Path>,
    llama_port: Option<u16>,
) -> Result<String> {
    if results.is_empty() {
        return Ok("No search results were found to answer the query.".to_string());
    }

    let user_prompt = build_context_prompt(query, results);

    // Step 1: Verify if llama-server / llama-serve exists on the system
    let llama_binary = llama::find_llama_server();
    if let Some(ref binary) = llama_binary {
        // Check if known ports are already active to avoid calling/spawning twice
        let known_ports = llama::check_known_ports();
        eprintln!("checking llama-server ports:");
        for p in &known_ports {
            eprintln!(
                "  - port {} ({} / {}): {}",
                p.port,
                p.name,
                p.model_name,
                if p.is_active { "active" } else { "inactive" }
            );
        }

        // Determine target model, port and name
        let (target_model_path, target_port, model_name) =
            model::resolve_model_and_port(custom_model, llama_port);

        let port_url = format!("http://127.0.0.1:{target_port}");
        let already_active = llama::probe_health(&port_url);

        if already_active {
            eprintln!(
                "llama-server already active on port {target_port} ({model_name}); reusing instance (will not call twice)..."
            );
            match llama::generate(target_port, SYSTEM_PROMPT, &user_prompt, DEFAULT_MAX_TOKENS) {
                Ok(answer) => return Ok(answer),
                Err(e) => {
                    eprintln!(
                        "active llama-server on port {target_port} failed: {e}; falling back to candle in-process..."
                    );
                }
            }
        } else if target_model_path.is_file() {
            eprintln!(
                "tier 1: attempting inference via llama-server with {} on port {target_port}...",
                target_model_path.display()
            );
            match llama::ensure_server(binary, &target_model_path, target_port)
                .and_then(|()| llama::generate(target_port, SYSTEM_PROMPT, &user_prompt, DEFAULT_MAX_TOKENS))
            {
                Ok(answer) => return Ok(answer),
                Err(e) => {
                    eprintln!(
                        "tier 1 (llama-server on port {target_port}) failed: {e}; falling back to candle in-process..."
                    );
                }
            }
        } else {
            eprintln!(
                "tier 1: GGUF model file {} not found; falling back to candle in-process...",
                target_model_path.display()
            );
        }
    } else {
        eprintln!(
            "tier 1: neither llama-server nor llama-serve found on system; falling back to candle in-process..."
        );
    }

    // Step 2: Fall back to in-process Candle inference
    eprintln!("tier 2: verifying candle model weights in ~/models ...");
    let models_dir = model::default_models_dir();
    let (candle_model_path, candle_tokenizer_path) = model::ensure_candle_weights(&models_dir)?;

    candle::generate(
        &candle_model_path,
        &candle_tokenizer_path,
        SYSTEM_PROMPT,
        &user_prompt,
        DEFAULT_MAX_TOKENS,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_context_prompt_formats_numbered_results() {
        let results = vec![SearchResult {
            engine: "wikipedia".into(),
            title: "Rust".into(),
            url: "https://rust-lang.org".into(),
            snippet: "A language empowering everyone.".into(),
        }];

        let prompt = build_context_prompt("what is rust?", &results);
        assert!(prompt.contains("[1] Title: Rust"));
        assert!(prompt.contains("URL: https://rust-lang.org"));
        assert!(prompt.contains("Snippet: A language empowering everyone."));
        assert!(prompt.contains("Question: what is rust?"));
    }
}
