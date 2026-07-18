//! Telling the user a newer release exists, without ever making them wait for it.
//!
//! The rule this module is built around: a version check must not slow down a
//! command the user actually asked for. So the notice is rendered from a cache
//! file, and the network is only touched when that cache has gone stale — and
//! even then only for a few hundred milliseconds before the run gives up and
//! tries again another day.
//!
//! Set `CONFAI_NO_UPDATE_CHECK` to switch the whole thing off.

use std::fs;
use std::path::PathBuf;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result};
use semver::Version;
use serde::{Deserialize, Serialize};

use crate::brand;

/// How long a cached answer is trusted.
const CACHE_TTL: Duration = Duration::from_secs(24 * 60 * 60);
/// How long to wait before retrying after a failed check, so a machine that is
/// offline does not pay the timeout on every single run.
const RETRY_AFTER: Duration = Duration::from_secs(60 * 60);
/// The longest a normal command will wait for the check to finish.
const BACKGROUND_BUDGET: Duration = Duration::from_millis(400);
/// The budget for `confai update`, where waiting is the point.
const FOREGROUND_TIMEOUT: Duration = Duration::from_secs(10);

const RELEASES_API: &str = "https://api.github.com/repos/redstone-md/ConfAI/releases/latest";

/// What the last successful check found.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct Cached {
    /// Unix seconds of the last attempt, successful or not.
    attempted_at: u64,
    /// Unix seconds of the last successful fetch.
    fetched_at: u64,
    latest: String,
    #[serde(default)]
    notes: String,
    #[serde(default)]
    url: String,
}

/// A release newer than this build.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Available {
    pub current: Version,
    pub latest: Version,
    pub notes: String,
    pub url: String,
}

impl Available {
    /// The first few changelog bullets, for a notice that must stay small.
    pub fn headline(&self, lines: usize) -> Vec<String> {
        summarise(&self.notes, lines)
    }
}

/// Whether a check found anything, for `confai update` which reports either way.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Status {
    UpToDate {
        current: Version,
    },
    Newer(Box<Available>),
    /// This build is ahead of the newest release, which is normal when running
    /// from a working tree.
    Unreleased {
        current: Version,
        latest: Version,
    },
}

/// The notice to print after a command, taken from cache alone.
///
/// Refreshes the cache first if it is stale, but never waits longer than
/// [`BACKGROUND_BUDGET`] to do so.
pub fn notice() -> Option<Available> {
    if disabled() {
        return None;
    }
    let cached = read_cache();
    if should_refresh(cached.as_ref()) {
        refresh_within(BACKGROUND_BUDGET);
    }
    compare(read_cache().as_ref())
}

/// Check now and report the outcome, waiting for the network.
pub fn check_now() -> Result<Status> {
    let release = fetch(FOREGROUND_TIMEOUT)?;
    write_cache(&release);

    let current = current_version()?;
    let latest = parse_tag(&release.latest)
        .with_context(|| format!("release {:?} is not a version", release.latest))?;

    Ok(if latest > current {
        Status::Newer(Box::new(Available {
            current,
            latest,
            notes: release.notes,
            url: release.url,
        }))
    } else if current > latest {
        Status::Unreleased { current, latest }
    } else {
        Status::UpToDate { current }
    })
}

fn disabled() -> bool {
    std::env::var_os("CONFAI_NO_UPDATE_CHECK").is_some()
}

fn current_version() -> Result<Version> {
    Version::parse(brand::VERSION).context("this build carries an unparseable version")
}

/// Turn a cache entry into a notice, if it names something newer.
fn compare(cached: Option<&Cached>) -> Option<Available> {
    let cached = cached?;
    let current = current_version().ok()?;
    let latest = parse_tag(&cached.latest)?;
    (latest > current).then(|| Available {
        current,
        latest,
        notes: cached.notes.clone(),
        url: cached.url.clone(),
    })
}

/// Releases are tagged `v0.0.1`; the leading `v` is not part of the version.
fn parse_tag(tag: &str) -> Option<Version> {
    Version::parse(tag.trim().trim_start_matches('v')).ok()
}

fn should_refresh(cached: Option<&Cached>) -> bool {
    let Some(cached) = cached else { return true };
    let now = unix_now();
    let since_attempt = now.saturating_sub(cached.attempted_at);
    let since_success = now.saturating_sub(cached.fetched_at);

    if since_attempt < RETRY_AFTER.as_secs() {
        return false;
    }
    since_success >= CACHE_TTL.as_secs()
}

/// Run the fetch on a worker thread and take the answer only if it arrives in
/// time. A slow network costs one bounded pause, not an unbounded one.
fn refresh_within(budget: Duration) {
    let (sender, receiver) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let outcome = fetch(budget);
        // The receiver may already have given up; that is the expected case on a
        // slow link and is not an error.
        let _ = sender.send(outcome);
    });

    match receiver.recv_timeout(budget) {
        Ok(Ok(release)) => write_cache(&release),
        // Record the attempt either way, so a failing check backs off instead of
        // being retried on every invocation.
        _ => note_attempt(),
    }
}

struct Release {
    latest: String,
    notes: String,
    url: String,
}

#[derive(Deserialize)]
struct ReleasePayload {
    tag_name: String,
    #[serde(default)]
    body: String,
    #[serde(default)]
    html_url: String,
}

fn fetch(timeout: Duration) -> Result<Release> {
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .timeout_global(Some(timeout))
        .user_agent(concat!("confai/", env!("CARGO_PKG_VERSION")))
        .build()
        .into();

    let response =
        match agent.get(RELEASES_API).header("Accept", "application/vnd.github+json").call() {
            Ok(response) => response,
            // GitHub answers 404 both for a repository with no releases at all and
            // for one it will not show us; the first is by far the likelier here.
            Err(ureq::Error::StatusCode(404)) => {
                anyhow::bail!("{} has published no releases yet", brand::REPOSITORY_SHORT)
            }
            Err(ureq::Error::StatusCode(403)) => {
                anyhow::bail!("GitHub is rate-limiting this address; try again later")
            }
            Err(err) => return Err(err).context("asking GitHub for the latest release"),
        };

    let payload: ReleasePayload =
        response.into_body().read_json().context("reading the release response")?;

    Ok(Release { latest: payload.tag_name, notes: payload.body, url: payload.html_url })
}

fn cache_path() -> Option<PathBuf> {
    Some(dirs::cache_dir().or_else(dirs::home_dir)?.join("confai").join("update.json"))
}

fn read_cache() -> Option<Cached> {
    let text = fs::read_to_string(cache_path()?).ok()?;
    serde_json::from_str(&text).ok()
}

fn write_cache(release: &Release) {
    let now = unix_now();
    store(&Cached {
        attempted_at: now,
        fetched_at: now,
        latest: release.latest.clone(),
        notes: release.notes.clone(),
        url: release.url.clone(),
    });
}

/// Remember that a check was tried, keeping whatever the last success found.
fn note_attempt() {
    let mut cached = read_cache().unwrap_or(Cached {
        attempted_at: 0,
        fetched_at: 0,
        latest: String::new(),
        notes: String::new(),
        url: String::new(),
    });
    cached.attempted_at = unix_now();
    store(&cached);
}

fn store(cached: &Cached) {
    let Some(path) = cache_path() else { return };
    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    if let Ok(text) = serde_json::to_string(cached) {
        let _ = fs::write(path, text);
    }
}

fn unix_now() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

/// Pull the first few meaningful bullets out of a release body.
///
/// Release notes are generated markdown with section headings and links; a
/// notice has room for the gist, not the document.
fn summarise(notes: &str, limit: usize) -> Vec<String> {
    notes
        .lines()
        .map(str::trim)
        .filter(|line| line.starts_with('-') || line.starts_with('*'))
        .map(|line| {
            let text = line.trim_start_matches(['-', '*']).trim();
            // Drop the trailing commit link git-cliff appends.
            let text = text.split(" ([").next().unwrap_or(text).trim();
            strip_links(text)
        })
        .filter(|line| !line.is_empty())
        .take(limit)
        .collect()
}

/// Reduce `[label](url)` to `label`, so a terminal notice is not full of URLs.
fn strip_links(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(open) = rest.find('[') {
        let Some(close) = rest[open..].find(']') else { break };
        let close = open + close;
        let is_link = rest[close + 1..].starts_with('(');
        out.push_str(&rest[..open]);
        out.push_str(&rest[open + 1..close]);

        rest = &rest[close + 1..];
        if is_link {
            if let Some(end) = rest.find(')') {
                rest = &rest[end + 1..];
            }
        }
    }
    out.push_str(rest);
    out.trim().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cached(latest: &str, attempted_ago: u64, fetched_ago: u64) -> Cached {
        let now = unix_now();
        Cached {
            attempted_at: now.saturating_sub(attempted_ago),
            fetched_at: now.saturating_sub(fetched_ago),
            latest: latest.to_string(),
            notes: String::new(),
            url: String::new(),
        }
    }

    #[test]
    fn tags_parse_with_or_without_their_v() {
        assert_eq!(parse_tag("v0.0.1"), Some(Version::new(0, 0, 1)));
        assert_eq!(parse_tag("0.2.0"), Some(Version::new(0, 2, 0)));
        assert_eq!(parse_tag(" v1.2.3 "), Some(Version::new(1, 2, 3)));
        assert!(parse_tag("nightly").is_none());
    }

    #[test]
    fn a_newer_release_becomes_a_notice() {
        let current = current_version().unwrap();
        let newer = format!("v{}.{}.{}", current.major, current.minor, current.patch + 1);

        let notice = compare(Some(&cached(&newer, 0, 0))).expect("no notice for a newer release");
        assert_eq!(notice.latest, parse_tag(&newer).unwrap());
        assert_eq!(notice.current, current);
    }

    #[test]
    fn the_same_or_an_older_release_is_silent() {
        let current = current_version().unwrap();
        assert!(compare(Some(&cached(&format!("v{current}"), 0, 0))).is_none());
        assert!(compare(Some(&cached("v0.0.0", 0, 0))).is_none());
        assert!(compare(None).is_none());
    }

    #[test]
    fn an_unparseable_tag_never_nags() {
        assert!(compare(Some(&cached("latest", 0, 0))).is_none());
    }

    #[test]
    fn a_fresh_cache_is_not_refreshed() {
        assert!(!should_refresh(Some(&cached("v9.9.9", 10, 10))));
    }

    #[test]
    fn a_stale_cache_is_refreshed() {
        let day = CACHE_TTL.as_secs() + 60;
        assert!(should_refresh(Some(&cached("v9.9.9", day, day))));
        assert!(should_refresh(None));
    }

    #[test]
    fn a_recent_failure_backs_off_instead_of_retrying() {
        // Stale data, but attempted a minute ago: leave the network alone.
        let stale = CACHE_TTL.as_secs() + 60;
        assert!(!should_refresh(Some(&cached("v9.9.9", 60, stale))));
    }

    #[test]
    fn summaries_take_the_bullets_and_drop_the_plumbing() {
        let notes = "\
## Features

- provider sync now prunes retired models ([abc1234](https://example.invalid/c/abc1234))
- the palette ranks matches by tightness

## Fixes

- the hint bar no longer slices a key in half
- something else entirely";

        let lines = summarise(notes, 3);
        assert_eq!(
            lines,
            vec![
                "provider sync now prunes retired models",
                "the palette ranks matches by tightness",
                "the hint bar no longer slices a key in half",
            ]
        );
    }

    #[test]
    fn links_collapse_to_their_label() {
        assert_eq!(strip_links("see [the docs](https://example.invalid)"), "see the docs");
        assert_eq!(strip_links("plain text"), "plain text");
        assert_eq!(strip_links("a [b](u) and [c](u)"), "a b and c");
        // An unterminated bracket must not eat the rest of the line.
        assert_eq!(strip_links("half [open"), "half [open");
    }

    #[test]
    fn summaries_are_empty_when_there_are_no_bullets() {
        assert!(summarise("## Features\n\nnothing itemised here", 3).is_empty());
        assert!(summarise("", 3).is_empty());
    }
}
