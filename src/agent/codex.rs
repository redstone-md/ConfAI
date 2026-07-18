//! OpenAI Codex CLI — `~/.codex/config.toml`.
//!
//! Edited through `toml_edit`, so comments survive. That matters here: parking a
//! spare endpoint on a commented-out `base_url` is a normal way to work, and a
//! re-serialising editor would delete it on the first write.

use std::path::PathBuf;

use anyhow::{Context, Result};
use toml_edit::{DocumentMut, Item, Table, Value};

use super::{validate_provider_id, Agent, AgentConfig, AgentInfo, Capabilities};
use crate::domain::{Provider, WireApi};
use crate::store;

/// Keys under `[model_providers.<id>]` that map onto [`Provider`] fields.
/// Everything else is carried in `extras`.
const MAPPED_KEYS: [&str; 4] = ["name", "base_url", "wire_api", "experimental_bearer_token"];

pub struct Codex {
    info: AgentInfo,
}

impl Codex {
    /// Honours `CODEX_HOME`, which is how Codex itself locates its config.
    pub fn discover() -> Option<Self> {
        let home = std::env::var_os("CODEX_HOME")
            .map(PathBuf::from)
            .or_else(|| dirs::home_dir().map(|h| h.join(".codex")))?;

        Some(Self {
            info: AgentInfo {
                id: "codex",
                name: "Codex",
                binaries: &["codex"],
                config_path: home.join("config.toml"),
                capabilities: Capabilities {
                    named_providers: true,
                    selectable_provider: true,
                    per_provider_models: false,
                    inline_api_key: true,
                },
            },
        })
    }
}

impl Agent for Codex {
    fn info(&self) -> &AgentInfo {
        &self.info
    }

    fn load(&self) -> Result<Box<dyn AgentConfig>> {
        let text = store::read_or_empty(&self.info.config_path)?;
        let doc = text
            .parse::<DocumentMut>()
            .with_context(|| format!("parsing {}", self.info.config_path.display()))?;
        Ok(Box::new(CodexConfig { info: self.info.clone(), doc }))
    }
}

pub struct CodexConfig {
    info: AgentInfo,
    doc: DocumentMut,
}

impl CodexConfig {
    fn providers_table(&self) -> Option<&Table> {
        self.doc.get("model_providers")?.as_table()
    }

    /// The `[model_providers]` table, created as implicit so it never renders as
    /// a bare `[model_providers]` header of its own.
    fn providers_table_mut(&mut self) -> Result<&mut Table> {
        let item = self.doc.entry("model_providers").or_insert_with(|| {
            let mut table = Table::new();
            table.set_implicit(true);
            Item::Table(table)
        });
        item.as_table_mut().context("`model_providers` is not a table")
    }

    fn read_provider(id: &str, table: &Table) -> Provider {
        let string_at = |key: &str| table.get(key).and_then(Item::as_str).map(str::to_owned);

        let extras = table
            .iter()
            .filter(|(key, _)| !MAPPED_KEYS.contains(key))
            .filter_map(|(key, item)| {
                item.as_value().map(|value| (key.to_string(), scalar_to_string(value)))
            })
            .collect();

        Provider {
            id: id.to_string(),
            display_name: string_at("name"),
            base_url: string_at("base_url"),
            api_key: string_at("experimental_bearer_token"),
            wire_api: string_at("wire_api").as_deref().and_then(WireApi::parse),
            models: Vec::new(),
            extras,
        }
    }
}

impl AgentConfig for CodexConfig {
    fn info(&self) -> &AgentInfo {
        &self.info
    }

    fn providers(&self) -> Vec<Provider> {
        let Some(table) = self.providers_table() else {
            return Vec::new();
        };
        table
            .iter()
            .filter_map(|(id, item)| item.as_table().map(|t| Self::read_provider(id, t)))
            .collect()
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

        let table = self.providers_table_mut()?;
        let entry = table
            .entry(&merged.id)
            .or_insert_with(|| Item::Table(Table::new()))
            .as_table_mut()
            .with_context(|| format!("`model_providers.{}` is not a table", merged.id))?;

        set_or_clear(entry, "name", merged.display_name.clone());
        set_or_clear(entry, "base_url", merged.base_url.clone());
        set_or_clear(entry, "wire_api", merged.wire_api.map(|w| w.to_string()));
        set_or_clear(entry, "experimental_bearer_token", merged.api_key.clone());
        for (key, raw) in &merged.extras {
            entry[key.as_str()] = Item::Value(parse_scalar(raw));
        }
        Ok(())
    }

    fn remove_provider(&mut self, id: &str) -> Result<bool> {
        let removed = self.providers_table_mut()?.remove(id).is_some();
        if self.active_provider().as_deref() == Some(id) {
            self.doc.remove("model_provider");
        }
        Ok(removed)
    }

    fn active_provider(&self) -> Option<String> {
        self.doc.get("model_provider")?.as_str().map(str::to_owned)
    }

    fn set_active_provider(&mut self, id: &str) -> Result<()> {
        validate_provider_id(id)?;
        self.doc["model_provider"] = toml_edit::value(id);
        Ok(())
    }

    fn model(&self) -> Option<String> {
        self.doc.get("model")?.as_str().map(str::to_owned)
    }

    fn set_model(&mut self, model: &str) -> Result<()> {
        self.doc["model"] = toml_edit::value(model);
        Ok(())
    }

    fn render(&self) -> String {
        self.doc.to_string()
    }
}

/// Write `value`, or drop the key entirely when there is nothing to write.
fn set_or_clear(table: &mut Table, key: &str, value: Option<String>) {
    match value {
        Some(text) => table[key] = toml_edit::value(text),
        None => {
            table.remove(key);
        }
    }
}

/// Render a TOML scalar as the text ConfAI keeps in `extras`.
fn scalar_to_string(value: &Value) -> String {
    match value {
        Value::String(s) => s.value().clone(),
        other => other.to_string().trim().to_string(),
    }
}

/// Turn `extras` text back into the narrowest TOML type it fits.
fn parse_scalar(raw: &str) -> Value {
    if let Ok(flag) = raw.parse::<bool>() {
        return flag.into();
    }
    if let Ok(number) = raw.parse::<i64>() {
        return number.into();
    }
    raw.into()
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r#"model = "gpt-5.6-terra"
# model_provider = "primary"

[model_providers.primary]
name = "CodexLB"
wire_api = "responses"
base_url = "http://192.0.2.10:8080/v1"
experimental_bearer_token = "secret"
supports_websockets = false
# base_url = "http://192.0.2.11:2455/v1"

[features]
multi_agent = true
"#;

    fn config(text: &str) -> CodexConfig {
        CodexConfig {
            info: Codex::discover().unwrap().info,
            doc: text.parse::<DocumentMut>().unwrap(),
        }
    }

    #[test]
    fn reads_providers_with_their_unmapped_keys() {
        let providers = config(SAMPLE).providers();
        assert_eq!(providers.len(), 1);

        let p = &providers[0];
        assert_eq!(p.id, "primary");
        assert_eq!(p.display_name.as_deref(), Some("CodexLB"));
        assert_eq!(p.wire_api, Some(WireApi::Responses));
        assert_eq!(p.api_key.as_deref(), Some("secret"));
        assert_eq!(p.extras.get("supports_websockets").map(String::as_str), Some("false"));
    }

    #[test]
    fn untouched_config_renders_byte_identical() {
        assert_eq!(config(SAMPLE).render(), SAMPLE);
    }

    #[test]
    fn editing_a_provider_keeps_comments_and_neighbours() {
        let mut cfg = config(SAMPLE);
        let mut patch = Provider::new("primary");
        patch.base_url = Some("https://byesu.com/v1".into());
        cfg.upsert_provider(&patch).unwrap();

        let out = cfg.render();
        assert!(out.contains(r#"base_url = "https://byesu.com/v1""#));
        assert!(out.contains(r#"# base_url = "http://192.0.2.11:2455/v1""#), "lost the parked url:\n{out}");
        assert!(out.contains(r#"# model_provider = "primary""#));
        assert!(out.contains("[features]"));
        // Unset fields on the patch must not wipe what is already there.
        assert!(out.contains(r#"experimental_bearer_token = "secret""#));
    }

    #[test]
    fn adding_a_provider_creates_only_its_own_table() {
        let mut cfg = config(SAMPLE);
        let mut provider = Provider::new("byesu");
        provider.display_name = Some("Byesu".into());
        provider.base_url = Some("https://byesu.com/v1".into());
        provider.wire_api = Some(WireApi::Chat);
        cfg.upsert_provider(&provider).unwrap();

        let out = cfg.render();
        assert!(out.contains("[model_providers.byesu]"));
        assert!(!out.contains("\n[model_providers]\n"), "emitted a bare parent table:\n{out}");
        assert_eq!(cfg.providers().len(), 2);
    }

    #[test]
    fn switching_provider_writes_a_root_key_before_any_table() {
        let mut cfg = config(SAMPLE);
        cfg.set_active_provider("primary").unwrap();

        let out = cfg.render();
        let key = out.find("model_provider =").expect("no model_provider key");
        let first_table = out.find('[').expect("no tables");
        assert!(key < first_table, "root key landed inside a table:\n{out}");
        assert_eq!(cfg.active_provider().as_deref(), Some("primary"));
    }

    #[test]
    fn removing_the_active_provider_clears_the_selection() {
        let mut cfg = config(SAMPLE);
        cfg.set_active_provider("primary").unwrap();
        assert!(cfg.remove_provider("primary").unwrap());

        assert!(cfg.active_provider().is_none());
        assert!(cfg.providers().is_empty());
        assert!(!cfg.remove_provider("primary").unwrap());
    }

    #[test]
    fn extras_round_trip_without_becoming_strings() {
        let mut cfg = config(SAMPLE);
        let mut patch = Provider::new("primary");
        patch.extras.insert("requires_openai_auth".into(), "true".into());
        patch.extras.insert("startup_timeout_sec".into(), "120".into());
        cfg.upsert_provider(&patch).unwrap();

        let out = cfg.render();
        assert!(out.contains("requires_openai_auth = true"));
        assert!(out.contains("startup_timeout_sec = 120"));
    }
}
