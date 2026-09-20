//! SearXNG container lifecycle (rootless podman) and JSON search client.

use std::io::Read;
use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::{anyhow, Context, Result};
use serde::Deserialize;

use crate::engine::SearchResult;

pub(crate) const CONTAINER_NAME: &str = "searxng";
pub(crate) const IMAGE: &str = "docker.io/searxng/searxng:latest";
pub(crate) const DEFAULT_PORT: u16 = 8888;
const HEALTH_TIMEOUT: Duration = Duration::from_millis(300);
const STARTUP_BUDGET: Duration = Duration::from_secs(30);

/// Locate a program on PATH.
fn which(program: &str) -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path)
        .map(|dir| dir.join(program))
        .find(|candidate| candidate.is_file())
}

pub(crate) fn podman_path() -> Option<PathBuf> {
    which("podman")
}

/// Install `package` with sudo via the detected package manager, printing
/// the command first so the mutation is always visible to the user.
/// Decide how to install `package` with the available package managers:
/// returns (program, needs_sudo, args). The pasta binary ships in the
/// `passt` package on apt, dnf and Homebrew alike; Homebrew refuses sudo.
fn install_plan(
    package: &str,
    probe: &dyn Fn(&str) -> bool,
) -> Option<(String, bool, Vec<String>)> {
    let package = if package == "pasta" { "passt" } else { package };
    if probe("apt-get") {
        Some((
            "apt-get".into(),
            true,
            vec!["install".into(), "-y".into(), package.into()],
        ))
    } else if probe("dnf") {
        Some((
            "dnf".into(),
            true,
            vec!["install".into(), "-y".into(), package.into()],
        ))
    } else if probe("brew") {
        Some(("brew".into(), false, vec!["install".into(), package.into()]))
    } else {
        None
    }
}

/// Install `package` via the detected package manager, printing the command
/// first so the mutation is always visible to the user.
fn sudo_install(package: &str) -> Result<()> {
    let Some((program, needs_sudo, args)) =
        install_plan(package, &|program| which(program).is_some())
    else {
        return Err(anyhow!(
            "{package} is missing and no supported package manager (apt-get/dnf/brew) was \
             found; install it manually"
        ));
    };
    let mut command =
        std::process::Command::new(if needs_sudo { "sudo" } else { program.as_str() });
    if needs_sudo {
        command.arg(&program);
        eprintln!(
            "running: sudo {program} {} (sudo may ask for your password)",
            args.join(" ")
        );
    } else {
        eprintln!("running: {program} {}", args.join(" "));
    }
    let status = command
        .args(&args)
        .status()
        .with_context(|| format!("failed to execute {program}"))?;
    if !status.success() {
        return Err(anyhow!(
            "{package} installation failed ({status}); install it manually and retry"
        ));
    }
    Ok(())
}

/// Install podman with sudo via the detected package manager.
fn install_podman() -> Result<()> {
    sudo_install("podman")?;
    if podman_path().is_none() {
        return Err(anyhow!(
            "podman was installed but is still not on PATH; open a new shell and retry"
        ));
    }
    Ok(())
}

/// Rootless podman 5.x configures container networking with pasta; without
/// it `podman run` fails with exit 127.
fn ensure_pasta() -> Result<()> {
    if which("pasta").is_some() {
        return Ok(());
    }
    sudo_install("pasta")?;
    if which("pasta").is_none() {
        return Err(anyhow!(
            "pasta was installed but is still not on PATH; open a new shell and retry"
        ));
    }
    Ok(())
}

/// Guarantee podman is available, installing it on first run if necessary.
pub(crate) fn ensure_podman() -> Result<PathBuf> {
    if let Some(path) = podman_path() {
        return Ok(path);
    }
    install_podman()?;
    podman_path().ok_or_else(|| anyhow!("podman still not on PATH after installation"))
}

/// ~/apps/searxng
fn app_dir() -> PathBuf {
    let home = std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."));
    home.join("apps").join("searxng")
}

/// Minimal settings.yml that unlocks the JSON API for local use.
/// `use_default_settings` keeps everything else at upstream defaults.
fn settings_yml(secret_key: &str) -> String {
    format!(
        "use_default_settings: true\n\
         server:\n\
         \x20 secret_key: \"{secret_key}\"\n\
         \x20 limiter: false\n\
         \x20 public_instance: false\n\
         search:\n\
         \x20 formats:\n\
         \x20   - html\n\
         \x20   - json\n"
    )
}

fn random_secret() -> String {
    let mut bytes = [0u8; 16];
    if let Ok(mut file) = std::fs::File::open("/dev/urandom") {
        if file.read_exact(&mut bytes).is_ok() {
            return bytes.iter().map(|b| format!("{b:02x}")).collect();
        }
    }
    // Fallback without /dev/urandom: weak but unique-enough per machine.
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    format!("{nanos:032x}")
}

/// Create ~/apps/searxng/config/settings.yml once; never clobber user edits.
fn ensure_config(dir: &Path) -> Result<PathBuf> {
    let config_dir = dir.join("config");
    std::fs::create_dir_all(&config_dir)
        .with_context(|| format!("cannot create {}", config_dir.display()))?;
    let settings = config_dir.join("settings.yml");
    if !settings.exists() {
        std::fs::write(&settings, settings_yml(&random_secret()))
            .with_context(|| format!("cannot write {}", settings.display()))?;
        eprintln!("wrote {}", settings.display());
    }
    Ok(config_dir)
}

/// Pure builder for `podman run` arguments (unit-testable).
fn run_args(config_dir: &Path, data_dir: &Path, port: u16, network: Option<&str>) -> Vec<String> {
    let mut args = vec![
        "run".into(),
        "-d".into(),
        "--name".into(),
        CONTAINER_NAME.into(),
    ];
    if let Some(network) = network {
        args.push("--network".into());
        args.push(network.into());
    }
    args.extend([
        "-p".into(),
        format!("127.0.0.1:{port}:8080"),
        "-v".into(),
        format!("{}:/etc/searxng", config_dir.display()),
        "-v".into(),
        format!("{}:/var/cache/searxng", data_dir.display()),
        IMAGE.into(),
    ]);
    args
}

/// Choose the rootless network backend at container creation time: pasta is
/// podman's default; slirp4netns is used when pasta is absent; pasta is
/// auto-installed (visible sudo) only when neither backend exists.
/// Pure decision for Linux networking backends: None = podman default
/// (pasta), Some = explicit backend, Err = neither helper is installed.
fn linux_network_choice(pasta: bool, slirp: bool) -> Result<Option<&'static str>, ()> {
    if pasta {
        Ok(None)
    } else if slirp {
        Ok(Some("slirp4netns"))
    } else {
        Err(())
    }
}

/// Choose the container networking backend at creation time. Linux-only
/// logic: pasta is podman's default, slirp4netns is the fallback and pasta
/// is auto-installed (visible sudo) when both are absent. On macOS the
/// podman machine owns networking, so no flag is needed.
fn resolve_network() -> Result<Option<&'static str>> {
    if !cfg!(target_os = "linux") {
        return Ok(None);
    }
    match linux_network_choice(which("pasta").is_some(), which("slirp4netns").is_some()) {
        Ok(Some(backend)) => {
            eprintln!("pasta not found; using slirp4netns for container networking");
            Ok(Some(backend))
        }
        Ok(None) => Ok(None),
        Err(()) => {
            ensure_pasta()?;
            Ok(None)
        }
    }
}

/// On macOS podman runs containers inside a managed Linux VM; make sure it
/// is initialized and running before touching the container.
#[cfg(target_os = "macos")]
fn ensure_podman_machine(podman: &Path) -> Result<()> {
    let reachable = std::process::Command::new(podman)
        .arg("info")
        .output()
        .map(|out| out.status.success())
        .unwrap_or(false);
    if reachable {
        return Ok(());
    }
    eprintln!("podman machine not running; starting it (first boot can take a minute)...");
    let listing = std::process::Command::new(podman)
        .args(["machine", "list", "--format", "{{.Name}}"])
        .output()
        .context("failed to run podman machine list")?;
    if String::from_utf8_lossy(&listing.stdout).trim().is_empty() {
        let status = std::process::Command::new(podman)
            .args(["machine", "init"])
            .status()
            .context("failed to run podman machine init")?;
        if !status.success() {
            return Err(anyhow!("podman machine init failed ({status})"));
        }
    }
    let status = std::process::Command::new(podman)
        .args(["machine", "start"])
        .status()
        .context("failed to run podman machine start")?;
    if !status.success() {
        return Err(anyhow!("podman machine start failed ({status})"));
    }
    Ok(())
}

/// Linux manages containers directly; there is no podman machine layer.
#[cfg(not(target_os = "macos"))]
fn ensure_podman_machine(_podman: &Path) -> Result<()> {
    Ok(())
}

fn container_exists(podman: &Path) -> bool {
    std::process::Command::new(podman)
        .args(["container", "exists", CONTAINER_NAME])
        .output()
        .map(|out| out.status.success())
        .unwrap_or(false)
}

fn image_exists(podman: &Path) -> bool {
    std::process::Command::new(podman)
        .args(["image", "exists", IMAGE])
        .output()
        .map(|out| out.status.success())
        .unwrap_or(false)
}

fn start_container(podman: &Path, port: u16) -> Result<()> {
    if container_exists(podman) {
        eprintln!("starting existing searxng container...");
        let status = std::process::Command::new(podman)
            .args(["start", CONTAINER_NAME])
            .status()
            .context("failed to run podman start")?;
        if !status.success() {
            return Err(anyhow!("podman start searxng failed ({status})"));
        }
        return Ok(());
    }

    if !image_exists(podman) {
        eprintln!("pulling {IMAGE} (first run only)...");
        let status = std::process::Command::new(podman)
            .args(["pull", IMAGE])
            .status()
            .context("failed to run podman pull")?;
        if !status.success() {
            return Err(anyhow!("podman pull {IMAGE} failed ({status})"));
        }
    }

    let dir = app_dir();
    let config_dir = ensure_config(&dir)?;
    let data_dir = dir.join("data");
    std::fs::create_dir_all(&data_dir)
        .with_context(|| format!("cannot create {}", data_dir.display()))?;

    let network = resolve_network()?;
    eprintln!("creating searxng container on 127.0.0.1:{port}...");
    let status = std::process::Command::new(podman)
        .args(run_args(&config_dir, &data_dir, port, network))
        .status()
        .context("failed to run podman run")?;
    if !status.success() {
        return Err(anyhow!("podman run searxng failed ({status})"));
    }
    Ok(())
}

fn healthy(port: u16) -> bool {
    ureq::get(&format!("http://127.0.0.1:{port}/healthz"))
        .timeout(HEALTH_TIMEOUT)
        .call()
        .map(|response| response.status() == 200)
        .unwrap_or(false)
}

/// Wait until /healthz answers OK, bounded by `budget`.
fn wait_healthy(port: u16, budget: Duration) -> Result<()> {
    let started = std::time::Instant::now();
    while started.elapsed() < budget {
        if healthy(port) {
            return Ok(());
        }
        std::thread::sleep(Duration::from_millis(500));
    }
    Err(anyhow!(
        "searxng did not become healthy within {}s",
        budget.as_secs()
    ))
}

/// Ensure the SearXNG container is reachable: healthy check first (cheap),
/// then podman presence, image, container creation/start and health wait.
pub(crate) fn ensure_ready(port: u16) -> Result<()> {
    if healthy(port) {
        return Ok(());
    }
    let podman = ensure_podman()?;
    ensure_podman_machine(&podman)?;
    start_container(&podman, port)?;
    wait_healthy(port, STARTUP_BUDGET)
}

/// Upstream engine failures arrive either as `["bing (HTTP error 429)"]`
/// strings or as `["brave", "timeout"]` [engine, reason] pairs depending on
/// the SearXNG version; normalize both to "engine (reason)" strings.
fn deserialize_unresponsive<'de, D>(deserializer: D) -> Result<Vec<String>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let raw: Vec<serde_json::Value> = serde::Deserialize::deserialize(deserializer)?;
    Ok(raw
        .into_iter()
        .map(|entry| match entry {
            serde_json::Value::String(text) => text,
            serde_json::Value::Array(parts) => {
                let texts: Vec<String> = parts
                    .into_iter()
                    .filter_map(|value| value.as_str().map(str::to_string))
                    .collect();
                if texts.len() >= 2 {
                    format!("{} ({})", texts[0], texts[1])
                } else {
                    texts.join(" ")
                }
            }
            other => other.to_string(),
        })
        .collect())
}

#[derive(Deserialize)]
struct SearxngResponse {
    #[serde(default)]
    results: Vec<SearxngResult>,
    /// Upstream engines that failed (rate limit, captcha, timeout). Their
    /// results are missing from `results`, so coverage silently degrades;
    /// the cascade uses this to route those engines to the obscura fallback.
    #[serde(default, deserialize_with = "deserialize_unresponsive")]
    unresponsive_engines: Vec<String>,
}

#[derive(Deserialize)]
struct SearxngResult {
    title: String,
    url: String,
    #[serde(default)]
    content: String,
    #[serde(default)]
    engines: Vec<String>,
}

/// One SearXNG API response: results plus the upstream engines that failed.
pub(crate) struct SearxngSearch {
    pub(crate) results: Vec<SearchResult>,
    /// Source engines that answered with rate limits/captchas this time.
    pub(crate) unresponsive_engines: Vec<String>,
}

pub(crate) fn parse_search(json: &str) -> Result<SearxngSearch> {
    let response: SearxngResponse = serde_json::from_str(json).map_err(|error| {
        let preview: String = json.chars().take(200).collect();
        anyhow!("invalid searxng JSON response ({error}); body starts with: {preview:?}")
    })?;
    let results = response
        .results
        .into_iter()
        .map(|result| SearchResult {
            // Attribute each result to its real source engine (searxng reports
            // which engines produced it); fall back to "searxng" itself.
            engine: result
                .engines
                .first()
                .cloned()
                .unwrap_or_else(|| "searxng".to_string()),
            title: result.title,
            url: result.url,
            snippet: result.content,
        })
        .collect();
    Ok(SearxngSearch {
        results,
        unresponsive_engines: response.unresponsive_engines,
    })
}

/// Query the local SearXNG instance for the JSON API results.
pub(crate) fn search(
    port: u16,
    query: &str,
    engines: Option<&[String]>,
    limit: usize,
) -> Result<SearxngSearch> {
    let mut url = format!(
        "http://127.0.0.1:{port}/search?q={}&format=json",
        crate::engine::encode_query(query)
    );
    if let Some(engines) = engines {
        url.push_str("&engines=");
        url.push_str(&engines.join(","));
    }
    let response = ureq::get(&url)
        .timeout(Duration::from_secs(30))
        .call()
        .map_err(|error| anyhow!("searxng query failed: {error}"))?;
    if response.status() != 200 {
        return Err(anyhow!("searxng returned HTTP {}", response.status()));
    }
    let body = response
        .into_string()
        .context("failed to read searxng response body")?;
    let mut search = parse_search(&body)?;
    search.results.truncate(limit);
    Ok(search)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn linux_network_choice_prefers_pasta_then_slirp() {
        assert!(matches!(linux_network_choice(true, false), Ok(None)));
        assert!(matches!(
            linux_network_choice(false, true),
            Ok(Some("slirp4netns"))
        ));
        assert!(matches!(linux_network_choice(true, true), Ok(None)));
        assert!(linux_network_choice(false, false).is_err());
    }

    #[test]
    fn install_plan_maps_packages_and_managers() {
        let (program, sudo, args) = install_plan("podman", &|p| p == "apt-get").expect("apt plan");
        assert_eq!(program, "apt-get");
        assert!(sudo);
        assert_eq!(args, vec!["install", "-y", "podman"]);

        // The pasta binary ships as the `passt` package, and Homebrew
        // refuses sudo.
        let (program, sudo, args) = install_plan("pasta", &|p| p == "brew").expect("brew plan");
        assert_eq!(program, "brew");
        assert!(!sudo);
        assert_eq!(args, vec!["install", "passt"]);

        assert!(install_plan("podman", &|_| false).is_none());
    }

    #[test]
    fn settings_unlock_json_api_and_disable_limiter() {
        let yaml = settings_yml("abc123");
        assert!(yaml.contains("secret_key: \"abc123\""));
        assert!(yaml.contains("limiter: false"));
        assert!(yaml.contains("public_instance: false"));
        assert!(yaml.contains("- json"));
        assert!(yaml.contains("use_default_settings: true"));
    }

    #[test]
    fn run_args_bind_localhost_and_mount_volumes() {
        let args = run_args(
            Path::new("/home/u/apps/searxng/config"),
            Path::new("/home/u/apps/searxng/data"),
            8888,
            None,
        );
        let joined = args.join(" ");
        assert!(joined.contains("--name searxng"));
        assert!(joined.contains("-p 127.0.0.1:8888:8080"));
        assert!(joined.contains("/home/u/apps/searxng/config:/etc/searxng"));
        assert!(joined.contains("/home/u/apps/searxng/data:/var/cache/searxng"));
        assert!(joined.ends_with(IMAGE));
        assert!(!joined.contains("--network"));
    }

    #[test]
    fn run_args_support_fallback_network_backend() {
        let args = run_args(
            Path::new("/cfg"),
            Path::new("/data"),
            8888,
            Some("slirp4netns"),
        );
        let joined = args.join(" ");
        assert!(joined.contains("--network slirp4netns"));
        assert!(joined.ends_with(IMAGE));
    }

    #[test]
    fn parses_fixture_and_reports_unresponsive_engines() {
        let json = include_str!("../tests/fixtures/searxng.json");
        let search = parse_search(json).expect("valid fixture");
        assert_eq!(search.results.len(), 3);
        assert_eq!(search.results[0].engine, "duckduckgo");
        assert_eq!(search.results[1].engine, "github");
        assert_eq!(
            search.results[2].url,
            "https://en.wikipedia.org/wiki/Rust_(programming_language)"
        );
        assert!(!search.results[0].snippet.is_empty());
        // Both unresponsive entry shapes normalize to "engine (reason)".
        assert_eq!(
            search.unresponsive_engines,
            vec![
                "brave (timeout)",
                "wikidata (timeout)",
                "google (HTTP error 429)"
            ]
        );
    }
}
