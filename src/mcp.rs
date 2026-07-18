//! MCP servers, in terms every agent can be mapped onto.
//!
//! The three agents disagree about almost everything here: where the servers
//! live, whether a command is one string or a list, whether `env` is called
//! `env` or `environment`, and whether a server can be disabled without being
//! deleted. As with providers, the backends translate and everything above this
//! line is written once.

use std::collections::BTreeMap;
use std::time::Duration;

use serde::{Deserialize, Serialize};

/// How an agent reaches a server.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum Transport {
    /// A child process speaking MCP over stdin and stdout.
    Stdio {
        command: String,
        #[serde(default)]
        args: Vec<String>,
    },
    /// An HTTP endpoint.
    Remote { url: String },
}

impl Transport {
    /// The executable a stdio server runs, for checks that need it.
    pub fn program(&self) -> Option<&str> {
        match self {
            Transport::Stdio { command, .. } => Some(command),
            Transport::Remote { .. } => None,
        }
    }

    pub fn url(&self) -> Option<&str> {
        match self {
            Transport::Remote { url } => Some(url),
            Transport::Stdio { .. } => None,
        }
    }

    /// The command as one line, for listings.
    pub fn summary(&self) -> String {
        match self {
            Transport::Stdio { command, args } if args.is_empty() => command.clone(),
            Transport::Stdio { command, args } => format!("{command} {}", args.join(" ")),
            Transport::Remote { url } => url.clone(),
        }
    }

    pub fn kind(&self) -> &'static str {
        match self {
            Transport::Stdio { .. } => "stdio",
            Transport::Remote { .. } => "remote",
        }
    }
}

/// One MCP server as configured in an agent.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Server {
    pub name: String,
    #[serde(flatten)]
    pub transport: Transport,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub env: BTreeMap<String, String>,
    /// `None` where the agent has no way to disable a server short of removing it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub enabled: Option<bool>,
}

impl Server {
    pub fn stdio(name: impl Into<String>, command: impl Into<String>, args: &[&str]) -> Self {
        Self {
            name: name.into(),
            transport: Transport::Stdio {
                command: command.into(),
                args: args.iter().map(|a| a.to_string()).collect(),
            },
            env: BTreeMap::new(),
            enabled: None,
        }
    }

    pub fn remote(name: impl Into<String>, url: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            transport: Transport::Remote { url: url.into() },
            env: BTreeMap::new(),
            enabled: None,
        }
    }

    /// Disabled only when the agent says so; absence of the flag means running.
    pub fn is_enabled(&self) -> bool {
        self.enabled.unwrap_or(true)
    }
}

/// What checking a server found.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Health {
    /// The command resolves on `PATH`, or the endpoint answered.
    Ok(String),
    /// Configured but not runnable as written.
    Broken(String),
    /// Nothing was checked, and why.
    Skipped(String),
}

impl Health {
    pub fn is_ok(&self) -> bool {
        matches!(self, Health::Ok(_))
    }

    pub fn message(&self) -> &str {
        match self {
            Health::Ok(m) | Health::Broken(m) | Health::Skipped(m) => m,
        }
    }
}

/// Check whether a server could actually start.
///
/// For a stdio server this resolves the executable on `PATH` rather than running
/// it: launching an arbitrary configured command to see what happens is not a
/// diagnostic, it is executing whatever is in the config. `npx`-style launchers
/// are reported as the launcher they are, since the package behind them cannot
/// be verified without fetching it.
pub fn check(server: &Server, timeout: Duration) -> Health {
    if !server.is_enabled() {
        return Health::Skipped("disabled".into());
    }

    match &server.transport {
        Transport::Stdio { command, args } => match which::which(command) {
            Ok(path) => {
                if is_launcher(command) {
                    let package = args.iter().find(|a| !a.starts_with('-'));
                    match package {
                        Some(package) => {
                            Health::Ok(format!("{command} will fetch {package} on first run"))
                        }
                        None => Health::Ok(path.display().to_string()),
                    }
                } else {
                    Health::Ok(path.display().to_string())
                }
            }
            Err(_) => Health::Broken(format!("{command} is not on PATH")),
        },
        Transport::Remote { url } => {
            let agent: ureq::Agent = ureq::Agent::config_builder()
                .timeout_global(Some(timeout))
                .user_agent(concat!("confai/", env!("CARGO_PKG_VERSION")))
                .build()
                .into();

            match agent.get(url).call() {
                Ok(response) => Health::Ok(format!("HTTP {}", response.status().as_u16())),
                // An MCP endpoint commonly refuses a bare GET; that it answered
                // at all is what is being tested here.
                Err(ureq::Error::StatusCode(code)) => Health::Ok(format!("HTTP {code}")),
                Err(err) => Health::Broken(first_clause(&err.to_string())),
            }
        }
    }
}

/// Commands that fetch and run something else, so resolving them proves less
/// than it appears to.
fn is_launcher(command: &str) -> bool {
    let stem = command.rsplit(['/', '\\']).next().unwrap_or(command);
    let stem = stem.strip_suffix(".cmd").or_else(|| stem.strip_suffix(".exe")).unwrap_or(stem);
    matches!(stem, "npx" | "bunx" | "pnpm" | "uvx" | "pipx" | "deno")
}

fn first_clause(message: &str) -> String {
    message.split([':', ';']).next().unwrap_or(message).trim().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_command_line_reads_as_one_string() {
        let s = Server::stdio("continuum", "npx", &["-y", "continuum-mcp"]);
        assert_eq!(s.transport.summary(), "npx -y continuum-mcp");
        assert_eq!(s.transport.kind(), "stdio");
        assert_eq!(s.transport.program(), Some("npx"));

        let bare = Server::stdio("x", "continuum-adapter", &[]);
        assert_eq!(bare.transport.summary(), "continuum-adapter");
    }

    #[test]
    fn a_remote_server_summarises_as_its_url() {
        let s = Server::remote("sentry", "https://mcp.example.invalid/sse");
        assert_eq!(s.transport.summary(), "https://mcp.example.invalid/sse");
        assert_eq!(s.transport.kind(), "remote");
        assert!(s.transport.program().is_none());
        assert_eq!(s.transport.url(), Some("https://mcp.example.invalid/sse"));
    }

    #[test]
    fn a_missing_enabled_flag_means_running() {
        let mut s = Server::stdio("a", "b", &[]);
        assert!(s.is_enabled());
        s.enabled = Some(false);
        assert!(!s.is_enabled());
        s.enabled = Some(true);
        assert!(s.is_enabled());
    }

    #[test]
    fn a_disabled_server_is_not_checked() {
        let mut s = Server::stdio("a", "definitely-not-on-path-xyz", &[]);
        s.enabled = Some(false);
        assert_eq!(check(&s, Duration::from_secs(1)), Health::Skipped("disabled".into()));
    }

    #[test]
    fn a_command_that_is_not_installed_is_reported_broken() {
        let s = Server::stdio("a", "definitely-not-on-path-xyz", &[]);
        let health = check(&s, Duration::from_secs(1));
        assert!(!health.is_ok());
        assert!(health.message().contains("not on PATH"), "{}", health.message());
    }

    #[test]
    fn launchers_are_recognised_with_and_without_an_extension() {
        for command in ["npx", "npx.cmd", "/usr/bin/npx", "bunx", "uvx", "pipx", "deno"] {
            assert!(is_launcher(command), "{command} should be a launcher");
        }
        for command in ["node", "continuum-adapter", "python"] {
            assert!(!is_launcher(command), "{command} should not be a launcher");
        }
    }

    #[test]
    fn health_carries_its_message() {
        assert!(Health::Ok("fine".into()).is_ok());
        assert!(!Health::Broken("bad".into()).is_ok());
        assert_eq!(Health::Skipped("disabled".into()).message(), "disabled");
    }
}
