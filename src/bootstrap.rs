use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Stdio;

use anyhow::{anyhow, Context, Result};

/// Minimum obscura version we are compatible with.
const MIN_MAJOR: u32 = 0;
const MIN_MINOR: u32 = 2;

const RELEASE_BASE: &str = "https://github.com/h4ckf0r0day/obscura/releases/latest/download";

/// Binaries shipped inside the release tarball.
const TARBALL_BINARIES: [&str; 2] = ["obscura", "obscura-worker"];

/// Map a (target os, target arch) pair to the obscura release asset name.
/// Pure so it can be unit-tested for every supported combination.
pub fn release_asset(os: &str, arch: &str) -> Option<&'static str> {
    match (os, arch) {
        ("linux", "x86_64") => Some("obscura-x86_64-linux.tar.gz"),
        ("linux", "aarch64") => Some("obscura-aarch64-linux.tar.gz"),
        ("macos", "x86_64") => Some("obscura-x86_64-macos.tar.gz"),
        ("macos", "aarch64") => Some("obscura-aarch64-macos.tar.gz"),
        _ => None,
    }
}

pub fn release_url(asset: &str) -> String {
    format!("{RELEASE_BASE}/{asset}")
}

/// Directory where we install obscura when it is missing: ~/.local/bin.
fn install_dir() -> PathBuf {
    if let Some(home) = std::env::var_os("HOME") {
        let mut dir = PathBuf::from(home);
        dir.push(".local");
        dir.push("bin");
        return dir;
    }
    PathBuf::from(".")
}

/// Look for `obscura` in every directory listed in PATH.
pub fn find_on_path() -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    for dir in std::env::split_paths(&path) {
        let candidate = dir.join("obscura");
        if candidate.is_file() {
            return Some(candidate);
        }
    }
    None
}

/// Parse `obscura 0.2.2` (or `obscura 0.2.2 (build ...)`) into (major, minor).
pub fn parse_version(output: &str) -> Option<(u32, u32)> {
    let version = output.split_whitespace().nth(1)?;
    let mut parts = version.split('.');
    let major = parts.next()?.parse().ok()?;
    let minor = parts.next()?.parse().ok()?;
    Some((major, minor))
}

pub fn version_compatible((major, minor): (u32, u32)) -> bool {
    major > MIN_MAJOR || (major == MIN_MAJOR && minor >= MIN_MINOR)
}

/// Run `obscura --version` and check the reported version.
fn verify(binary: &Path) -> Result<(u32, u32)> {
    let out = std::process::Command::new(binary)
        .arg("--version")
        .stdin(Stdio::null())
        .output()
        .with_context(|| format!("failed to execute {}", binary.display()))?;
    let text = String::from_utf8_lossy(&out.stdout);
    let version = parse_version(&text)
        .ok_or_else(|| anyhow!("cannot parse obscura version from: {}", text.trim()))?;
    if !version_compatible(version) {
        return Err(anyhow!(
            "obscura {} is too old (need >= {MIN_MAJOR}.{MIN_MINOR}); upgrade it from {}",
            text.trim(),
            RELEASE_BASE
        ));
    }
    Ok(version)
}

fn download_tarball(url: &str) -> Result<Vec<u8>> {
    eprintln!("downloading {url} ...");
    let response = ureq::get(url).call().map_err(|e| anyhow!("{e}"))?;
    let mut bytes = Vec::new();
    std::io::copy(&mut response.into_reader(), &mut bytes)
        .context("failed to read tarball body")?;
    Ok(bytes)
}

fn extract_binaries(tarball: &[u8], dest: &Path) -> Result<Vec<PathBuf>> {
    let gz = flate2::read::GzDecoder::new(tarball);
    let mut archive = tar::Archive::new(gz);
    let mut installed = Vec::new();
    for entry in archive
        .entries()
        .context("failed to read tarball entries")?
    {
        let mut entry = entry.context("corrupt tarball entry")?;
        let name = entry.path()?.to_path_buf();
        let file_name = match name.file_name().and_then(|n| n.to_str()) {
            Some(f) => f.to_string(),
            None => continue,
        };
        if !TARBALL_BINARIES.contains(&file_name.as_str()) {
            continue;
        }
        let target = dest.join(&file_name);
        let mut out = std::fs::File::create(&target)
            .with_context(|| format!("cannot create {}", target.display()))?;
        std::io::copy(&mut entry, &mut out)
            .with_context(|| format!("cannot extract {file_name}"))?;
        out.flush().ok();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&target, std::fs::Permissions::from_mode(0o755))
                .with_context(|| format!("cannot chmod {}", target.display()))?;
        }
        installed.push(target);
    }
    if !installed
        .iter()
        .any(|p| p.file_name() == Some(std::ffi::OsStr::new("obscura")))
    {
        return Err(anyhow!("tarball did not contain the obscura binary"));
    }
    Ok(installed)
}

/// Ensure the obscura binary is available and compatible; install it on first run.
pub async fn ensure_obscura() -> Result<PathBuf> {
    if let Some(path) = find_on_path() {
        verify(&path)?;
        return Ok(path);
    }

    let dir = install_dir();
    let candidate = dir.join("obscura");
    if candidate.is_file() {
        verify(&candidate)?;
        return Ok(candidate);
    }

    eprintln!("obscura not found; installing to {}", dir.display());
    let os = std::env::consts::OS;
    let arch = std::env::consts::ARCH;
    let asset = release_asset(os, arch).ok_or_else(|| {
        anyhow!("unsupported platform {os}/{arch}: install obscura manually from https://github.com/h4ckf0r0day/obscura/releases")
    })?;
    let url = release_url(asset);

    let bytes = download_tarball(&url)?;
    std::fs::create_dir_all(&dir).with_context(|| format!("cannot create {}", dir.display()))?;
    let installed = tokio::task::spawn_blocking(move || extract_binaries(&bytes, &dir))
        .await
        .context("install task panicked")??;

    let binary = installed
        .iter()
        .find(|p| p.file_name() == Some(std::ffi::OsStr::new("obscura")))
        .expect("checked above");
    verify(binary)?;
    eprintln!("installed obscura at {}", binary.display());
    Ok(binary.clone())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn release_assets_cover_supported_platforms() {
        assert_eq!(
            release_asset("linux", "x86_64"),
            Some("obscura-x86_64-linux.tar.gz")
        );
        assert_eq!(
            release_asset("linux", "aarch64"),
            Some("obscura-aarch64-linux.tar.gz")
        );
        assert_eq!(
            release_asset("macos", "x86_64"),
            Some("obscura-x86_64-macos.tar.gz")
        );
        assert_eq!(
            release_asset("macos", "aarch64"),
            Some("obscura-aarch64-macos.tar.gz")
        );
        assert_eq!(release_asset("windows", "x86_64"), None);
        assert_eq!(release_asset("linux", "powerpc64"), None);
    }

    #[test]
    fn release_url_is_absolute() {
        assert_eq!(
            release_url("obscura-x86_64-linux.tar.gz"),
            "https://github.com/h4ckf0r0day/obscura/releases/latest/download/obscura-x86_64-linux.tar.gz"
        );
    }

    #[test]
    fn parses_and_gates_versions() {
        assert_eq!(parse_version("obscura 0.2.2"), Some((0, 2)));
        assert_eq!(parse_version("obscura 1.0.0 (build abc)"), Some((1, 0)));
        assert_eq!(parse_version("garbage"), None);
        assert!(version_compatible((0, 2)));
        assert!(version_compatible((1, 0)));
        assert!(!version_compatible((0, 1)));
    }
}
