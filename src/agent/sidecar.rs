//! A ConfAI-owned provider list for agents whose config has no place for one.
//!
//! Claude Code points at exactly one endpoint, through environment variables.
//! There is nowhere in its settings to park the other three endpoints you
//! switch between. Rather than inventing keys inside a file another program
//! owns, ConfAI keeps the roster beside its own state and writes only the
//! selected entry into the agent's config.

use std::path::PathBuf;

use anyhow::{Context, Result};

use crate::domain::Provider;
use crate::store;

/// The provider roster for one agent.
#[derive(Debug, Default)]
pub struct Sidecar {
    path: PathBuf,
    providers: Vec<Provider>,
}

impl Sidecar {
    /// Load the roster for `agent_id`, or start an empty one.
    pub fn load(agent_id: &str) -> Result<Self> {
        let path = path_for(agent_id)?;
        let text = store::read_or_empty(&path)?;
        let providers = if text.trim().is_empty() {
            Vec::new()
        } else {
            serde_json::from_str(&text)
                .with_context(|| format!("parsing {}", path.display()))?
        };
        Ok(Self { path, providers })
    }

    pub fn path(&self) -> &PathBuf {
        &self.path
    }

    pub fn providers(&self) -> &[Provider] {
        &self.providers
    }

    pub fn get(&self, id: &str) -> Option<&Provider> {
        self.providers.iter().find(|p| p.id == id)
    }

    /// Overlay onto the entry with the same id, or append a new one.
    pub fn upsert(&mut self, provider: &Provider) {
        match self.providers.iter_mut().find(|p| p.id == provider.id) {
            Some(existing) => existing.merge_from(provider),
            None => self.providers.push(provider.clone()),
        }
    }

    pub fn remove(&mut self, id: &str) -> bool {
        let before = self.providers.len();
        self.providers.retain(|p| p.id != id);
        before != self.providers.len()
    }

    /// Record an endpoint that was already configured in the agent, so it shows
    /// up in listings instead of looking like it does not exist.
    ///
    /// An endpoint the roster already covers is skipped even when its id differs:
    /// the id here is synthesised from the URL, so `byesu` and `byesu.com` would
    /// otherwise both appear for one endpoint.
    pub fn adopt(&mut self, provider: Provider) {
        let already_known = self.providers.iter().any(|p| {
            p.id == provider.id
                || crate::domain::same_endpoint(p.base_url.as_deref(), provider.base_url.as_deref())
        });
        if !already_known {
            self.providers.push(provider);
        }
    }

    pub fn save(&self) -> Result<()> {
        let mut text = serde_json::to_string_pretty(&self.providers)
            .context("serialising the provider roster")?;
        text.push('\n');
        store::write_atomic(&self.path, &text)
    }
}

fn path_for(agent_id: &str) -> Result<PathBuf> {
    let home = dirs::home_dir().context("locating the home directory")?;
    Ok(home.join(".confai").join("agents").join(format!("{agent_id}.json")))
}

/// Derive a stable, id-safe name from a base URL, for endpoints ConfAI adopts
/// from an agent that only stored a bare URL.
pub fn id_from_url(url: &str) -> String {
    let rest = url.split_once("://").map_or(url, |(_, rest)| rest);
    let host = rest.split('/').next().unwrap_or(rest);
    let cleaned: String = host
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() || c == '-' || c == '.' { c } else { '-' })
        .collect();
    let trimmed = cleaned.trim_matches(['.', '-']).to_string();
    if trimmed.is_empty() {
        "current".to_string()
    } else {
        trimmed
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sidecar() -> Sidecar {
        Sidecar { path: std::env::temp_dir().join("confai-sidecar-test.json"), providers: Vec::new() }
    }

    #[test]
    fn upsert_merges_instead_of_duplicating() {
        let mut s = sidecar();
        let mut first = Provider::new("byesu");
        first.base_url = Some("https://byesu.com".into());
        first.api_key = Some("k".into());
        s.upsert(&first);

        let mut patch = Provider::new("byesu");
        patch.base_url = Some("https://byesu.com/v1".into());
        s.upsert(&patch);

        assert_eq!(s.providers().len(), 1);
        assert_eq!(s.get("byesu").unwrap().base_url.as_deref(), Some("https://byesu.com/v1"));
        assert_eq!(s.get("byesu").unwrap().api_key.as_deref(), Some("k"));
    }

    #[test]
    fn adopt_never_overwrites_a_managed_entry() {
        let mut s = sidecar();
        let mut managed = Provider::new("byesu.com");
        managed.api_key = Some("managed".into());
        s.upsert(&managed);

        s.adopt(Provider::new("byesu.com"));
        assert_eq!(s.providers().len(), 1);
        assert_eq!(s.get("byesu.com").unwrap().api_key.as_deref(), Some("managed"));
    }

    #[test]
    fn adopt_recognises_a_known_endpoint_under_a_different_id() {
        let mut s = sidecar();
        let mut managed = Provider::new("byesu");
        managed.base_url = Some("https://byesu.com/v1".into());
        s.upsert(&managed);

        // What ConfAI would synthesise from the agent's own config.
        let mut synthesised = Provider::new(id_from_url("https://byesu.com/v1"));
        synthesised.base_url = Some("https://byesu.com/v1/".into());
        s.adopt(synthesised);

        assert_eq!(s.providers().len(), 1, "one endpoint listed twice: {:?}", s.providers());
        assert_eq!(s.providers()[0].id, "byesu");
    }

    #[test]
    fn remove_reports_whether_anything_went() {
        let mut s = sidecar();
        s.upsert(&Provider::new("a"));
        assert!(s.remove("a"));
        assert!(!s.remove("a"));
    }

    #[test]
    fn ids_derived_from_urls_are_id_safe() {
        assert_eq!(id_from_url("https://byesu.com/v1"), "byesu.com");
        assert_eq!(id_from_url("http://127.0.0.1:1337/v1"), "127.0.0.1-1337");
        assert_eq!(id_from_url("https://"), "current");
        assert!(crate::agent::validate_provider_id(&id_from_url("http://x.y:80/v1")).is_ok());
    }
}
