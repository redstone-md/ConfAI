//! opencode — `~/.config/opencode/opencode.json`, plus its credential file.
//!
//! The only backend that stores a model list per provider, and the reason
//! `provider sync` exists: opencode will not offer a model it has not been told
//! about, and it needs the context limit spelled out.
//!
//! It is also the only backend spread over two files. Keys live in
//! [`auth`], not in the config, so a provider that looks keyless here usually is
//! not — reading both is what makes a health check tell the truth.

pub mod auth;

use std::path::PathBuf;

use anyhow::{Context, Result};
use serde_json::{json, Map, Value};

use self::auth::AuthStore;
use super::json;
use super::{validate_provider_id, Agent, AgentConfig, AgentInfo, Capabilities};
use crate::domain::{Model, Provider, WireApi};

/// The `npm` adapter opencode loads for each wire protocol.
/// Synthetic extra naming how a provider authenticates. It is reported, never
/// written: [`OpenCodeConfig::upsert_provider`] only ever writes `npm`.
pub const AUTH_EXTRA: &str = "auth";

const NPM_CHAT: &str = "@ai-sdk/openai-compatible";
const NPM_ANTHROPIC: &str = "@ai-sdk/anthropic";
const NPM_RESPONSES: &str = "@ai-sdk/openai";

pub struct OpenCode {
    info: AgentInfo,
}

impl OpenCode {
    /// Honours `OPENCODE_CONFIG`, then `XDG_CONFIG_HOME`, then `~/.config`.
    pub fn discover() -> Option<Self> {
        let path = match std::env::var_os("OPENCODE_CONFIG") {
            Some(explicit) => PathBuf::from(explicit),
            None => config_home()?.join("opencode").join("opencode.json"),
        };

        Some(Self {
            info: AgentInfo {
                id: "opencode",
                name: "opencode",
                binaries: &["opencode"],
                config_path: path,
                capabilities: Capabilities {
                    named_providers: true,
                    selectable_provider: true,
                    per_provider_models: true,
                    inline_api_key: true,
                },
            },
        })
    }
}

fn config_home() -> Option<PathBuf> {
    std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| dirs::home_dir().map(|h| h.join(".config")))
}

impl Agent for OpenCode {
    fn info(&self) -> &AgentInfo {
        &self.info
    }

    fn load(&self) -> Result<Box<dyn AgentConfig>> {
        let root = json::load(&self.info.config_path)?;
        // A missing or unreadable credential file must not stop the config being
        // edited; it only means no keys are known.
        let auth = AuthStore::load().unwrap_or_else(|_| {
            AuthStore::empty(auth::auth_path().unwrap_or_else(|_| PathBuf::from("auth.json")))
        });
        Ok(Box::new(OpenCodeConfig { info: self.info.clone(), root, auth }))
    }
}

pub struct OpenCodeConfig {
    info: AgentInfo,
    root: Value,
    auth: AuthStore,
}

impl OpenCodeConfig {
    /// The credential file backing this config.
    pub fn auth(&self) -> &AuthStore {
        &self.auth
    }

    fn read_provider(id: &str, entry: &Value) -> Provider {
        let models = json::object(entry, "models")
            .map(|models| {
                models
                    .iter()
                    .map(|(model_id, spec)| Model {
                        id: model_id.clone(),
                        display_name: spec.get("name").and_then(Value::as_str).map(str::to_owned),
                        context_limit: spec.pointer("/limit/context").and_then(Value::as_u64),
                        output_limit: spec.pointer("/limit/output").and_then(Value::as_u64),
                    })
                    .collect()
            })
            .unwrap_or_default();

        let mut extras = std::collections::BTreeMap::new();
        if let Some(npm) = entry.get("npm").and_then(Value::as_str) {
            extras.insert("npm".to_string(), npm.to_string());
        }

        Provider {
            id: id.to_string(),
            display_name: entry.get("name").and_then(Value::as_str).map(str::to_owned),
            base_url: json::string_at(entry, &["options", "baseURL"]),
            api_key: json::string_at(entry, &["options", "apiKey"]),
            wire_api: entry.get("npm").and_then(Value::as_str).and_then(npm_to_wire),
            models,
            extras,
        }
    }

    /// Splice a model into `models`, preserving any keys ConfAI does not model
    /// (`variants`, `reasoning`, and whatever opencode adds next).
    fn write_model(models: &mut Map<String, Value>, model: &Model) {
        let entry = models
            .entry(model.id.clone())
            .or_insert_with(|| Value::Object(Map::new()));
        let Some(spec) = entry.as_object_mut() else {
            *entry = json!({ "name": model.label() });
            return;
        };

        if let Some(name) = &model.display_name {
            spec.insert("name".into(), json!(name));
        }

        if model.context_limit.is_none() && model.output_limit.is_none() {
            return;
        }
        let limit = spec
            .entry("limit".to_string())
            .or_insert_with(|| Value::Object(Map::new()));
        if let Some(limit) = limit.as_object_mut() {
            if let Some(context) = model.context_limit {
                limit.insert("context".into(), json!(context));
            }
            if let Some(output) = model.output_limit {
                limit.insert("output".into(), json!(output));
            }
        }
    }
}

impl AgentConfig for OpenCodeConfig {
    fn info(&self) -> &AgentInfo {
        &self.info
    }

    fn providers(&self) -> Vec<Provider> {
        json::object(&self.root, "provider")
            .map(|providers| {
                providers
                    .iter()
                    .map(|(id, entry)| {
                        let mut provider = Self::read_provider(id, entry);
                        // An inline key in the config wins, because that is what
                        // opencode itself would load first.
                        if provider.api_key.is_none() {
                            provider.api_key = self.auth.key(id);
                        }
                        if let Some(method) = self.auth.method(id) {
                            provider.extras.insert(AUTH_EXTRA.into(), method.as_str().into());
                        }
                        provider
                    })
                    .collect()
            })
            .unwrap_or_default()
    }

    fn upsert_provider(&mut self, provider: &Provider) -> Result<()> {
        validate_provider_id(&provider.id)?;

        let merged = match self.provider(&provider.id) {
            Some(mut existing) => {
                existing.merge_from(provider);
                existing
            }
            None => provider.clone(),
        };

        // A key already inline in the config stays inline; anything else goes to
        // the credential file, where `opencode auth login` puts it. Moving a
        // secret between files behind the user's back would be worse than either.
        let key_is_inline = json::string_at(&self.root, &["provider", &merged.id, "options", "apiKey"])
            .is_some();
        let inline_key = merged.api_key.clone().filter(|_| key_is_inline);

        let providers = json::object_mut(&mut self.root, "provider")?;
        let entry = providers
            .entry(merged.id.clone())
            .or_insert_with(|| Value::Object(Map::new()));
        let entry = entry
            .as_object_mut()
            .with_context(|| format!("`provider.{}` is not a JSON object", merged.id))?;

        json::set_or_clear(entry, "name", merged.display_name.clone().map(Value::from));

        // opencode picks its SDK adapter from `npm`; an explicit extra wins over
        // the one inferred from the wire protocol.
        let npm = merged
            .extras
            .get("npm")
            .cloned()
            .or_else(|| merged.wire_api.map(|w| wire_to_npm(w).to_string()));
        json::set_or_clear(entry, "npm", npm.map(Value::from));

        if merged.base_url.is_some() || inline_key.is_some() {
            let options = entry
                .entry("options".to_string())
                .or_insert_with(|| Value::Object(Map::new()));
            if let Some(options) = options.as_object_mut() {
                if let Some(url) = &merged.base_url {
                    options.insert("baseURL".into(), json!(url));
                }
                if let Some(key) = &inline_key {
                    options.insert("apiKey".into(), json!(key));
                }
            }
        }

        if !merged.models.is_empty() {
            let models = entry
                .entry("models".to_string())
                .or_insert_with(|| Value::Object(Map::new()));
            if let Some(models) = models.as_object_mut() {
                for model in &merged.models {
                    Self::write_model(models, model);
                }
            }
        }

        if let Some(key) = merged.api_key.filter(|_| !key_is_inline) {
            if self.auth.key(&merged.id).as_deref() != Some(key.as_str()) {
                self.auth.set_key(&merged.id, &key)?;
            }
        }
        Ok(())
    }

    fn remove_provider(&mut self, id: &str) -> Result<bool> {
        Ok(json::object_mut(&mut self.root, "provider")?.remove(id).is_some())
    }

    fn prune_models(&mut self, id: &str, keep: &[String]) -> Result<usize> {
        let Some(models) = self
            .root
            .get_mut("provider")
            .and_then(|providers| providers.get_mut(id))
            .and_then(|provider| provider.get_mut("models"))
            .and_then(Value::as_object_mut)
        else {
            return Ok(0);
        };

        let before = models.len();
        models.retain(|model_id, _| keep.iter().any(|kept| kept == model_id));
        let removed = before - models.len();

        // A selected model that has just been pruned would leave opencode
        // pointing at something the provider no longer serves.
        if removed > 0 {
            if let Some(current) = self.model() {
                if let Some((provider, model)) = current.split_once('/') {
                    if provider == id && !keep.iter().any(|kept| kept == model) {
                        if let Some(fallback) = keep.first() {
                            self.set_model(&format!("{id}/{fallback}"))?;
                        }
                    }
                }
            }
        }
        Ok(removed)
    }

    /// opencode selects a provider through the `provider/model` pair in `model`.
    fn active_provider(&self) -> Option<String> {
        let model = self.root.get("model")?.as_str()?;
        model.split_once('/').map(|(provider, _)| provider.to_string())
    }

    fn set_active_provider(&mut self, id: &str) -> Result<()> {
        validate_provider_id(id)?;

        let provider = self
            .provider(id)
            .with_context(|| format!("opencode has no provider {id:?}"))?;

        // Keep the current model when the target provider serves it, so switching
        // between two gateways that both host a model is not also a model change.
        let current = self.model().and_then(|m| {
            m.split_once('/').map(|(_, model)| model.to_string())
        });
        let model = current
            .filter(|m| provider.models.iter().any(|candidate| &candidate.id == m))
            .or_else(|| provider.models.first().map(|m| m.id.clone()))
            .with_context(|| {
                format!("provider {id:?} lists no models; run `confai provider sync {id}` first")
            })?;

        self.root
            .as_object_mut()
            .context("config root is not a JSON object")?
            .insert("model".into(), json!(format!("{id}/{model}")));
        Ok(())
    }

    fn model(&self) -> Option<String> {
        self.root.get("model")?.as_str().map(str::to_owned)
    }

    /// Accepts either `provider/model` or a bare model id, which is resolved
    /// against the currently selected provider.
    fn set_model(&mut self, model: &str) -> Result<()> {
        let qualified = if model.contains('/') {
            model.to_string()
        } else {
            let provider = self.active_provider().with_context(|| {
                format!("no provider selected; use `provider/model` instead of {model:?}")
            })?;
            format!("{provider}/{model}")
        };

        self.root
            .as_object_mut()
            .context("config root is not a JSON object")?
            .insert("model".into(), json!(qualified));
        Ok(())
    }

    fn render(&self) -> String {
        json::render(&self.root)
    }

    fn save(&self) -> Result<()> {
        // Credentials first: a config naming a provider whose key failed to land
        // is a clearer state to recover from than a key with no provider.
        self.auth.save()?;
        json::write(self.path(), &self.root)
    }
}

fn wire_to_npm(wire: WireApi) -> &'static str {
    match wire {
        WireApi::Chat => NPM_CHAT,
        WireApi::Responses => NPM_RESPONSES,
        WireApi::Anthropic => NPM_ANTHROPIC,
    }
}

fn npm_to_wire(npm: &str) -> Option<WireApi> {
    match npm {
        NPM_CHAT => Some(WireApi::Chat),
        NPM_RESPONSES => Some(WireApi::Responses),
        NPM_ANTHROPIC => Some(WireApi::Anthropic),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r#"{
      "$schema": "https://opencode.ai/config.json",
      "model": "vendor/gpt-5.5",
      "provider": {
        "vendor": {
          "npm": "@ai-sdk/openai-compatible",
          "name": "Codex Sale",
          "options": { "baseURL": "https://vendor.example/v1", "apiKey": "sk-live" },
          "models": {
            "gpt-5.5": {
              "name": "GPT 5.5",
              "variants": { "high": { "reasoningEffort": "high" } }
            }
          }
        },
        "local": {
          "name": "LocalModels",
          "options": { "baseURL": "http://127.0.0.1:1337/v1" },
          "models": { "gpt-5.5": { "name": "local copy" } }
        }
      }
    }"#;

    fn config(text: &str) -> OpenCodeConfig {
        config_with_keys(text, &[])
    }

    /// A config plus the API keys opencode would have in its credential file.
    fn config_with_keys(text: &str, keys: &[(&str, &str)]) -> OpenCodeConfig {
        let mut auth = AuthStore::empty(PathBuf::from("auth.json"));
        for (id, key) in keys {
            auth.set_key(id, key).unwrap();
        }
        OpenCodeConfig {
            info: OpenCode::discover().unwrap().info,
            root: serde_json::from_str(text).unwrap(),
            auth,
        }
    }

    #[test]
    fn a_key_from_the_credential_file_reaches_the_provider() {
        let cfg = config_with_keys(SAMPLE, &[("local", "sk-from-auth")]);

        // `local` has no inline key in the config, so the credential file supplies it.
        let local = cfg.provider("local").unwrap();
        assert_eq!(local.api_key.as_deref(), Some("sk-from-auth"));
        assert_eq!(local.extras.get(AUTH_EXTRA).map(String::as_str), Some("api"));
    }

    #[test]
    fn an_inline_key_wins_because_opencode_loads_it_first() {
        let cfg = config_with_keys(SAMPLE, &[("vendor", "sk-from-auth")]);
        assert_eq!(cfg.provider("vendor").unwrap().api_key.as_deref(), Some("sk-live"));
    }

    #[test]
    fn a_new_key_goes_to_the_credential_file_not_the_config() {
        let mut cfg = config(SAMPLE);
        let mut patch = Provider::new("local");
        patch.api_key = Some("sk-fresh".into());
        cfg.upsert_provider(&patch).unwrap();

        assert_eq!(cfg.auth().key("local").as_deref(), Some("sk-fresh"));
        assert!(
            cfg.root["provider"]["local"]["options"].get("apiKey").is_none(),
            "secret leaked into opencode.json: {}",
            cfg.root["provider"]["local"]["options"]
        );
    }

    #[test]
    fn a_key_already_inline_is_updated_where_it_lives() {
        let mut cfg = config(SAMPLE);
        let mut patch = Provider::new("vendor");
        patch.api_key = Some("sk-rotated".into());
        cfg.upsert_provider(&patch).unwrap();

        assert_eq!(cfg.root["provider"]["vendor"]["options"]["apiKey"], json!("sk-rotated"));
        assert!(cfg.auth().key("vendor").is_none(), "secret duplicated into auth.json");
    }

    #[test]
    fn rotating_a_key_updates_it_in_the_credential_file() {
        let mut cfg = config(SAMPLE);
        cfg.auth.set_key("local", "sk-old").unwrap();

        let mut patch = Provider::new("local");
        patch.api_key = Some("sk-new".into());
        cfg.upsert_provider(&patch).unwrap();
        assert_eq!(cfg.auth().key("local").as_deref(), Some("sk-new"));
    }

    #[test]
    fn reads_providers_models_and_wire_api() {
        let cfg = config(SAMPLE);
        let provider = cfg.provider("vendor").unwrap();

        assert_eq!(provider.display_name.as_deref(), Some("Codex Sale"));
        assert_eq!(provider.base_url.as_deref(), Some("https://vendor.example/v1"));
        assert_eq!(provider.api_key.as_deref(), Some("sk-live"));
        assert_eq!(provider.wire_api, Some(WireApi::Chat));
        assert_eq!(provider.models.len(), 1);
        assert_eq!(provider.models[0].display_name.as_deref(), Some("GPT 5.5"));
    }

    #[test]
    fn syncing_models_keeps_variants_and_adds_limits() {
        let mut cfg = config(SAMPLE);
        let mut patch = Provider::new("vendor");
        patch.models = vec![Model {
            id: "gpt-5.5".into(),
            display_name: Some("GPT 5.5".into()),
            context_limit: Some(400_000),
            output_limit: Some(128_000),
        }];
        cfg.upsert_provider(&patch).unwrap();

        let spec = &cfg.root["provider"]["vendor"]["models"]["gpt-5.5"];
        assert_eq!(spec["limit"]["context"], json!(400_000));
        assert_eq!(spec["limit"]["output"], json!(128_000));
        assert!(spec.get("variants").is_some(), "sync dropped variants: {spec}");
        // Untouched siblings stay untouched.
        assert_eq!(cfg.root["provider"]["vendor"]["options"]["apiKey"], json!("sk-live"));
    }

    #[test]
    fn wire_api_chooses_the_npm_adapter() {
        let mut cfg = config(SAMPLE);
        let mut patch = Provider::new("byesu");
        patch.base_url = Some("https://byesu.com/v1".into());
        patch.wire_api = Some(WireApi::Anthropic);
        cfg.upsert_provider(&patch).unwrap();

        assert_eq!(cfg.root["provider"]["byesu"]["npm"], json!(NPM_ANTHROPIC));
        assert_eq!(cfg.provider("byesu").unwrap().wire_api, Some(WireApi::Anthropic));
    }

    #[test]
    fn switching_provider_keeps_the_model_when_both_serve_it() {
        let mut cfg = config(SAMPLE);
        cfg.set_active_provider("local").unwrap();
        assert_eq!(cfg.model().as_deref(), Some("local/gpt-5.5"));
        assert_eq!(cfg.active_provider().as_deref(), Some("local"));
    }

    #[test]
    fn switching_to_a_provider_with_no_models_explains_the_fix() {
        let mut cfg = config(SAMPLE);
        cfg.upsert_provider(&Provider::new("empty")).unwrap();

        let err = cfg.set_active_provider("empty").unwrap_err().to_string();
        assert!(err.contains("provider sync"), "unhelpful error: {err}");
    }

    #[test]
    fn bare_model_ids_resolve_against_the_active_provider() {
        let mut cfg = config(SAMPLE);
        cfg.set_model("gpt-5.4").unwrap();
        assert_eq!(cfg.model().as_deref(), Some("vendor/gpt-5.4"));

        cfg.set_model("local/other").unwrap();
        assert_eq!(cfg.model().as_deref(), Some("local/other"));
    }

    #[test]
    fn pruning_drops_only_models_the_endpoint_no_longer_serves() {
        let mut cfg = config(SAMPLE);
        let mut patch = Provider::new("vendor");
        patch.models = vec![Model::new("gpt-5.6"), Model::new("gpt-5.5")];
        cfg.upsert_provider(&patch).unwrap();
        assert_eq!(cfg.provider("vendor").unwrap().models.len(), 2);

        let removed = cfg.prune_models("vendor", &["gpt-5.6".to_string()]).unwrap();

        assert_eq!(removed, 1);
        let models = cfg.provider("vendor").unwrap().models;
        assert_eq!(models.len(), 1);
        assert_eq!(models[0].id, "gpt-5.6");
        // Other providers are none of pruning's business.
        assert_eq!(cfg.provider("local").unwrap().models.len(), 1);
    }

    #[test]
    fn pruning_the_selected_model_moves_the_selection_to_a_surviving_one() {
        let mut cfg = config(SAMPLE);
        assert_eq!(cfg.model().as_deref(), Some("vendor/gpt-5.5"));

        let mut patch = Provider::new("vendor");
        patch.models = vec![Model::new("gpt-5.6")];
        cfg.upsert_provider(&patch).unwrap();
        cfg.prune_models("vendor", &["gpt-5.6".to_string()]).unwrap();

        assert_eq!(cfg.model().as_deref(), Some("vendor/gpt-5.6"));
    }

    #[test]
    fn pruning_leaves_another_providers_selection_alone() {
        let mut cfg = config(SAMPLE);
        cfg.set_model("local/gpt-5.5").unwrap();
        cfg.prune_models("vendor", &[]).unwrap();

        assert_eq!(cfg.model().as_deref(), Some("local/gpt-5.5"));
    }

    #[test]
    fn pruning_an_unknown_provider_is_a_no_op() {
        let mut cfg = config(SAMPLE);
        assert_eq!(cfg.prune_models("nope", &[]).unwrap(), 0);
    }

    #[test]
    fn removing_a_provider_leaves_the_rest_intact() {
        let mut cfg = config(SAMPLE);
        assert!(cfg.remove_provider("local").unwrap());
        assert!(!cfg.remove_provider("local").unwrap());
        assert_eq!(cfg.providers().len(), 1);
        assert_eq!(cfg.root["$schema"], json!("https://opencode.ai/config.json"));
    }
}
