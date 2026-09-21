use std::env;
use std::fs;
use std::io::Write;
use std::path::PathBuf;

use anyhow::{anyhow, Context, Result};

const REPO_OWNER: &str = "eddraz";
const REPO_NAME: &str = "faro";
const USER_AGENT: &str = "faro-update";

/// Map a (target os, target arch) pair to the faro release asset name.
/// Pure so it can be unit-tested for every supported combination.
pub fn release_asset(os: &str, arch: &str) -> Option<&'static str> {
    match (os, arch) {
        ("linux", "x86_64") => Some("faro-x86_64-linux.tar.gz"),
        ("linux", "aarch64") => Some("faro-aarch64-linux.tar.gz"),
        ("macos", "x86_64") => Some("faro-x86_64-macos.tar.gz"),
        ("macos", "aarch64") => Some("faro-aarch64-macos.tar.gz"),
        _ => None,
    }
}

/// Strip an optional leading `v` and parse `MAJOR.MINOR.PATCH`.
pub fn parse_tag(tag: &str) -> Option<(u32, u32, u32)> {
    let stripped = tag.strip_prefix('v').unwrap_or(tag);
    let mut parts = stripped.split('.');
    let major = parts.next()?.parse().ok()?;
    let minor = parts.next()?.parse().ok()?;
    let patch = parts.next()?.parse().ok()?;
    // Reject trailing components or extra separators.
    if parts.next().is_some() {
        return None;
    }
    Some((major, minor, patch))
}

/// True only when `remote` is strictly greater than `current`.
pub fn is_newer(remote: (u32, u32, u32), current: (u32, u32, u32)) -> bool {
    remote > current
}

fn api_latest_url() -> String {
    format!(
        "https://api.github.com/repos/{REPO_OWNER}/{REPO_NAME}/releases/latest"
    )
}

fn release_download_url(asset: &str) -> String {
    format!(
        "https://github.com/{REPO_OWNER}/{REPO_NAME}/releases/latest/download/{asset}"
    )
}

/// Fetch the latest release tag name from the GitHub API.
pub fn fetch_latest_tag() -> Result<String> {
    let url = api_latest_url();
    eprintln!("checking for updates at {url} ...");
    let response = ureq::get(&url)
        .set("User-Agent", USER_AGENT)
        .set("Accept", "application/vnd.github+json")
        .call()
        .map_err(|e| anyhow!("failed to fetch latest release: {e}"))?;

    let value: serde_json::Value = serde_json::from_reader(response.into_reader())
        .context("failed to parse GitHub release JSON")?;

    value
        .get("tag_name")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
        .ok_or_else(|| anyhow!("GitHub release response missing tag_name"))
}

/// Download the latest release asset for this platform and atomically replace
/// the current executable.
pub fn download_and_replace() -> Result<()> {
    let os = env::consts::OS;
    let arch = env::consts::ARCH;
    let asset = release_asset(os, arch).ok_or_else(|| {
        anyhow!("unsupported platform {os}/{arch}: no prebuilt faro release asset")
    })?;

    let url = release_download_url(asset);
    eprintln!("downloading {url} ...");
    let response = ureq::get(&url)
        .set("User-Agent", USER_AGENT)
        .call()
        .map_err(|e| anyhow!("failed to download release asset: {e}"))?;

    let mut bytes = Vec::new();
    std::io::copy(&mut response.into_reader(), &mut bytes)
        .context("failed to read release asset body")?;

    eprintln!("extracting faro binary ...");
    let gz = flate2::read::GzDecoder::new(&bytes[..]);
    let mut archive = tar::Archive::new(gz);
    let mut binary_bytes: Option<Vec<u8>> = None;

    for entry in archive
        .entries()
        .context("failed to read tarball entries")?
    {
        let mut entry = entry.context("corrupt tarball entry")?;
        let name = entry.path()?.to_path_buf();
        let file_name = match name.file_name().and_then(|n| n.to_str()) {
            Some("faro") => "faro",
            _ => continue,
        };
        let mut buf = Vec::new();
        std::io::copy(&mut entry, &mut buf)
            .with_context(|| format!("cannot extract {file_name}"))?;
        binary_bytes = Some(buf);
        break;
    }

    let binary_bytes = binary_bytes.ok_or_else(|| anyhow!("tarball did not contain the faro binary"))?;

    let current = env::current_exe().context("cannot determine current executable path")?;
    let dir = current
        .parent()
        .ok_or_else(|| anyhow!("current executable has no parent directory"))?;
    let pid = std::process::id();
    let tmp_path: PathBuf = dir.join(format!("faro.update-{pid}.tmp"));

    {
        let mut tmp = fs::File::create(&tmp_path)
            .with_context(|| format!("cannot create temporary file {}", tmp_path.display()))?;
        tmp.write_all(&binary_bytes)
            .with_context(|| format!("cannot write to temporary file {}", tmp_path.display()))?;
        tmp.flush().ok();
    }

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&tmp_path, fs::Permissions::from_mode(0o755))
            .with_context(|| format!("cannot chmod {}", tmp_path.display()))?;
    }

    eprintln!("replacing {} ...", current.display());
    fs::rename(&tmp_path, &current)
        .with_context(|| format!("cannot replace {} with {}", current.display(), tmp_path.display()))?;

    Ok(())
}

/// Entry point for `faro update`.
pub fn run() -> Result<()> {
    let current_version = env!("CARGO_PKG_VERSION");
    let current = parse_tag(current_version)
        .ok_or_else(|| anyhow!("cannot parse current version {current_version}"))?;

    let tag = fetch_latest_tag()?;
    let remote = parse_tag(&tag)
        .ok_or_else(|| anyhow!("cannot parse remote version tag {tag}"))?;

    if !is_newer(remote, current) {
        println!("faro {current_version} is up to date (latest: {tag})");
        return Ok(());
    }

    eprintln!("Updating faro {current_version} -> {tag} ...");
    download_and_replace()?;
    println!("faro updated to {tag}");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn release_assets_cover_supported_platforms() {
        assert_eq!(
            release_asset("linux", "x86_64"),
            Some("faro-x86_64-linux.tar.gz")
        );
        assert_eq!(
            release_asset("linux", "aarch64"),
            Some("faro-aarch64-linux.tar.gz")
        );
        assert_eq!(
            release_asset("macos", "x86_64"),
            Some("faro-x86_64-macos.tar.gz")
        );
        assert_eq!(
            release_asset("macos", "aarch64"),
            Some("faro-aarch64-macos.tar.gz")
        );
        assert_eq!(release_asset("windows", "x86_64"), None);
        assert_eq!(release_asset("linux", "powerpc64"), None);
    }

    #[test]
    fn parses_version_tags() {
        assert_eq!(parse_tag("v0.2.0"), Some((0, 2, 0)));
        assert_eq!(parse_tag("0.2.0"), Some((0, 2, 0)));
        assert_eq!(parse_tag("v1.10.3"), Some((1, 10, 3)));
    }

    #[test]
    fn rejects_invalid_version_tags() {
        assert_eq!(parse_tag("v1.0"), None);
        assert_eq!(parse_tag("garbage"), None);
        assert_eq!(parse_tag(""), None);
        assert_eq!(parse_tag("v1.2.3.4"), None);
        assert_eq!(parse_tag("v1.a.0"), None);
    }

    #[test]
    fn compares_versions_correctly() {
        assert!(is_newer((0, 2, 0), (0, 1, 0)));
        assert!(is_newer((1, 0, 0), (0, 9, 9)));
        assert!(is_newer((0, 2, 1), (0, 2, 0)));
        assert!(!is_newer((0, 2, 0), (0, 2, 0)));
        assert!(!is_newer((0, 1, 0), (0, 2, 0)));
        assert!(!is_newer((0, 2, 0), (0, 2, 1)));
    }
}
