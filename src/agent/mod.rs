//! The agent abstraction: one trait per installed CLI, one registry over all of them.

pub mod claude;
pub mod codex;
pub mod json;
pub mod opencode;
pub mod sidecar;

use std::path::{Path, PathBuf};

use anyhow::{bail, Result};

use crate::domain::Provider;
use crate::store;

/// What an agent can express in its config, so the UI offers only real choices.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Capabilities {
    /// Several named endpoints can coexist in the config.
    pub named_providers: bool,
    /// The active endpoint is a config value, not just whichever one exists.
    pub selectable_provider: bool,
    /// Providers enumerate the models they serve.
    pub per_provider_models: bool,
    /// An API key lives in the config rather than only in the environment.
    pub inline_api_key: bool,
}

/// Static description of an agent: enough to detect and locate it without loading anything.
#[derive(Debug, Clone)]
pub struct AgentInfo {
    pub id: &'static str,
    pub name: &'static str,
    /// Executable names to look for on `PATH`.
    pub binaries: &'static [&'static str],
    pub config_path: PathBuf,
    pub capabilities: Capabilities,
}

/// Evidence that an agent is present.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Detection {
    pub binary_on_path: bool,
    pub config_exists: bool,
}

impl Detection {
    pub fn installed(self) -> bool {
        self.binary_on_path || self.config_exists
    }

    pub fn describe(self) -> &'static str {
        match (self.binary_on_path, self.config_exists) {
            (true, true) => "binary + config",
            (true, false) => "binary only",
            (false, true) => "config only",
            (false, false) => "not found",
        }
    }
}

/// An agent ConfAI knows how to edit.
pub trait Agent {
    fn info(&self) -> &AgentInfo;

    /// Parse the agent's config into an editable view.
    fn load(&self) -> Result<Box<dyn AgentConfig>>;

    fn detect(&self) -> Detection {
        let info = self.info();
        Detection {
            binary_on_path: info.binaries.iter().any(|bin| which::which(bin).is_ok()),
            config_exists: info.config_path.exists(),
        }
    }
}

/// A parsed config, edited in place so unknown keys, comments and key order survive.
pub trait AgentConfig {
    fn info(&self) -> &AgentInfo;

    fn providers(&self) -> Vec<Provider>;

    /// Insert `provider`, or overlay it onto the existing entry with the same id.
    fn upsert_provider(&mut self, provider: &Provider) -> Result<()>;

    fn remove_provider(&mut self, id: &str) -> Result<bool>;

    /// Drop models of `id` that are not in `keep`, returning how many went.
    ///
    /// Upserting is a merge, so a model the endpoint has retired would otherwise
    /// linger forever. Agents that do not store a model list have nothing to
    /// prune and say so by returning zero.
    fn prune_models(&mut self, id: &str, keep: &[String]) -> Result<usize> {
        let _ = (id, keep);
        Ok(0)
    }

    /// The provider the agent currently routes through, if it records one.
    fn active_provider(&self) -> Option<String>;

    fn set_active_provider(&mut self, id: &str) -> Result<()>;

    fn model(&self) -> Option<String>;

    fn set_model(&mut self, model: &str) -> Result<()>;

    /// Select `model`, attributing it to the endpoint it came from.
    ///
    /// Most agents name a model on its own. opencode names it `provider/model`,
    /// so picking a model from a provider that is not the active one would
    /// otherwise silently resolve against the wrong endpoint.
    fn set_model_for(&mut self, provider_id: &str, model: &str) -> Result<()> {
        let _ = provider_id;
        self.set_model(model)
    }

    /// Serialise the edited document. Byte-identical to the input when nothing changed.
    fn render(&self) -> String;

    fn provider(&self, id: &str) -> Option<Provider> {
        self.providers().into_iter().find(|p| p.id == id)
    }

    fn path(&self) -> &Path {
        &self.info().config_path
    }

    fn save(&self) -> Result<()> {
        store::write_atomic(self.path(), &self.render())
    }
}

/// Reject ids that would need quoting or would collide with table syntax.
pub fn validate_provider_id(id: &str) -> Result<()> {
    if id.is_empty() {
        bail!("provider id must not be empty");
    }
    if !id.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.')) {
        bail!("provider id {id:?} may only contain letters, digits, '_', '-' and '.'");
    }
    Ok(())
}

/// Every agent ConfAI supports, in display order.
pub fn all() -> Vec<Box<dyn Agent>> {
    let mut agents: Vec<Box<dyn Agent>> = Vec::new();
    if let Some(agent) = codex::Codex::discover() {
        agents.push(Box::new(agent));
    }
    if let Some(agent) = claude::Claude::discover() {
        agents.push(Box::new(agent));
    }
    if let Some(agent) = opencode::OpenCode::discover() {
        agents.push(Box::new(agent));
    }
    agents
}

/// The subset of [`all`] that is actually present on this machine.
pub fn installed() -> Vec<Box<dyn Agent>> {
    all().into_iter().filter(|a| a.detect().installed()).collect()
}

/// Look an agent up by id, whether or not it is installed.
pub fn find(id: &str) -> Result<Box<dyn Agent>> {
    let wanted = id.trim().to_ascii_lowercase();
    all().into_iter().find(|a| a.info().id == wanted).ok_or_else(|| {
        let known: Vec<&str> = all().iter().map(|a| a.info().id).collect();
        anyhow::anyhow!("unknown agent {id:?}; known agents: {}", known.join(", "))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn provider_ids_reject_table_breaking_characters() {
        assert!(validate_provider_id("primary").is_ok());
        assert!(validate_provider_id("codex-lb.2").is_ok());
        assert!(validate_provider_id("").is_err());
        assert!(validate_provider_id("has space").is_err());
        assert!(validate_provider_id("quote\"break").is_err());
    }

    #[test]
    fn detection_reports_the_evidence_it_found() {
        let d = Detection { binary_on_path: false, config_exists: true };
        assert!(d.installed());
        assert_eq!(d.describe(), "config only");

        let none = Detection { binary_on_path: false, config_exists: false };
        assert!(!none.installed());
    }
}
