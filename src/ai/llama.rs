//! llama-server integration: process lifecycle and HTTP client.

use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::Duration;

use anyhow::{anyhow, Context, Result};
use serde_json::{json, Value};

/// Locate `llama-server` or `llama-serve` on PATH or in ~/.local/bin.
pub fn find_llama_server() -> Option<PathBuf> {
    if let Some(path) = which("llama-server") {
        return Some(path);
    }
    if let Some(path) = which("llama-serve") {
        return Some(path);
    }

    if let Some(home) = std::env::var_os("HOME") {
        let p = PathBuf::from(&home).join(".local").join("bin").join("llama-server");
        if p.is_file() {
            return Some(p);
        }
        let p2 = PathBuf::from(&home).join(".local").join("bin").join("llama-serve");
        if p2.is_file() {
            return Some(p2);
        }
    }

    None
}

fn which(program: &str) -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    for dir in std::env::split_paths(&path) {
        let candidate = dir.join(program);
        if candidate.is_file() {
            return Some(candidate);
        }
    }
    None
}

/// Probe the /health endpoint of llama-server.
pub fn probe_health(base_url: &str) -> bool {
    ureq::get(&format!("{base_url}/health"))
        .timeout(Duration::from_millis(500))
        .call()
        .map(|res| res.status() == 200)
        .unwrap_or(false)
}

pub const PORT_EMBEDDINGS: u16 = 43210;
pub const PORT_LFM25: u16 = 43211;
pub const PORT_K2: u16 = 43212;

#[derive(Debug, Clone)]
pub struct ServerPortStatus {
    pub port: u16,
    pub name: &'static str,
    pub model_name: &'static str,
    pub is_active: bool,
}

/// Check status of known ports (43210 for embeddings, 43211 for LFM2.5, 43212 for K2).
pub fn check_known_ports() -> Vec<ServerPortStatus> {
    vec![
        ServerPortStatus {
            port: PORT_EMBEDDINGS,
            name: "embeddings",
            model_name: "bge-m3-q8_0.gguf",
            is_active: probe_health(&format!("http://127.0.0.1:{PORT_EMBEDDINGS}")),
        },
        ServerPortStatus {
            port: PORT_LFM25,
            name: "LFM2.5",
            model_name: "LFM2.5-230M-F16.gguf",
            is_active: probe_health(&format!("http://127.0.0.1:{PORT_LFM25}")),
        },
        ServerPortStatus {
            port: PORT_K2,
            name: "K2",
            model_name: "K2-Horizon-1B-BF16.gguf",
            is_active: probe_health(&format!("http://127.0.0.1:{PORT_K2}")),
        },
    ]
}

#[cfg(unix)]
use std::os::unix::process::CommandExt;

#[cfg(unix)]
extern "C" {
    fn setsid() -> i32;
}

/// Ensure llama-server is healthy on `port`, spawning detached if necessary.
pub fn ensure_server(binary: &Path, model_path: &Path, port: u16) -> Result<()> {
    let base_url = format!("http://127.0.0.1:{port}");
    if probe_health(&base_url) {
        eprintln!("llama-server already active on port {port}; reusing instance (will not spawn again).");
        return Ok(());
    }

    eprintln!(
        "starting llama-server with {} on port {port}...",
        model_path.display()
    );
    let mut cmd = std::process::Command::new(binary);
    cmd.args([
        "-m",
        &model_path.to_string_lossy(),
        "--port",
        &port.to_string(),
        "-c",
        "4096",
        "--log-disable",
    ])
    .stdin(Stdio::null())
    .stdout(Stdio::null())
    .stderr(Stdio::null());

    #[cfg(unix)]
    unsafe {
        cmd.pre_exec(|| {
            setsid();
            Ok(())
        });
    }

    let _child = cmd
        .spawn()
        .with_context(|| format!("failed to spawn {}", binary.display()))?;

    // Wait up to 15s for the server to become healthy
    let start = std::time::Instant::now();
    let budget = Duration::from_secs(15);
    while start.elapsed() < budget {
        if probe_health(&base_url) {
            eprintln!("llama-server is ready on port {port}.");
            return Ok(());
        }
        std::thread::sleep(Duration::from_millis(300));
    }

    Err(anyhow!(
        "llama-server on port {port} did not become healthy within {}s",
        budget.as_secs()
    ))
}

/// Request a chat completion from llama-server on `port`.
pub fn generate(
    port: u16,
    system: &str,
    user: &str,
    max_tokens: usize,
) -> Result<String> {
    let url = format!("http://127.0.0.1:{port}/v1/chat/completions");

    let payload = json!({
        "messages": [
            {"role": "system", "content": system},
            {"role": "user", "content": user}
        ],
        "temperature": 0.2,
        "max_tokens": max_tokens,
    });

    let json_body = serde_json::to_string(&payload)?;
    let response = ureq::post(&url)
        .set("Content-Type", "application/json")
        .timeout(Duration::from_secs(60))
        .send_string(&json_body)
        .map_err(|e| anyhow!("llama-server chat request failed on port {port}: {e}"))?;

    let value: Value = serde_json::from_reader(response.into_reader())
        .context("failed to parse llama-server JSON response")?;

    let content = value
        .get("choices")
        .and_then(|c| c.as_array())
        .and_then(|arr| arr.first())
        .and_then(|first| first.get("message"))
        .and_then(|m| m.get("content"))
        .and_then(|c| c.as_str())
        .ok_or_else(|| anyhow!("missing or invalid choices[0].message.content in llama response: {value}"))?;

    Ok(content.trim().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_ports_are_configured_as_expected() {
        assert_eq!(PORT_EMBEDDINGS, 43210);
        assert_eq!(PORT_LFM25, 43211);
        assert_eq!(PORT_K2, 43212);

        let ports = check_known_ports();
        assert_eq!(ports.len(), 3);
        assert_eq!(ports[0].port, 43210);
        assert_eq!(ports[0].name, "embeddings");
        assert_eq!(ports[1].port, 43211);
        assert_eq!(ports[1].name, "LFM2.5");
        assert_eq!(ports[2].port, 43212);
        assert_eq!(ports[2].name, "K2");
    }
}

