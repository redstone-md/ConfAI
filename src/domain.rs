//! Agent-neutral vocabulary.
//!
//! Every backend maps its own on-disk shape onto these types, so commands,
//! presets and the TUI are written once and work against all agents.

use std::collections::BTreeMap;
use std::fmt;

use serde::{Deserialize, Serialize};

/// How a provider expects to be spoken to over HTTP.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum WireApi {
    /// OpenAI `/v1/chat/completions`.
    Chat,
    /// OpenAI `/v1/responses`.
    Responses,
    /// Anthropic `/v1/messages`.
    Anthropic,
}

impl WireApi {
    pub const ALL: [WireApi; 3] = [WireApi::Chat, WireApi::Responses, WireApi::Anthropic];

    pub fn as_str(self) -> &'static str {
        match self {
            WireApi::Chat => "chat",
            WireApi::Responses => "responses",
            WireApi::Anthropic => "anthropic",
        }
    }

    pub fn parse(raw: &str) -> Option<Self> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "chat" | "chat_completions" | "openai" | "openai-compatible" => Some(WireApi::Chat),
            "responses" => Some(WireApi::Responses),
            "anthropic" | "messages" => Some(WireApi::Anthropic),
            _ => None,
        }
    }
}

impl fmt::Display for WireApi {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// A model exposed by a provider. Agents that do not enumerate models ignore this.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Model {
    pub id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub display_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub context_limit: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output_limit: Option<u64>,
}

impl Model {
    pub fn new(id: impl Into<String>) -> Self {
        Self { id: id.into(), display_name: None, context_limit: None, output_limit: None }
    }

    pub fn label(&self) -> &str {
        self.display_name.as_deref().unwrap_or(&self.id)
    }
}

/// An endpoint an agent can talk to.
///
/// Fields an agent has no slot for are dropped on write; fields it stores but
/// this type does not model survive untouched, because every backend edits the
/// original document in place rather than re-serialising it.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Provider {
    pub id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub display_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub base_url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub api_key: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub wire_api: Option<WireApi>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub models: Vec<Model>,
    /// Backend-specific scalars carried verbatim, e.g. `npm` for opencode or
    /// `requires_openai_auth` for codex.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub extras: BTreeMap<String, String>,
}

impl Provider {
    pub fn new(id: impl Into<String>) -> Self {
        Self { id: id.into(), ..Self::default() }
    }

    pub fn label(&self) -> &str {
        self.display_name.as_deref().unwrap_or(&self.id)
    }

    /// Host and port of [`Self::base_url`], for compact listings.
    pub fn host(&self) -> Option<&str> {
        let url = self.base_url.as_deref()?;
        let rest = url.split_once("://").map_or(url, |(_, rest)| rest);
        Some(rest.split('/').next().unwrap_or(rest))
    }

    /// Overlay `other` onto `self`, keeping existing values where `other` is empty.
    pub fn merge_from(&mut self, other: &Provider) {
        if other.display_name.is_some() {
            self.display_name = other.display_name.clone();
        }
        if other.base_url.is_some() {
            self.base_url = other.base_url.clone();
        }
        if other.api_key.is_some() {
            self.api_key = other.api_key.clone();
        }
        if other.wire_api.is_some() {
            self.wire_api = other.wire_api;
        }
        if !other.models.is_empty() {
            self.models = other.models.clone();
        }
        self.extras.extend(other.extras.iter().map(|(k, v)| (k.clone(), v.clone())));
    }
}

/// Compare base URLs ignoring a trailing slash, which agents write inconsistently.
pub fn same_endpoint(left: Option<&str>, right: Option<&str>) -> bool {
    match (left, right) {
        (Some(left), Some(right)) => left.trim_end_matches('/') == right.trim_end_matches('/'),
        _ => false,
    }
}

/// Redact a secret for display: keep enough to recognise it, hide the rest.
pub fn mask(secret: &str) -> String {
    let visible = 4;
    if secret.chars().count() <= visible * 2 {
        return "*".repeat(secret.chars().count().max(3));
    }
    let head: String = secret.chars().take(visible).collect();
    let tail: String = secret.chars().rev().take(visible).collect::<Vec<_>>().into_iter().rev().collect();
    format!("{head}…{tail}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn host_strips_scheme_and_path() {
        let mut p = Provider::new("x");
        p.base_url = Some("https://byesu.com/v1".into());
        assert_eq!(p.host(), Some("byesu.com"));
        p.base_url = Some("http://192.0.2.10:8080/v1".into());
        assert_eq!(p.host(), Some("192.0.2.10:8080"));
    }

    #[test]
    fn merge_keeps_existing_when_source_empty() {
        let mut base = Provider::new("a");
        base.base_url = Some("https://old/v1".into());
        base.api_key = Some("k".into());

        let mut patch = Provider::new("a");
        patch.base_url = Some("https://new/v1".into());
        base.merge_from(&patch);

        assert_eq!(base.base_url.as_deref(), Some("https://new/v1"));
        assert_eq!(base.api_key.as_deref(), Some("k"));
    }

    #[test]
    fn mask_hides_the_middle() {
        assert_eq!(mask("sk-abcdefghijkl"), "sk-a…ijkl");
        assert_eq!(mask("short"), "*****");
    }
}
