//! Background "update available" check. Fetches a small version manifest from
//! the product website (`bohay.dev/latest.json`, emitted at deploy time) and, if
//! it names a newer release than this build, tells the UI to show the indicator
//! by the sidebar version number.
//!
//! Notify-only by design: bohay is installed via cargo / brew / the install
//! script / Nix, so it never replaces its own binary — it points the user at the
//! changelog and their installer's upgrade command. The check is a single
//! `curl`/`wget` GET on its own thread, so it never touches the event loop, and
//! it only runs when `config.check_updates` is on.

use std::process::Command;
use std::sync::mpsc::Sender;
use std::thread;
use std::time::Duration;

use crate::event::AppEvent;

/// The version manifest the product site publishes at deploy time.
const MANIFEST_URL: &str = "https://bohay.dev/latest.json";
/// This build's version (no leading `v`).
const CURRENT: &str = env!("CARGO_PKG_VERSION");

/// The manifest URL to check, honoring `$BOHAY_UPDATE_MANIFEST` — an override for
/// testing (point it at a local `file://…/latest.json` or a dev server to see the
/// indicator without deploying the site). Falls back to the production URL.
fn manifest_url() -> String {
    std::env::var("BOHAY_UPDATE_MANIFEST").unwrap_or_else(|_| MANIFEST_URL.to_string())
}

/// Spawn the background checker: one check shortly after startup, then once a day
/// for a long-lived session. Sends [`AppEvent::UpdateAvailable`] only when the
/// manifest names a strictly newer release than this build.
pub fn spawn_check(tx: Sender<AppEvent>) {
    thread::spawn(move || {
        // A short initial delay so a launch is never slowed by a network call.
        thread::sleep(Duration::from_secs(5));
        loop {
            if let Some(latest) = fetch_latest() {
                if is_newer(&latest, CURRENT) {
                    let _ = tx.send(AppEvent::UpdateAvailable(latest));
                }
            }
            thread::sleep(Duration::from_secs(24 * 60 * 60));
        }
    });
}

fn fetch_latest() -> Option<String> {
    parse_version(&http_get(&manifest_url())?)
}

/// Pull the `"version"` string out of the manifest JSON (leading `v` trimmed).
fn parse_version(body: &str) -> Option<String> {
    let v: serde_json::Value = serde_json::from_str(body).ok()?;
    let s = v.get("version")?.as_str()?.trim();
    Some(s.trim_start_matches('v').to_string())
}

/// True when `latest` is a strictly higher semver than `current`. Both accept an
/// optional leading `v`; any pre-release/build suffix on a component is ignored.
pub fn is_newer(latest: &str, current: &str) -> bool {
    semver(latest) > semver(current)
}

fn semver(s: &str) -> (u32, u32, u32) {
    let s = s.trim().trim_start_matches('v');
    let mut it = s.split('.').map(|part| {
        part.split(|c: char| !c.is_ascii_digit())
            .next()
            .unwrap_or("")
            .parse::<u32>()
            .unwrap_or(0)
    });
    (
        it.next().unwrap_or(0),
        it.next().unwrap_or(0),
        it.next().unwrap_or(0),
    )
}

/// Fetch a URL with `curl`, then `wget` — whichever is installed. `None` on any
/// failure (offline, tool missing, non-200): a missed check is a silent no-op.
fn http_get(url: &str) -> Option<String> {
    let curl = ["-fsSL", "--max-time", "15", "-H", "User-Agent: bohay", url];
    if let Some(out) = try_cmd("curl", &curl) {
        return Some(out);
    }
    let wget = [
        "-q",
        "-O",
        "-",
        "--timeout=15",
        "--header=User-Agent: bohay",
        url,
    ];
    try_cmd("wget", &wget)
}

fn try_cmd(prog: &str, args: &[&str]) -> Option<String> {
    let out = Command::new(prog).args(args).output().ok()?;
    out.status
        .success()
        .then(|| String::from_utf8_lossy(&out.stdout).into_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn newer_compares_semver_with_optional_v() {
        assert!(is_newer("0.9.3", "0.9.2"));
        assert!(is_newer("v0.10.0", "0.9.9"));
        assert!(is_newer("1.0.0", "0.9.9"));
        assert!(!is_newer("0.9.2", "0.9.2"), "same version is not newer");
        assert!(!is_newer("0.9.1", "0.9.2"), "older is not newer");
        // A pre-release suffix on a component doesn't break the compare.
        assert!(is_newer("0.9.3-rc1", "0.9.2"));
    }

    #[test]
    fn parses_the_manifest_version() {
        assert_eq!(
            parse_version(r#"{"version":"0.9.3","notes":"x"}"#).as_deref(),
            Some("0.9.3")
        );
        // A leading `v` is trimmed.
        assert_eq!(
            parse_version(r#"{"version":"v1.2.0"}"#).as_deref(),
            Some("1.2.0")
        );
        // Garbage / missing field → None (no false "update available").
        assert_eq!(parse_version("not json"), None);
        assert_eq!(parse_version(r#"{"other":1}"#), None);
    }
}
