//! Model discovery and download helpers for local LLM inference.

use std::fs::File;
use std::io::Write;
use std::path::{Path, PathBuf};

use anyhow::{anyhow, Context, Result};

pub const DEFAULT_MODEL_NAME: &str = "LFM2.5-230M-F16.gguf";
pub const DEFAULT_TOKENIZER_NAME: &str = "LFM2.5-tokenizer.json";

pub const MODEL_LFM25: &str = "LFM2.5-230M-F16.gguf";
pub const MODEL_K2: &str = "K2-Horizon-1B-BF16.gguf";
pub const MODEL_EMBEDDINGS: &str = "bge-m3-q8_0.gguf";

pub const REMOTE_GGUF_URL: &str =
    "https://huggingface.co/LiquidAI/LFM2.5-230M-GGUF/resolve/main/LFM2.5-230M-F16.gguf";
pub const REMOTE_TOKENIZER_URL: &str =
    "https://huggingface.co/LiquidAI/LFM2.5-230M/resolve/main/tokenizer.json";

/// Returns ~/models as the standard models directory.
pub fn default_models_dir() -> PathBuf {
    if let Some(home) = std::env::var_os("HOME") {
        PathBuf::from(&home).join("models")
    } else {
        PathBuf::from("models")
    }
}

/// Resolve a model alias (e.g. "k2", "lfm") or path to an existing or expected model PathBuf.
pub fn resolve_model_path(name_or_path: &Path) -> PathBuf {
    if name_or_path.is_file() {
        return name_or_path.to_path_buf();
    }
    let lower = name_or_path.to_string_lossy().to_lowercase();
    let file_name = if lower == "k2" || lower == "k2-horizon" {
        MODEL_K2
    } else if lower == "lfm" || lower == "lfm2.5" {
        MODEL_LFM25
    } else if lower == "bge" || lower == "embeddings" {
        MODEL_EMBEDDINGS
    } else {
        name_or_path.file_name().and_then(|f| f.to_str()).unwrap_or("")
    };

    let in_models = default_models_dir().join(file_name);
    if in_models.is_file() {
        return in_models;
    }

    name_or_path.to_path_buf()
}

/// Resolve target model file, port, and display name.
pub fn resolve_model_and_port(
    custom: Option<&Path>,
    explicit_port: Option<u16>,
) -> (PathBuf, u16, &'static str) {
    if let Some(path) = custom {
        let resolved = resolve_model_path(path);
        let path_str = resolved.to_string_lossy().to_lowercase();
        let (default_port, name) = if path_str.contains("k2") || path_str.contains("horizon") {
            (crate::ai::llama::PORT_K2, "K2")
        } else if path_str.contains("bge") || path_str.contains("embed") {
            (crate::ai::llama::PORT_EMBEDDINGS, "embeddings")
        } else {
            (crate::ai::llama::PORT_LFM25, "LFM2.5")
        };
        let port = explicit_port.unwrap_or(default_port);
        (resolved, port, name)
    } else {
        let model_path = default_models_dir().join(DEFAULT_MODEL_NAME);
        let port = explicit_port.unwrap_or(crate::ai::llama::PORT_LFM25);
        (model_path, port, "LFM2.5")
    }
}

/// Find a GGUF model file: checks custom override, then standard locations.
pub fn find_gguf_model(custom: Option<&Path>) -> Option<PathBuf> {
    if let Some(path) = custom {
        let resolved = resolve_model_path(path);
        if resolved.is_file() {
            return Some(resolved);
        }
        return None;
    }

    let candidate = default_models_dir().join(DEFAULT_MODEL_NAME);
    if candidate.is_file() {
        return Some(candidate);
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

    #[test]
    fn resolve_model_and_port_maps_correctly() {
        // Defaults
        let (path, port, name) = resolve_model_and_port(None, None);
        assert_eq!(port, 43211);
        assert_eq!(name, "LFM2.5");
        assert!(path.to_string_lossy().contains(DEFAULT_MODEL_NAME));

        // K2 alias
        let (_k2_path, k2_port, k2_name) = resolve_model_and_port(Some(Path::new("k2")), None);
        assert_eq!(k2_port, 43212);
        assert_eq!(k2_name, "K2");

        // Embeddings alias
        let (_emb_path, emb_port, emb_name) = resolve_model_and_port(Some(Path::new("embeddings")), None);
        assert_eq!(emb_port, 43210);
        assert_eq!(emb_name, "embeddings");

        // Explicit port override
        let (_custom_path, custom_port, _name) = resolve_model_and_port(None, Some(9999));
        assert_eq!(custom_port, 9999);
    }
}
