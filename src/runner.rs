//! Obscura subprocess wrapper: one fetch, one HTML payload.

use std::path::Path;

use anyhow::{anyhow, Context, Result};

/// Run `obscura fetch <url> --stealth --dump html` and return the HTML.
///
/// `--wait` is intentionally never passed: it blocks the fetch forever.
pub(crate) async fn fetch_html(binary: &Path, url: &str, timeout_secs: u64) -> Result<String> {
    let command = tokio::process::Command::new(binary)
        .args(["fetch", url, "--stealth", "--dump", "html"])
        .stdin(std::process::Stdio::null())
        .output();

    let output = tokio::time::timeout(std::time::Duration::from_secs(timeout_secs), command)
        .await
        .map_err(|_| anyhow!("obscura fetch timed out after {timeout_secs}s: {url}"))?
        .context("failed to spawn obscura")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let detail = stderr.lines().last().unwrap_or("no stderr output");
        let status = output.status;
        return Err(anyhow!("obscura exited with {status}: {detail}"));
    }

    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A missing binary must surface an error instead of hanging.
    #[tokio::test]
    async fn missing_binary_fails_fast() {
        let result = fetch_html(Path::new("/nonexistent/obscura"), "https://example.com", 5).await;
        assert!(result.is_err());
    }
}
