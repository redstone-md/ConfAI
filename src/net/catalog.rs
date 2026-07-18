//! Model facts from <https://models.dev> — context limits and prices.
//!
//! A provider's `/v1/models` tells you which ids exist but not how big their
//! context is, which is exactly what opencode's config needs. models.dev fills
//! that gap. Its catalogue is a few megabytes, so it is cached on disk and only
//! re-fetched once a day.

use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use std::time::Duration;

use anyhow::{Context, Result};
use serde::Deserialize;

const CATALOG_URL: &str = "https://models.dev/api.json";
const MAX_AGE: Duration = Duration::from_secs(24 * 60 * 60);
const FETCH_TIMEOUT: Duration = Duration::from_secs(30);

/// What ConfAI keeps from a models.dev entry.
#[derive(Debug, Clone, PartialEq)]
pub struct Facts {
    pub id: String,
    pub name: Option<String>,
    pub context: Option<u64>,
    pub output: Option<u64>,
    /// USD per million input tokens.
    pub cost_input: Option<f64>,
    /// USD per million output tokens.
    pub cost_output: Option<f64>,
}

impl Facts {
    /// Price line for listings, or `None` when models.dev has no cost data.
    pub fn price(&self) -> Option<String> {
        let (input, output) = (self.cost_input?, self.cost_output?);
        Some(format!("${input:.2}/${output:.2} per Mtok"))
    }
}

/// Every model models.dev knows about, indexed for lookup by id.
#[derive(Debug, Default)]
pub struct Catalog {
    by_id: HashMap<String, Facts>,
}

impl Catalog {
    /// Load from cache, fetching when the cache is missing, stale or `refresh` is set.
    pub fn load(refresh: bool) -> Result<Self> {
        let path = cache_path()?;
        let raw = match cached(&path, refresh) {
            Some(text) => text,
            None => {
                let text = fetch()?;
                if let Some(parent) = path.parent() {
                    let _ = fs::create_dir_all(parent);
                }
                let _ = fs::write(&path, &text);
                text
            }
        };
        Self::parse(&raw)
    }

    /// Load from cache only. Used on paths where a network stall would be worse
    /// than missing enrichment.
    pub fn cached_only() -> Option<Self> {
        let path = cache_path().ok()?;
        let raw = cached(&path, false)?;
        Self::parse(&raw).ok()
    }

    fn parse(raw: &str) -> Result<Self> {
        let providers: HashMap<String, ProviderEntry> =
            serde_json::from_str(raw).context("parsing the models.dev catalogue")?;

        let mut by_id: HashMap<String, Facts> = HashMap::new();
        for provider in providers.into_values() {
            for (key, model) in provider.models {
                let facts = Facts {
                    id: model.id.clone().unwrap_or_else(|| key.clone()),
                    name: model.name,
                    context: model.limit.as_ref().and_then(|l| l.context),
                    output: model.limit.as_ref().and_then(|l| l.output),
                    cost_input: model.cost.as_ref().and_then(|c| c.input),
                    cost_output: model.cost.as_ref().and_then(|c| c.output),
                };
                // The same id is served by many providers with identical limits;
                // the richer record wins so partial entries never mask a full one.
                by_id
                    .entry(key)
                    .and_modify(|existing| {
                        if existing.context.is_none() {
                            *existing = facts.clone();
                        }
                    })
                    .or_insert(facts);
            }
        }
        Ok(Self { by_id })
    }

    pub fn len(&self) -> usize {
        self.by_id.len()
    }

    pub fn is_empty(&self) -> bool {
        self.by_id.is_empty()
    }

    /// Look a model up by exact id, then by the id with any vendor prefix stripped.
    ///
    /// Gateways rename freely: `xiaomi/mimo-v2.5-pro` upstream is `mimo-v2.5-pro`
    /// on models.dev, and vice versa.
    pub fn lookup(&self, id: &str) -> Option<&Facts> {
        if let Some(facts) = self.by_id.get(id) {
            return Some(facts);
        }
        if let Some((_, bare)) = id.split_once('/') {
            if let Some(facts) = self.by_id.get(bare) {
                return Some(facts);
            }
        }
        self.by_id
            .iter()
            .find(|(key, _)| key.rsplit('/').next() == id.rsplit('/').next())
            .map(|(_, facts)| facts)
    }
}

#[derive(Deserialize)]
struct ProviderEntry {
    #[serde(default)]
    models: HashMap<String, ModelEntry>,
}

#[derive(Deserialize)]
struct ModelEntry {
    id: Option<String>,
    name: Option<String>,
    limit: Option<Limit>,
    cost: Option<Cost>,
}

#[derive(Deserialize)]
struct Limit {
    context: Option<u64>,
    output: Option<u64>,
}

#[derive(Deserialize)]
struct Cost {
    input: Option<f64>,
    output: Option<f64>,
}

pub fn cache_path() -> Result<PathBuf> {
    let dir = dirs::cache_dir()
        .or_else(dirs::home_dir)
        .context("locating a cache directory")?;
    Ok(dir.join("confai").join("models.dev.json"))
}

/// Cached contents, unless the cache is absent, unreadable or older than [`MAX_AGE`].
fn cached(path: &PathBuf, refresh: bool) -> Option<String> {
    if refresh {
        return None;
    }
    let age = fs::metadata(path).ok()?.modified().ok()?.elapsed().ok()?;
    if age > MAX_AGE {
        return None;
    }
    fs::read_to_string(path).ok()
}

fn fetch() -> Result<String> {
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .timeout_global(Some(FETCH_TIMEOUT))
        .user_agent(concat!("confai/", env!("CARGO_PKG_VERSION")))
        .build()
        .into();

    agent
        .get(CATALOG_URL)
        .call()
        .context("fetching https://models.dev/api.json")?
        .body_mut()
        .read_to_string()
        .context("reading the models.dev response")
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r#"{
      "anthropic": {
        "id": "anthropic",
        "models": {
          "claude-opus-4-8": {
            "id": "claude-opus-4-8",
            "name": "Claude Opus 4.8",
            "limit": { "context": 1000000, "output": 128000 },
            "cost": { "input": 5, "output": 25 }
          }
        }
      },
      "xiaomi": {
        "id": "xiaomi",
        "models": {
          "mimo-v2.5-pro": {
            "id": "mimo-v2.5-pro",
            "limit": { "context": 1000000, "output": 8192 }
          }
        }
      }
    }"#;

    #[test]
    fn exact_id_lookup_carries_limits_and_price() {
        let catalog = Catalog::parse(SAMPLE).unwrap();
        let facts = catalog.lookup("claude-opus-4-8").unwrap();
        assert_eq!(facts.context, Some(1_000_000));
        assert_eq!(facts.output, Some(128_000));
        assert_eq!(facts.price().as_deref(), Some("$5.00/$25.00 per Mtok"));
    }

    #[test]
    fn vendor_prefixes_resolve_in_both_directions() {
        let catalog = Catalog::parse(SAMPLE).unwrap();
        assert_eq!(catalog.lookup("xiaomi/mimo-v2.5-pro").unwrap().context, Some(1_000_000));
        assert_eq!(catalog.lookup("mimo-v2.5-pro").unwrap().context, Some(1_000_000));
    }

    #[test]
    fn unknown_models_return_nothing() {
        let catalog = Catalog::parse(SAMPLE).unwrap();
        assert!(catalog.lookup("no-such-model").is_none());
        assert_eq!(catalog.len(), 2);
    }
}
