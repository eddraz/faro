//! In-process generation with a quantized LFM2.5 GGUF model via candle.

use std::path::Path;

use anyhow::{anyhow, Result};
use candle::quantized::gguf_file;
use candle::{Device, Tensor};
use candle_transformers::generation::{LogitsProcessor, Sampling};
use candle_transformers::models::quantized_lfm2::ModelWeights;
use candle_transformers::utils::apply_repeat_penalty;
use tokenizers::Tokenizer;

const EOS_TOKEN: &str = "<|im_end|>";
const SEED: u64 = 42;
const REPEAT_PENALTY: f32 = 1.05;

/// Render a system + user turn in LFM2.5's ChatML format.
pub fn render_chat(system: &str, user: &str) -> String {
    format!(
        "<|startoftext|><|im_start|>system\n{system}<|im_end|>\n<|im_start|>user\n{user}<|im_end|>\n<|im_start|>assistant\n"
    )
}

/// Load a quantized LFM2.5 GGUF model and return (model, context_length).
pub fn load_model(model_path: &Path) -> Result<(ModelWeights, usize)> {
    let device = Device::Cpu;
    let mut file = std::fs::File::open(model_path)
        .map_err(|e| anyhow!("failed to open GGUF file {}: {e}", model_path.display()))?;
    let gguf = gguf_file::Content::read(&mut file)
        .map_err(|e| anyhow!("failed to parse GGUF structure: {e}"))?;

    let context_length = gguf
        .metadata
        .get("lfm2.context_length")
        .and_then(|v| v.to_u32().ok().map(|v| v as usize))
        .unwrap_or(8192);

    let model = ModelWeights::from_gguf(gguf, &mut file, &device)
        .map_err(|e| anyhow!("failed to load model weights from GGUF: {e}"))?;

    Ok((model, context_length))
}

/// Load the tokenizer from a tokenizer.json file.
pub fn load_tokenizer(tokenizer_path: &Path) -> Result<Tokenizer> {
    Tokenizer::from_file(tokenizer_path)
        .map_err(|e| anyhow!("failed to load tokenizer from {}: {e}", tokenizer_path.display()))
}

/// Generate an answer in-process using Candle.
pub fn generate(
    model_path: &Path,
    tokenizer_path: &Path,
    system: &str,
    user: &str,
    max_tokens: usize,
) -> Result<String> {
    eprintln!("loading model into candle in-process...");
    let (mut model, context_length) = load_model(model_path)?;
    let tokenizer = load_tokenizer(tokenizer_path)?;
    let prompt = render_chat(system, user);

    let mut tokens = tokenizer
        .encode(prompt.as_str(), false)
        .map_err(|e| anyhow!("failed to encode prompt: {e}"))?
        .get_ids()
        .to_vec();

    if tokens.len() > context_length.saturating_sub(1) {
        tokens.truncate(context_length.saturating_sub(1));
    }

    let eos_id = tokenizer
        .token_to_id(EOS_TOKEN)
        .ok_or_else(|| anyhow!("tokenizer is missing EOS token {EOS_TOKEN}"))?;

    let device = Device::Cpu;
    let sampling = Sampling::TopP {
        p: 0.95,
        temperature: 0.2,
    };
    let mut logits_processor = LogitsProcessor::from_sampling(SEED, sampling);

    let input = Tensor::new(tokens.as_slice(), &device)?
        .unsqueeze(0)?;
    let logits = model
        .forward(&input, 0)?
        .squeeze(0)?;

    let mut next_token = logits_processor.sample(&logits)?;
    tokens.push(next_token);

    let mut generated_tokens = vec![next_token];

    eprintln!("generating response with candle...");
    for _ in 1..max_tokens {
        if next_token == eos_id {
            break;
        }
        let input = Tensor::new(&[next_token], &device)?.unsqueeze(0)?;
        let logits = model.forward(&input, tokens.len() - 1)?.squeeze(0)?;
        let logits = apply_repeat_penalty(&logits, REPEAT_PENALTY, &tokens)?;
        next_token = logits_processor.sample(&logits)?;
        tokens.push(next_token);
        generated_tokens.push(next_token);
    }

    let answer = tokenizer
        .decode(&generated_tokens, true)
        .map_err(|e| anyhow!("failed to decode generated tokens: {e}"))?;

    Ok(answer.trim().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn render_chat_formats_chatml() {
        let chat = render_chat("system prompt", "user query");
        assert!(chat.starts_with("<|startoftext|><|im_start|>system\nsystem prompt<|im_end|>"));
        assert!(chat.contains("<|im_start|>user\nuser query<|im_end|>"));
        assert!(chat.ends_with("<|im_start|>assistant\n"));
    }
}
