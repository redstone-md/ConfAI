//! Named endpoint recipes, applied to any agent in one command.
//!
//! A preset describes an endpoint once, in agent-neutral terms. Applying it is
//! just an upsert of the [`Provider`] it carries, so a preset written for one
//! agent works for every other agent without being rewritten.

use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;

use anyhow::{bail, Context, Result};
use serde::Deserialize;

use crate::domain::{Model, Provider, WireApi};

include!(concat!(env!("OUT_DIR"), "/presets.rs"));

/// A recipe for one endpoint.
#[derive(Debug, Clone, Deserialize)]
pub struct Preset {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub homepage: Option<String>,
    /// Environment variable to read the key from when `--api-key` is not passed.
    #[serde(default)]
    pub api_key_env: Option<String>,
    /// Model to select after applying, if the agent tracks one.
    #[serde(default)]
    pub default_model: Option<String>,
    provider: ProviderSpec,
    #[serde(default)]
    models: Vec<ModelSpec>,
    /// Where this preset came from, for `preset list`.
    #[serde(skip)]
    pub origin: Origin,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Origin {
    #[default]
    Builtin,
    User,
}

impl Origin {
    pub fn as_str(self) -> &'static str {
        match self {
            Origin::Builtin => "built-in",
            Origin::User => "user",
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
struct ProviderSpec {
    /// Defaults to the preset id, so most presets never spell this out.
    #[serde(default)]
    id: Option<String>,
    #[serde(default)]
    display_name: Option<String>,
    base_url: String,
    #[serde(default)]
    wire_api: Option<String>,
    #[serde(default)]
    extras: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Deserialize)]
struct ModelSpec {
    id: String,
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    context: Option<u64>,
    #[serde(default)]
    output: Option<u64>,
}

impl Preset {
    /// The provider to upsert, with `api_key` resolved from the argument or the
    /// preset's environment variable.
    pub fn provider(&self, api_key: Option<&str>) -> Result<Provider> {
        let wire_api = match &self.provider.wire_api {
            Some(raw) => Some(
                WireApi::parse(raw)
                    .with_context(|| format!("preset {:?} has an unknown wire_api {raw:?}", self.id))?,
            ),
            None => None,
        };

        Ok(Provider {
            id: self.provider.id.clone().unwrap_or_else(|| self.id.clone()),
            display_name: self.provider.display_name.clone().or_else(|| Some(self.name.clone())),
            base_url: Some(self.provider.base_url.clone()),
            api_key: self.resolve_key(api_key),
            wire_api,
            models: self
                .models
                .iter()
                .map(|m| Model {
                    id: m.id.clone(),
                    display_name: m.name.clone(),
                    context_limit: m.context,
                    output_limit: m.output,
                })
                .collect(),
            extras: self.provider.extras.clone(),
        })
    }

    /// An explicit key wins; otherwise fall back to the preset's env var.
    fn resolve_key(&self, api_key: Option<&str>) -> Option<String> {
        api_key
            .map(str::to_owned)
            .or_else(|| self.api_key_env.as_ref().and_then(|var| std::env::var(var).ok()))
            .filter(|key| !key.is_empty())
    }

    /// Whether this preset needs a key the caller has not supplied.
    pub fn missing_key(&self, api_key: Option<&str>) -> bool {
        self.api_key_env.is_some() && self.resolve_key(api_key).is_none()
    }
}

/// Every preset, built-ins first, with user presets overriding same-id built-ins.
pub fn all() -> Result<Vec<Preset>> {
    let mut presets: Vec<Preset> = Vec::new();

    for (index, source) in BUILTIN_PRESETS.iter().enumerate() {
        let mut preset: Preset = toml_edit::de::from_str(source)
            .with_context(|| format!("parsing built-in preset #{index}"))?;
        preset.origin = Origin::Builtin;
        presets.push(preset);
    }

    for (path, source) in user_sources()? {
        let mut preset: Preset = toml_edit::de::from_str(&source)
            .with_context(|| format!("parsing {}", path.display()))?;
        preset.origin = Origin::User;
        match presets.iter().position(|p| p.id == preset.id) {
            Some(index) => presets[index] = preset,
            None => presets.push(preset),
        }
    }

    presets.sort_by(|a, b| a.id.cmp(&b.id));
    Ok(presets)
}

pub fn find(id: &str) -> Result<Preset> {
    let presets = all()?;
    let wanted = id.trim().to_ascii_lowercase();
    if let Some(preset) = presets.iter().find(|p| p.id.to_ascii_lowercase() == wanted) {
        return Ok(preset.clone());
    }
    let known: Vec<&str> = presets.iter().map(|p| p.id.as_str()).collect();
    if known.is_empty() {
        bail!("unknown preset {id:?}; no presets are available");
    }
    bail!("unknown preset {id:?}; available: {}", known.join(", "))
}

/// Where user-contributed presets live.
pub fn user_dir() -> Option<PathBuf> {
    dirs::home_dir().map(|home| home.join(".confai").join("presets"))
}

fn user_sources() -> Result<Vec<(PathBuf, String)>> {
    let Some(dir) = user_dir().filter(|dir| dir.is_dir()) else {
        return Ok(Vec::new());
    };

    let mut sources: Vec<(PathBuf, String)> = fs::read_dir(&dir)
        .with_context(|| format!("reading {}", dir.display()))?
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "toml"))
        .filter_map(|path| fs::read_to_string(&path).ok().map(|text| (path, text)))
        .collect();
    sources.sort_by(|a, b| a.0.cmp(&b.0));
    Ok(sources)
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r#"
id = "byesu"
name = "Byesu"
description = "Byesu gateway"
api_key_env = "BYESU_API_KEY"
default_model = "gpt-5.5"

[provider]
base_url = "https://byesu.com/v1"
wire_api = "chat"

[[models]]
id = "gpt-5.5"
name = "GPT 5.5"
context = 400000
output = 128000
"#;

    fn parse(source: &str) -> Preset {
        toml_edit::de::from_str(source).unwrap()
    }

    #[test]
    fn provider_id_defaults_to_the_preset_id() {
        let provider = parse(SAMPLE).provider(Some("k")).unwrap();
        assert_eq!(provider.id, "byesu");
        assert_eq!(provider.display_name.as_deref(), Some("Byesu"));
        assert_eq!(provider.wire_api, Some(WireApi::Chat));
        assert_eq!(provider.models[0].context_limit, Some(400_000));
    }

    #[test]
    fn an_explicit_key_beats_the_environment() {
        let preset = parse(SAMPLE);
        assert_eq!(preset.provider(Some("explicit")).unwrap().api_key.as_deref(), Some("explicit"));
        assert!(!preset.missing_key(Some("explicit")));
    }

    #[test]
    fn a_preset_wanting_a_key_says_so_when_it_has_none() {
        let preset = parse(SAMPLE);
        // The env var is not set in the test process.
        assert!(preset.missing_key(None));
        assert!(preset.provider(None).unwrap().api_key.is_none());
    }

    #[test]
    fn an_unknown_wire_api_is_rejected_with_the_preset_named() {
        let preset = parse(SAMPLE.replace(r#"wire_api = "chat""#, r#"wire_api = "carrier-pigeon""#).as_str());
        let err = preset.provider(None).unwrap_err().to_string();
        assert!(err.contains("byesu") && err.contains("carrier-pigeon"), "{err}");
    }

    #[test]
    fn every_shipped_preset_parses_and_has_a_unique_id() {
        let presets = all().expect("built-in presets must parse");
        let mut ids: Vec<&str> = presets.iter().map(|p| p.id.as_str()).collect();
        let count = ids.len();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), count, "duplicate preset ids");

        for preset in &presets {
            preset.provider(None).unwrap_or_else(|err| panic!("preset {}: {err}", preset.id));
        }
    }
}
