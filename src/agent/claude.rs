//! Claude Code — `~/.claude/settings.json`.
//!
//! Claude Code talks to one endpoint at a time, chosen by the `ANTHROPIC_*`
//! variables in the settings file's `env` block. There is no room in that shape
//! for the endpoints you are *not* currently using, so the roster lives in a
//! [`Sidecar`] and only the selected entry is written into `env`.

use std::path::PathBuf;

use anyhow::{Context, Result};
use serde_json::Value;

use super::json;
use super::sidecar::{self, Sidecar};
use super::{validate_provider_id, Agent, AgentConfig, AgentInfo, Capabilities};
use crate::domain::Provider;

const ENV_BASE_URL: &str = "ANTHROPIC_BASE_URL";
const ENV_AUTH_TOKEN: &str = "ANTHROPIC_AUTH_TOKEN";

pub struct Claude {
    info: AgentInfo,
}

impl Claude {
    /// Honours `CLAUDE_CONFIG_DIR`, which is how Claude Code relocates its state.
    pub fn discover() -> Option<Self> {
        let home = std::env::var_os("CLAUDE_CONFIG_DIR")
            .map(PathBuf::from)
            .or_else(|| dirs::home_dir().map(|h| h.join(".claude")))?;

        Some(Self {
            info: AgentInfo {
                id: "claude",
                name: "Claude Code",
                binaries: &["claude"],
                config_path: home.join("settings.json"),
                capabilities: Capabilities {
                    named_providers: false,
                    selectable_provider: true,
                    per_provider_models: false,
                    inline_api_key: true,
                },
            },
        })
    }
}

impl Agent for Claude {
    fn info(&self) -> &AgentInfo {
        &self.info
    }

    fn load(&self) -> Result<Box<dyn AgentConfig>> {
        let root = json::load(&self.info.config_path)?;
        let mut roster = Sidecar::load(self.info.id)?;

        // Whatever `env` currently points at is a real endpoint the user is
        // using; surface it rather than pretending only the roster exists.
        if let Some(current) = endpoint_in_env(&root) {
            roster.adopt(current);
        }

        Ok(Box::new(ClaudeConfig { info: self.info.clone(), root, roster }))
    }
}

pub struct ClaudeConfig {
    info: AgentInfo,
    root: Value,
    roster: Sidecar,
}

impl ClaudeConfig {
    fn env_value(&self, key: &str) -> Option<String> {
        json::string_at(&self.root, &["env", key])
    }
}

/// Reconstruct a [`Provider`] from the `env` block, if it names an endpoint.
fn endpoint_in_env(root: &Value) -> Option<Provider> {
    let base_url = json::string_at(root, &["env", ENV_BASE_URL])?;
    let mut provider = Provider::new(sidecar::id_from_url(&base_url));
    provider.api_key = json::string_at(root, &["env", ENV_AUTH_TOKEN]);
    provider.base_url = Some(base_url);
    provider.wire_api = Some(crate::domain::WireApi::Anthropic);
    Some(provider)
}

impl AgentConfig for ClaudeConfig {
    fn info(&self) -> &AgentInfo {
        &self.info
    }

    fn providers(&self) -> Vec<Provider> {
        self.roster.providers().to_vec()
    }

    fn upsert_provider(&mut self, provider: &Provider) -> Result<()> {
        validate_provider_id(&provider.id)?;
        self.roster.upsert(provider);

        // Keep `env` in step when the endpoint being edited is the live one.
        if self.active_provider().as_deref() == Some(provider.id.as_str()) {
            let id = provider.id.clone();
            self.set_active_provider(&id)?;
        }
        Ok(())
    }

    fn remove_provider(&mut self, id: &str) -> Result<bool> {
        if self.active_provider().as_deref() == Some(id) {
            if let Some(env) = self.root.get_mut("env").and_then(Value::as_object_mut) {
                env.remove(ENV_BASE_URL);
                env.remove(ENV_AUTH_TOKEN);
            }
        }
        Ok(self.roster.remove(id))
    }

    /// The roster entry whose URL matches what `env` points at.
    fn active_provider(&self) -> Option<String> {
        let live = self.env_value(ENV_BASE_URL)?;
        self.roster
            .providers()
            .iter()
            .find(|p| crate::domain::same_endpoint(p.base_url.as_deref(), Some(&live)))
            .map(|p| p.id.clone())
    }

    fn set_active_provider(&mut self, id: &str) -> Result<()> {
        let provider = self
            .roster
            .get(id)
            .cloned()
            .with_context(|| format!("Claude Code has no provider {id:?}"))?;
        let base_url = provider.base_url.clone().with_context(|| {
            format!("provider {id:?} has no base URL, so there is nothing to point Claude Code at")
        })?;

        let env = json::object_mut(&mut self.root, "env")?;
        env.insert(ENV_BASE_URL.into(), Value::from(base_url));
        json::set_or_clear(env, ENV_AUTH_TOKEN, provider.api_key.clone().map(Value::from));
        Ok(())
    }

    fn model(&self) -> Option<String> {
        self.root.get("model")?.as_str().map(str::to_owned)
    }

    fn set_model(&mut self, model: &str) -> Result<()> {
        self.root
            .as_object_mut()
            .context("config root is not a JSON object")?
            .insert("model".into(), Value::from(model));
        Ok(())
    }

    fn render(&self) -> String {
        json::render(&self.root)
    }

    fn save(&self) -> Result<()> {
        self.roster.save()?;
        json::write(self.path(), &self.root)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json as j;

    fn config(root: Value, roster: Vec<Provider>) -> ClaudeConfig {
        let mut sidecar = Sidecar::default();
        for provider in &roster {
            sidecar.upsert(provider);
        }
        if let Some(current) = endpoint_in_env(&root) {
            sidecar.adopt(current);
        }
        ClaudeConfig { info: Claude::discover().unwrap().info, root, roster: sidecar }
    }

    fn provider(id: &str, url: &str) -> Provider {
        let mut p = Provider::new(id);
        p.base_url = Some(url.into());
        p.api_key = Some(format!("key-for-{id}"));
        p
    }

    #[test]
    fn an_endpoint_already_in_env_is_adopted_and_reported_active() {
        let cfg = config(j!({ "env": { ENV_BASE_URL: "https://byesu.com" } }), vec![]);
        assert_eq!(cfg.providers().len(), 1);
        assert_eq!(cfg.active_provider().as_deref(), Some("byesu.com"));
    }

    #[test]
    fn switching_writes_both_env_vars_and_leaves_settings_alone() {
        let mut cfg = config(
            j!({ "model": "opus[1m]", "theme": "auto" }),
            vec![provider("byesu", "https://byesu.com/v1")],
        );
        cfg.set_active_provider("byesu").unwrap();

        assert_eq!(cfg.root["env"][ENV_BASE_URL], j!("https://byesu.com/v1"));
        assert_eq!(cfg.root["env"][ENV_AUTH_TOKEN], j!("key-for-byesu"));
        assert_eq!(cfg.root["theme"], j!("auto"));
        assert_eq!(cfg.active_provider().as_deref(), Some("byesu"));
    }

    #[test]
    fn a_provider_without_a_key_clears_the_stale_token() {
        let mut cfg = config(
            j!({ "env": { ENV_BASE_URL: "https://old", ENV_AUTH_TOKEN: "stale" } }),
            vec![{
                let mut p = Provider::new("keyless");
                p.base_url = Some("https://keyless".into());
                p
            }],
        );
        cfg.set_active_provider("keyless").unwrap();

        assert_eq!(cfg.root["env"][ENV_BASE_URL], j!("https://keyless"));
        assert!(cfg.root["env"].get(ENV_AUTH_TOKEN).is_none(), "kept a token from another endpoint");
    }

    #[test]
    fn trailing_slashes_do_not_hide_the_active_endpoint() {
        let cfg = config(
            j!({ "env": { ENV_BASE_URL: "https://byesu.com/v1/" } }),
            vec![provider("byesu", "https://byesu.com/v1")],
        );
        assert_eq!(cfg.active_provider().as_deref(), Some("byesu"));
    }

    #[test]
    fn editing_the_live_provider_updates_env_too() {
        let mut cfg = config(
            j!({ "env": { ENV_BASE_URL: "https://byesu.com/v1" } }),
            vec![provider("byesu", "https://byesu.com/v1")],
        );
        let mut patch = Provider::new("byesu");
        patch.api_key = Some("rotated".into());
        cfg.upsert_provider(&patch).unwrap();

        assert_eq!(cfg.root["env"][ENV_AUTH_TOKEN], j!("rotated"));
    }

    #[test]
    fn removing_the_live_provider_unhooks_env() {
        let mut cfg = config(
            j!({ "env": { ENV_BASE_URL: "https://byesu.com/v1", ENV_AUTH_TOKEN: "k", "OTHER": "keep" } }),
            vec![provider("byesu", "https://byesu.com/v1")],
        );
        assert!(cfg.remove_provider("byesu").unwrap());

        assert!(cfg.root["env"].get(ENV_BASE_URL).is_none());
        assert_eq!(cfg.root["env"]["OTHER"], j!("keep"), "clobbered an unrelated variable");
        assert!(cfg.active_provider().is_none());
    }
}
