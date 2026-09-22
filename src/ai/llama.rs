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

pub struct ServerGuard {
    child: Option<std::process::Child>,
}

impl Drop for ServerGuard {
    fn drop(&mut self) {
        if let Some(ref mut child) = self.child {
            let _ = child.kill();
        }
    }
}

/// Ensure llama-server is healthy on `port`, spawning if necessary.
pub fn ensure_server(model_path: &Path, port: u16) -> Result<ServerGuard> {
    let base_url = format!("http://127.0.0.1:{port}");
    if probe_health(&base_url) {
        return Ok(ServerGuard { child: None });
    }

    let binary = find_llama_server().ok_or_else(|| {
        anyhow!("neither llama-server nor llama-serve found on PATH or in ~/.local/bin")
    })?;

    eprintln!(
        "starting llama-server with {} on port {port}...",
        model_path.display()
    );
    let child = std::process::Command::new(&binary)
        .args([
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
        .stderr(Stdio::null())
        .spawn()
        .with_context(|| format!("failed to spawn {}", binary.display()))?;

    // Wait up to 15s for the server to become healthy
    let start = std::time::Instant::now();
    let budget = Duration::from_secs(15);
    while start.elapsed() < budget {
        if probe_health(&base_url) {
            eprintln!("llama-server is ready.");
            return Ok(ServerGuard { child: Some(child) });
        }
        std::thread::sleep(Duration::from_millis(300));
    }

    Err(anyhow!(
        "llama-server did not become healthy within {}s",
        budget.as_secs()
    ))
}

/// Request a chat completion from llama-server.
pub fn generate(
    model_path: &Path,
    port: u16,
    system: &str,
    user: &str,
    max_tokens: usize,
) -> Result<String> {
    let _guard = ensure_server(model_path, port)?;
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
        .map_err(|e| anyhow!("llama-server chat request failed: {e}"))?;

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
