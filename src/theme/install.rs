use std::fs::{self, OpenOptions};
use std::io::{IsTerminal, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};

use anyhow::{anyhow, bail, Context, Result};
use serde::Serialize;
use sha2::{Digest, Sha256};

use super::format::{validate_id, ThemeFile, MAX_FILE_BYTES};
use super::registry::{is_reserved_id, validate_standalone, ThemeRegistry};

pub const COMMUNITY_PREFIX: &str = "community/";
const COMMUNITY_RAW_ROOT: &str =
    "https://raw.githubusercontent.com/RizRiyz/luvus/main/community/themes";
static NEXT_TMP: AtomicU64 = AtomicU64::new(1);

#[derive(Clone, Debug)]
pub struct InstalledTheme {
    pub id: String,
    pub display_name: String,
    pub path: PathBuf,
    pub source: String,
    pub warnings: Vec<String>,
}

#[derive(Debug, Serialize)]
struct Provenance<'a> {
    schema: u32,
    source: &'a str,
    sha256: String,
    installed_unix: u64,
}

pub fn install(source: &str, yes: bool) -> Result<InstalledTheme> {
    let (bytes, canonical_source, remote) = acquire(source)?;
    let file = ThemeFile::parse(&bytes)?;
    if is_reserved_id(&file.id) {
        bail!("theme ID `{}` is reserved by Luvus", file.id);
    }
    let registry = ThemeRegistry::load();
    let (_, warnings) = validate_standalone(&file, &registry)?;

    if remote && !yes {
        confirm(&file, &canonical_source)?;
    }

    let dir = super::ensure_themes_dir()?;
    let destination = dir.join(format!("{}.toml", file.id));
    reject_duplicate_destination(&registry, &file.id, &destination)?;
    let provenance = provenance_path(&dir, &file.id);
    let previous_theme = fs::read(&destination).ok();
    let previous_provenance = fs::read(&provenance).ok();

    let transaction = (|| -> Result<()> {
        atomic_write(&destination, &bytes)?;
        write_provenance(&dir, &file.id, &canonical_source, &bytes)?;

        // Validate the exact on-disk registry before reporting success. This
        // catches conflicts with manually copied files and resolution changes.
        let loaded = ThemeRegistry::load_from(&dir);
        if loaded.get(&file.id).is_none() {
            let message = loaded
                .problems()
                .iter()
                .find(|problem| problem.path == destination.display().to_string())
                .map(|problem| problem.message.clone())
                .unwrap_or_else(|| "theme did not load from the installed registry".to_string());
            bail!("installed theme failed registry validation: {message}");
        }
        Ok(())
    })();
    if let Err(error) = transaction {
        restore_file(&destination, previous_theme.as_deref());
        restore_file(&provenance, previous_provenance.as_deref());
        return Err(error);
    }

    Ok(InstalledTheme {
        id: file.id,
        display_name: file.display_name,
        path: destination,
        source: canonical_source,
        warnings,
    })
}

pub fn uninstall(id: &str) -> Result<PathBuf> {
    validate_id(id)?;
    if is_reserved_id(id) {
        bail!("`{id}` is bundled with Luvus and cannot be uninstalled");
    }
    let config = crate::config::load();
    if crate::ui::theme::canonical(&config.theme) == id {
        bail!("cannot uninstall active theme `{id}`; run `luvus theme use <other-id>` first");
    }
    let dir = super::themes_dir();
    let registry = ThemeRegistry::load_from(&dir);
    let entry = registry
        .get(id)
        .with_context(|| format!("theme `{id}` is not installed"))?;
    let path = match &entry.source {
        super::registry::ThemeSource::Local { path, .. } => PathBuf::from(path),
        _ => bail!("`{id}` is not a local theme"),
    };

    let dependents = local_dependents(&dir, id);
    if !dependents.is_empty() {
        bail!(
            "cannot uninstall `{id}`; required by {}",
            dependents.join(", ")
        );
    }
    fs::remove_file(&path).with_context(|| format!("remove {}", path.display()))?;
    let _ = fs::remove_file(provenance_path(&dir, id));
    Ok(path)
}

pub fn init(path: &Path, id: &str, extends: Option<&str>) -> Result<()> {
    validate_id(id)?;
    if is_reserved_id(id) {
        bail!("theme ID `{id}` is reserved by Luvus");
    }
    if let Some(parent) = extends {
        validate_id(parent).context("invalid parent theme ID")?;
        if ThemeRegistry::load().get(parent).is_none() {
            bail!("parent theme `{parent}` is not installed");
        }
    }
    let body = starter_toml(id, extends);
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .with_context(|| format!("create {}", path.display()))?;
    file.write_all(body.as_bytes())?;
    file.sync_all()?;
    Ok(())
}

pub fn validate_path(path: &Path, strict: bool) -> Result<(ThemeFile, Vec<String>)> {
    let metadata = fs::metadata(path).with_context(|| format!("inspect {}", path.display()))?;
    if metadata.len() > MAX_FILE_BYTES as u64 {
        bail!("theme file exceeds the {MAX_FILE_BYTES}-byte limit");
    }
    let bytes = fs::read(path).with_context(|| format!("read {}", path.display()))?;
    let file = ThemeFile::parse(&bytes)?;
    let (_, warnings) = validate_standalone(&file, &ThemeRegistry::load())?;
    if strict && !warnings.is_empty() {
        bail!("strict validation failed: {}", warnings.join("; "));
    }
    Ok((file, warnings))
}

pub fn community_url(id: &str) -> Result<String> {
    validate_id(id)?;
    Ok(format!("{COMMUNITY_RAW_ROOT}/{id}.toml"))
}

fn acquire(source: &str) -> Result<(Vec<u8>, String, bool)> {
    if let Some(id) = source.strip_prefix(COMMUNITY_PREFIX) {
        if id.is_empty() || id.contains('/') {
            bail!("community source must use `community/<theme-id>`");
        }
        let url = community_url(id)?;
        return Ok((fetch_https(&url)?, url, true));
    }
    if source.starts_with("https://") {
        return Ok((fetch_https(source)?, source.to_string(), true));
    }
    if source.contains("://") {
        bail!("remote theme sources must use HTTPS");
    }
    let path = PathBuf::from(source);
    let metadata = fs::metadata(&path).with_context(|| format!("inspect {}", path.display()))?;
    if !metadata.is_file() {
        bail!("theme source is not a regular file: {}", path.display());
    }
    if metadata.len() > MAX_FILE_BYTES as u64 {
        bail!("theme file exceeds the {MAX_FILE_BYTES}-byte limit");
    }
    let bytes = fs::read(&path).with_context(|| format!("read {}", path.display()))?;
    let source = fs::canonicalize(&path)
        .unwrap_or(path)
        .display()
        .to_string();
    Ok((bytes, source, false))
}

fn fetch_https(url: &str) -> Result<Vec<u8>> {
    if !url.starts_with("https://")
        || url
            .chars()
            .any(|character| character.is_control() || character.is_whitespace())
    {
        bail!("invalid HTTPS theme URL");
    }
    let max = MAX_FILE_BYTES.to_string();
    let curl = [
        "--fail",
        "--silent",
        "--show-error",
        "--location",
        "--max-time",
        "20",
        "--proto",
        "=https",
        "--proto-redir",
        "=https",
        "--max-filesize",
        max.as_str(),
        "--header",
        "Accept: application/toml,text/plain",
        "--header",
        "User-Agent: luvus",
        url,
    ];
    if let Some(bytes) = try_fetch("curl", &curl)? {
        return Ok(bytes);
    }
    let quota = format!("--quota={MAX_FILE_BYTES}");
    let wget = [
        "-q",
        "-O",
        "-",
        "--timeout=20",
        "--tries=1",
        "--https-only",
        quota.as_str(),
        "--header=Accept: application/toml,text/plain",
        "--header=User-Agent: luvus",
        url,
    ];
    if let Some(bytes) = try_fetch("wget", &wget)? {
        return Ok(bytes);
    }
    bail!("need curl or wget to download themes")
}

fn try_fetch(program: &str, args: &[&str]) -> Result<Option<Vec<u8>>> {
    let mut child = match Command::new(program)
        .args(args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
    {
        Ok(child) => child,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error).with_context(|| format!("run {program}")),
    };
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| anyhow!("capture {program} output"))?;
    let mut stderr = child
        .stderr
        .take()
        .ok_or_else(|| anyhow!("capture {program} errors"))?;
    let errors = std::thread::spawn(move || {
        let mut bytes = Vec::new();
        let _ = stderr.read_to_end(&mut bytes);
        bytes
    });
    let mut bytes = Vec::new();
    stdout
        .take((MAX_FILE_BYTES + 1) as u64)
        .read_to_end(&mut bytes)?;
    if bytes.len() > MAX_FILE_BYTES {
        let _ = child.kill();
        let _ = child.wait();
        let _ = errors.join();
        bail!("theme download exceeds the {MAX_FILE_BYTES}-byte limit");
    }
    let status = child.wait()?;
    let errors = errors.join().unwrap_or_default();
    if !status.success() {
        bail!("{program}: {}", String::from_utf8_lossy(&errors).trim());
    }
    Ok(Some(bytes))
}

fn confirm(file: &ThemeFile, source: &str) -> Result<()> {
    if !std::io::stdin().is_terminal() {
        bail!("remote theme installation requires --yes when stdin is not interactive");
    }
    eprintln!("Theme:  {} ({})", file.display_name, file.id);
    eprintln!("Source: {source}");
    eprint!("Install this data-only theme? [y/N] ");
    std::io::stderr().flush()?;
    let mut answer = String::new();
    std::io::stdin().read_line(&mut answer)?;
    if !matches!(answer.trim().to_ascii_lowercase().as_str(), "y" | "yes") {
        bail!("installation cancelled");
    }
    Ok(())
}

fn reject_duplicate_destination(
    registry: &ThemeRegistry,
    id: &str,
    destination: &Path,
) -> Result<()> {
    let Some(entry) = registry.get(id) else {
        return Ok(());
    };
    match &entry.source {
        super::registry::ThemeSource::Local { path, .. } if Path::new(path) == destination => {
            Ok(())
        }
        super::registry::ThemeSource::Local { path, .. } => bail!(
            "theme ID `{id}` is already installed from {path}; remove that file before replacing it"
        ),
        _ => bail!("theme ID `{id}` is reserved by Luvus"),
    }
}

fn restore_file(path: &Path, previous: Option<&[u8]>) {
    match previous {
        Some(bytes) => {
            let _ = atomic_write(path, bytes);
        }
        None => {
            let _ = fs::remove_file(path);
        }
    }
}

fn atomic_write(path: &Path, bytes: &[u8]) -> Result<()> {
    let parent = path
        .parent()
        .ok_or_else(|| anyhow!("theme path has no parent"))?;
    let temp = parent.join(format!(
        ".{}.tmp-{}-{}",
        path.file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("theme"),
        std::process::id(),
        NEXT_TMP.fetch_add(1, Ordering::Relaxed)
    ));
    let result = (|| -> Result<()> {
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temp)
            .with_context(|| format!("create {}", temp.display()))?;
        file.write_all(bytes)?;
        file.sync_all()?;
        atomic_replace(&temp, path)
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temp);
    }
    result
}

fn atomic_replace(temp: &Path, destination: &Path) -> Result<()> {
    #[cfg(not(windows))]
    {
        fs::rename(temp, destination)
            .with_context(|| format!("replace {}", destination.display()))?;
    }
    #[cfg(windows)]
    {
        let backup = destination.with_extension("toml.bak");
        if destination.exists() {
            let _ = fs::remove_file(&backup);
            fs::rename(destination, &backup)?;
        }
        if let Err(error) = fs::rename(temp, destination) {
            let _ = fs::rename(&backup, destination);
            return Err(error).with_context(|| format!("replace {}", destination.display()));
        }
        let _ = fs::remove_file(backup);
    }
    Ok(())
}

fn write_provenance(dir: &Path, id: &str, source: &str, bytes: &[u8]) -> Result<()> {
    let digest = format!("{:x}", Sha256::digest(bytes));
    let installed_unix = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let encoded = serde_json::to_vec_pretty(&Provenance {
        schema: 1,
        source,
        sha256: digest,
        installed_unix,
    })?;
    atomic_write(&provenance_path(dir, id), &encoded)
}

fn provenance_path(dir: &Path, id: &str) -> PathBuf {
    dir.join(format!("{id}.source.json"))
}

fn local_dependents(dir: &Path, parent: &str) -> Vec<String> {
    let mut dependents = Vec::new();
    let Ok(entries) = fs::read_dir(dir) else {
        return dependents;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|value| value.to_str()) != Some("toml") {
            continue;
        }
        let Ok(bytes) = fs::read(path) else {
            continue;
        };
        if let Ok(file) = ThemeFile::parse(&bytes) {
            if file.extends.as_deref() == Some(parent) {
                dependents.push(file.id);
            }
        }
    }
    dependents.sort();
    dependents
}

fn starter_toml(id: &str, extends: Option<&str>) -> String {
    let title = id
        .split(['-', '_', '.'])
        .filter(|part| !part.is_empty())
        .map(|part| {
            let mut chars = part.chars();
            chars
                .next()
                .map(|first| first.to_uppercase().chain(chars).collect::<String>())
                .unwrap_or_default()
        })
        .collect::<Vec<_>>()
        .join(" ");
    if let Some(parent) = extends {
        return format!(
            r##"schema = 1
id = "{id}"
display_name = "{title}"
description = ""
author = ""
version = "1.0.0"
requires_luvus = ">=0.11.0"
appearance = "dark"
extends = "{parent}"

[colors]
# Override only the semantic roles that differ from the parent.
accent = "#c6ff1a"
sel_bg = "#33450e"
"##
        );
    }
    format!(
        r##"schema = 1
id = "{id}"
display_name = "{title}"
description = ""
author = ""
version = "1.0.0"
requires_luvus = ">=0.11.0"
appearance = "dark"

[colors]
crust = "#070709"
mantle = "#111116"
base = "#202028"
surface0 = "#1a1a20"
surface1 = "#25252d"
overlay0 = "#4a4a54"
overlay1 = "#686873"
subtext0 = "#93939f"
subtext1 = "#b6b6c0"
text = "#e7e7ed"
accent = "#c6ff1a"
sel_bg = "#33450e"
border = "#383840"
border_focus = "#8c8c96"
green = "#8fbc7a"
mint = "#6fc6a3"
amber = "#e09a4d"
coral = "#e06c66"
"##
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn community_coordinate_is_bounded_and_unambiguous() {
        assert_eq!(
            community_url("warm-copper").unwrap(),
            "https://raw.githubusercontent.com/RizRiyz/luvus/main/community/themes/warm-copper.toml"
        );
        for bad in ["../escape", "two words", "a/b"] {
            assert!(community_url(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn parent_with_installed_dependents_cannot_be_removed() {
        let _env = crate::persist::test_env("theme-dependent-uninstall");
        let root = crate::persist::ensure_config_dir();
        let parent = root.join("parent.toml");
        init(&parent, "local-parent", None).unwrap();
        install(parent.to_str().unwrap(), true).unwrap();
        let child = root.join("child.toml");
        init(&child, "local-child", Some("local-parent")).unwrap();
        install(child.to_str().unwrap(), true).unwrap();
        let error = uninstall("local-parent").unwrap_err().to_string();
        assert!(error.contains("local-child"), "{error}");
    }

    #[test]
    fn starter_is_valid_schema_one() {
        let complete = starter_toml("my-theme", None);
        ThemeFile::parse(complete.as_bytes())
            .unwrap()
            .colors
            .resolve(None)
            .unwrap();
        let child = starter_toml("my-child", Some("noir"));
        let file = ThemeFile::parse(child.as_bytes()).unwrap();
        assert_eq!(file.extends.as_deref(), Some("noir"));
    }
}
