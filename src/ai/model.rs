//! Model discovery and download helpers for local LLM inference.

use std::fs::File;
use std::io::Write;
use std::path::{Path, PathBuf};

use anyhow::{anyhow, Context, Result};

pub const DEFAULT_MODEL_NAME: &str = "LFM2.5-230M-F16.gguf";
pub const DEFAULT_TOKENIZER_NAME: &str = "LFM2.5-tokenizer.json";

pub const REMOTE_GGUF_URL: &str =
    "https://huggingface.co/LiquidAI/LFM2.5-230M-GGUF/resolve/main/LFM2.5-230M-F16.gguf";
pub const REMOTE_TOKENIZER_URL: &str =
    "https://huggingface.co/LiquidAI/LFM2.5-230M/resolve/main/tokenizer.json";

/// Returns ~/models or fallback to .cache/faro/models
pub fn default_models_dir() -> PathBuf {
    if let Some(home) = std::env::var_os("HOME") {
        let dir = PathBuf::from(&home).join("models");
        if dir.is_dir() {
            return dir;
        }
        return PathBuf::from(&home).join(".cache").join("faro").join("models");
    }
    PathBuf::from(".models")
}

/// Find a GGUF model file: checks custom override, then standard locations.
pub fn find_gguf_model(custom: Option<&Path>) -> Option<PathBuf> {
    if let Some(path) = custom {
        if path.is_file() {
            return Some(path.to_path_buf());
        }
        return None;
    }

    if let Some(home) = std::env::var_os("HOME") {
        let candidate = PathBuf::from(home).join("models").join(DEFAULT_MODEL_NAME);
        if candidate.is_file() {
            return Some(candidate);
        }
    }

    let cache_candidate = default_models_dir().join(DEFAULT_MODEL_NAME);
    if cache_candidate.is_file() {
        return Some(cache_candidate);
    }

    None
}

/// Download a file to destination if it doesn't already exist.
pub fn download_file(url: &str, dest: &Path) -> Result<()> {
    if dest.is_file() {
        return Ok(());
    }

    if let Some(parent) = dest.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("cannot create directory {}", parent.display()))?;
    }

    eprintln!("downloading {} to {} ...", url, dest.display());
    let response = ureq::get(url)
        .timeout(std::time::Duration::from_secs(600))
        .call()
        .map_err(|e| anyhow!("failed to download from {url}: {e}"))?;

    let tmp_dest = dest.with_extension("download.tmp");
    let mut file = File::create(&tmp_dest)
        .with_context(|| format!("cannot create temp file {}", tmp_dest.display()))?;

    std::io::copy(&mut response.into_reader(), &mut file)
        .with_context(|| format!("failed to stream download to {}", tmp_dest.display()))?;
    file.flush().ok();

    std::fs::rename(&tmp_dest, dest).with_context(|| {
        format!(
            "failed to rename {} to {}",
            tmp_dest.display(),
            dest.display()
        )
    })?;

    eprintln!("download complete: {}", dest.display());
    Ok(())
}

/// Ensure model weights and tokenizer exist for candle; downloads them if missing.
pub fn ensure_candle_weights(dir: &Path) -> Result<(PathBuf, PathBuf)> {
    let model_path = dir.join(DEFAULT_MODEL_NAME);
    let tokenizer_path = dir.join(DEFAULT_TOKENIZER_NAME);

    if !model_path.is_file() {
        // Also check if user has it in ~/models
        let found = find_gguf_model(None);
        if let Some(existing) = found {
            eprintln!("found existing GGUF model at {}", existing.display());
            if !tokenizer_path.is_file() {
                download_file(REMOTE_TOKENIZER_URL, &tokenizer_path)?;
            }
            return Ok((existing, tokenizer_path));
        }

        eprintln!(
            "candle model weights missing; downloading {} ...",
            DEFAULT_MODEL_NAME
        );
        download_file(REMOTE_GGUF_URL, &model_path)?;
    }

    if !tokenizer_path.is_file() {
        eprintln!(
            "candle tokenizer missing; downloading {} ...",
            DEFAULT_TOKENIZER_NAME
        );
        download_file(REMOTE_TOKENIZER_URL, &tokenizer_path)?;
    }

    Ok((model_path, tokenizer_path))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_names_and_urls_are_valid() {
        assert_eq!(DEFAULT_MODEL_NAME, "LFM2.5-230M-F16.gguf");
        assert_eq!(DEFAULT_TOKENIZER_NAME, "LFM2.5-tokenizer.json");
        assert!(REMOTE_GGUF_URL.starts_with("https://"));
        assert!(REMOTE_TOKENIZER_URL.starts_with("https://"));
    }

    #[test]
    fn find_gguf_model_respects_custom_when_missing() {
        let non_existent = Path::new("/path/that/does/not/exist/model.gguf");
        assert_eq!(find_gguf_model(Some(non_existent)), None);
    }
}
