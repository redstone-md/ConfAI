//! The official MCP registry at <https://registry.modelcontextprotocol.io>.
//!
//! Nine hand-written presets were never going to keep up with a registry that
//! already lists more than a thousand servers, so this searches the real thing.
//! A registry entry is richer than our [`mcp::Server`] — it knows which
//! environment variables a server needs and which of those are secrets — and
//! that extra knowledge is reported rather than discarded, because "you must set
//! GITHUB_TOKEN first" is the difference between a server that runs and one that
//! fails silently on first use.

use std::time::Duration;

use anyhow::{Context, Result};
use serde::Deserialize;

use crate::mcp;

const BASE: &str = "https://registry.modelcontextprotocol.io/v0/servers";
const TIMEOUT: Duration = Duration::from_secs(20);

/// One server as the registry describes it.
#[derive(Debug, Clone)]
pub struct Entry {
    pub name: String,
    pub title: Option<String>,
    pub description: String,
    pub version: Option<String>,
    /// Ways to run it, best first.
    pub options: Vec<Launch>,
}

/// One concrete way to start a server.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Launch {
    /// A package run through a launcher such as `npx` or `uvx`.
    Package { runtime: String, args: Vec<String>, env: Vec<EnvVar> },
    /// An HTTP endpoint.
    Remote { url: String },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EnvVar {
    pub name: String,
    pub description: Option<String>,
    pub required: bool,
    pub secret: bool,
}

impl Entry {
    /// The launch this program would use, preferring a package over a remote:
    /// a package runs locally with no account, a remote usually needs one.
    pub fn preferred(&self) -> Option<&Launch> {
        self.options
            .iter()
            .find(|o| matches!(o, Launch::Package { .. }))
            .or_else(|| self.options.first())
    }

    /// A short name to record the server under, taken from the last path-like
    /// segment of the reverse-DNS registry name: `io.github.foo/bar` becomes `bar`.
    pub fn short_name(&self) -> String {
        let tail = self.name.rsplit('/').next().unwrap_or(&self.name);
        let cleaned: String = tail
            .chars()
            .map(|c| if c.is_ascii_alphanumeric() || c == '-' || c == '_' { c } else { '-' })
            .collect();
        cleaned.trim_matches('-').to_lowercase()
    }

    /// Turn this entry into something an agent can be told to launch.
    pub fn to_server(&self, name: Option<&str>) -> Result<mcp::Server> {
        let launch =
            self.preferred().with_context(|| format!("{} lists no way to run it", self.name))?;

        let transport = match launch {
            Launch::Package { runtime, args, .. } => {
                mcp::Transport::Stdio { command: runtime.clone(), args: args.clone() }
            }
            Launch::Remote { url } => mcp::Transport::Remote { url: url.clone() },
        };

        Ok(mcp::Server {
            name: name.map(str::to_owned).unwrap_or_else(|| self.short_name()),
            transport,
            // Values are not invented here: the registry names the variables, it
            // does not know your secrets. They are reported so you can set them.
            env: Default::default(),
            enabled: None,
        })
    }

    /// Variables the preferred launch needs that are not set in this environment.
    pub fn missing_env(&self) -> Vec<&EnvVar> {
        let Some(Launch::Package { env, .. }) = self.preferred() else {
            return Vec::new();
        };
        env.iter().filter(|var| var.required && std::env::var_os(&var.name).is_none()).collect()
    }
}

/// Search the registry. An empty query lists whatever it returns first.
pub fn search(query: &str, limit: usize) -> Result<Vec<Entry>> {
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .timeout_global(Some(TIMEOUT))
        .user_agent(concat!("confai/", env!("CARGO_PKG_VERSION")))
        .build()
        .into();

    let url = if query.is_empty() {
        format!("{BASE}?limit={limit}")
    } else {
        format!("{BASE}?limit={limit}&search={}", encode(query))
    };

    let page: Page = agent
        .get(&url)
        .call()
        .context("searching the MCP registry")?
        .into_body()
        .read_json()
        .context("reading the registry response")?;

    Ok(dedupe(page.servers))
}

/// Collapse the registry's per-version records into one entry per server.
///
/// The registry returns every published version as its own record: a search for
/// "github" came back with 100 records covering 46 servers, one of them listed
/// seven times. Left alone that fills a result list with the same thing over and
/// over. The record flagged `isLatest` wins; failing that, the highest version
/// by the registry's own ordering, which is the last one it sent.
fn dedupe(records: Vec<Record>) -> Vec<Entry> {
    let mut order: Vec<String> = Vec::new();
    let mut best: std::collections::HashMap<String, (bool, Entry)> =
        std::collections::HashMap::new();

    for record in records {
        let is_latest = record.is_latest();
        let entry: Entry = record.server.into();
        let name = entry.name.clone();

        match best.get(&name) {
            // Never displace a record the registry itself calls current.
            Some((true, _)) => continue,
            None => order.push(name.clone()),
            Some(_) => {}
        }
        best.insert(name, (is_latest, entry));
    }

    order.into_iter().filter_map(|name| best.remove(&name).map(|(_, entry)| entry)).collect()
}

/// Fetch one entry by its exact registry name.
pub fn get(name: &str) -> Result<Entry> {
    let wanted = name.trim();
    search(wanted, 50)?
        .into_iter()
        .find(|entry| entry.name == wanted)
        .with_context(|| format!("the registry has no server named {wanted:?}"))
}

/// Percent-encode a query, which may contain spaces and punctuation.
fn encode(query: &str) -> String {
    query
        .bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                (b as char).to_string()
            }
            b' ' => "+".to_string(),
            other => format!("%{other:02X}"),
        })
        .collect()
}

#[derive(Deserialize)]
struct Page {
    #[serde(default)]
    servers: Vec<Record>,
}

#[derive(Deserialize)]
struct Record {
    server: RawServer,
    #[serde(default, rename = "_meta")]
    meta: Option<Meta>,
}

impl Record {
    /// Whether the registry considers this the current version of the server.
    fn is_latest(&self) -> bool {
        self.meta.as_ref().and_then(|m| m.official.as_ref()).is_some_and(|o| o.is_latest)
    }
}

#[derive(Deserialize)]
struct Meta {
    #[serde(default, rename = "io.modelcontextprotocol.registry/official")]
    official: Option<Official>,
}

#[derive(Deserialize)]
struct Official {
    #[serde(default, rename = "isLatest")]
    is_latest: bool,
}

#[derive(Deserialize)]
struct RawServer {
    name: String,
    #[serde(default)]
    title: Option<String>,
    #[serde(default)]
    description: String,
    #[serde(default)]
    version: Option<String>,
    #[serde(default)]
    packages: Vec<RawPackage>,
    #[serde(default)]
    remotes: Vec<RawRemote>,
}

#[derive(Deserialize)]
struct RawPackage {
    identifier: String,
    #[serde(default)]
    version: Option<String>,
    #[serde(default, rename = "runtimeHint")]
    runtime_hint: Option<String>,
    #[serde(default, rename = "registryType")]
    registry_type: Option<String>,
    #[serde(default, rename = "runtimeArguments")]
    runtime_arguments: Vec<RawArgument>,
    #[serde(default, rename = "environmentVariables")]
    environment_variables: Vec<RawEnv>,
}

#[derive(Deserialize)]
struct RawArgument {
    #[serde(default)]
    value: Option<String>,
}

#[derive(Deserialize)]
struct RawEnv {
    name: String,
    #[serde(default)]
    description: Option<String>,
    #[serde(default, rename = "isRequired")]
    is_required: bool,
    #[serde(default, rename = "isSecret")]
    is_secret: bool,
}

#[derive(Deserialize)]
struct RawRemote {
    url: String,
}

impl From<RawServer> for Entry {
    fn from(raw: RawServer) -> Self {
        let mut options: Vec<Launch> =
            raw.packages.into_iter().filter_map(|package| package.into_launch()).collect();
        options.extend(raw.remotes.into_iter().map(|r| Launch::Remote { url: r.url }));

        Entry {
            name: raw.name,
            title: raw.title,
            description: raw.description,
            version: raw.version,
            options,
        }
    }
}

impl RawPackage {
    /// Build the command line a launcher needs, or give up on package types we
    /// cannot start from a config file.
    fn into_launch(self) -> Option<Launch> {
        let runtime = self.runtime_hint.or_else(|| {
            // The registry does not always state a hint, but the package type
            // implies the usual launcher.
            match self.registry_type.as_deref() {
                Some("npm") => Some("npx".to_string()),
                Some("pypi") => Some("uvx".to_string()),
                Some("oci") => Some("docker".to_string()),
                _ => None,
            }
        })?;

        // A container needs flags this program cannot guess — a port, a mount,
        // a network. Better to omit it than to write a command that will not run.
        if runtime == "docker" {
            return None;
        }

        let mut args: Vec<String> =
            self.runtime_arguments.into_iter().filter_map(|a| a.value).collect();
        args.push(match self.version {
            Some(version) if !version.is_empty() => format!("{}@{version}", self.identifier),
            _ => self.identifier,
        });

        let env = self
            .environment_variables
            .into_iter()
            .map(|raw| EnvVar {
                name: raw.name,
                description: raw.description,
                required: raw.is_required,
                secret: raw.is_secret,
            })
            .collect();

        Some(Launch::Package { runtime, args, env })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const PAGE: &str = r#"{
      "servers": [
        {
          "server": {
            "name": "io.github.example/filesystem",
            "title": "Filesystem",
            "description": "Read and write files",
            "version": "1.2.0",
            "packages": [{
              "registryType": "npm",
              "identifier": "server-filesystem",
              "version": "0.1.2",
              "runtimeHint": "npx",
              "runtimeArguments": [{ "value": "-y", "type": "positional" }],
              "environmentVariables": [
                { "name": "ROOT", "isRequired": true, "description": "Where to serve from" },
                { "name": "TOKEN", "isRequired": true, "isSecret": true },
                { "name": "OPTIONAL_ONE" }
              ]
            }]
          }
        },
        {
          "server": {
            "name": "ac.example/remote-only",
            "description": "Hosted",
            "remotes": [{ "type": "streamable-http", "url": "https://mcp.example.invalid/mcp" }]
          }
        },
        {
          "server": {
            "name": "com.example/container",
            "description": "Container only",
            "packages": [{
              "registryType": "oci",
              "identifier": "ghcr.io/example/thing:1.0",
              "runtimeHint": "docker"
            }]
          }
        }
      ]
    }"#;

    fn entries() -> Vec<Entry> {
        let page: Page = serde_json::from_str(PAGE).unwrap();
        page.servers.into_iter().map(|r| r.server.into()).collect()
    }

    #[test]
    fn a_package_becomes_a_launcher_command_line() {
        let entry = &entries()[0];
        let server = entry.to_server(None).unwrap();

        assert_eq!(server.name, "filesystem");
        assert_eq!(
            server.transport,
            mcp::Transport::Stdio {
                command: "npx".into(),
                args: vec!["-y".into(), "server-filesystem@0.1.2".into()],
            }
        );
    }

    #[test]
    fn a_remote_only_entry_becomes_a_remote_server() {
        let entry = &entries()[1];
        assert_eq!(
            entry.to_server(None).unwrap().transport,
            mcp::Transport::Remote { url: "https://mcp.example.invalid/mcp".into() }
        );
    }

    #[test]
    fn a_container_only_entry_offers_nothing_runnable() {
        // Docker needs mounts and ports this program cannot guess; writing a
        // command that will not run is worse than admitting there isn't one.
        let entry = &entries()[2];
        assert!(entry.options.is_empty());
        assert!(entry.to_server(None).is_err());
    }

    #[test]
    fn a_package_is_preferred_over_a_remote() {
        let mut entry = entries()[1].clone();
        entry.options.insert(
            0,
            Launch::Package { runtime: "npx".into(), args: vec!["thing".into()], env: Vec::new() },
        );
        // The remote was first in the list; the package still wins.
        assert!(matches!(entry.preferred(), Some(Launch::Package { .. })));
    }

    #[test]
    fn short_names_are_id_safe() {
        let entry = &entries()[0];
        assert_eq!(entry.short_name(), "filesystem");
        assert!(crate::agent::validate_provider_id(&entry.short_name()).is_ok());

        let awkward = Entry {
            name: "io.github.foo/Bar.Baz Qux".into(),
            title: None,
            description: String::new(),
            version: None,
            options: Vec::new(),
        };
        // Dots collapse too: `bar-baz-qux` reads as a name, `bar.baz-qux` reads
        // as a leftover of the reverse-DNS id it came from.
        assert_eq!(awkward.short_name(), "bar-baz-qux");
        assert!(crate::agent::validate_provider_id(&awkward.short_name()).is_ok());
    }

    #[test]
    fn only_required_and_unset_variables_are_reported_missing() {
        let entry = &entries()[0];
        let missing: Vec<&str> = entry.missing_env().iter().map(|v| v.name.as_str()).collect();

        // OPTIONAL_ONE is not required, so it is not chased.
        assert_eq!(missing, vec!["ROOT", "TOKEN"]);
        assert!(entry.missing_env().iter().any(|v| v.secret && v.name == "TOKEN"));
    }

    /// The shape the registry really sends: one record per published version,
    /// with exactly one of them flagged current.
    const VERSIONS: &str = r#"{
      "servers": [
        { "server": { "name": "a/thing", "description": "old" },
          "_meta": { "io.modelcontextprotocol.registry/official": { "isLatest": false } } },
        { "server": { "name": "b/other", "description": "only one" } },
        { "server": { "name": "a/thing", "description": "current" },
          "_meta": { "io.modelcontextprotocol.registry/official": { "isLatest": true } } },
        { "server": { "name": "a/thing", "description": "newer but not flagged" },
          "_meta": { "io.modelcontextprotocol.registry/official": { "isLatest": false } } }
      ]
    }"#;

    #[test]
    fn versions_of_one_server_collapse_to_the_current_one() {
        let page: Page = serde_json::from_str(VERSIONS).unwrap();
        let entries = dedupe(page.servers);

        assert_eq!(entries.len(), 2, "duplicate versions survived: {entries:?}");
        let thing = entries.iter().find(|e| e.name == "a/thing").unwrap();
        assert_eq!(thing.description, "current", "a later record displaced the flagged one");
    }

    #[test]
    fn deduplication_keeps_the_order_the_registry_sent() {
        let page: Page = serde_json::from_str(VERSIONS).unwrap();
        let names: Vec<String> = dedupe(page.servers).into_iter().map(|e| e.name).collect();
        assert_eq!(names, vec!["a/thing", "b/other"]);
    }

    #[test]
    fn without_any_latest_flag_the_last_record_wins() {
        let page: Page = serde_json::from_str(
            r#"{"servers":[
                {"server":{"name":"x/y","description":"first"}},
                {"server":{"name":"x/y","description":"second"}}
            ]}"#,
        )
        .unwrap();
        let entries = dedupe(page.servers);
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].description, "second");
    }

    #[test]
    fn queries_are_encoded_for_a_url() {
        assert_eq!(encode("github"), "github");
        assert_eq!(encode("file system"), "file+system");
        assert_eq!(encode("a/b&c"), "a%2Fb%26c");
    }
}
