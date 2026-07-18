//! opencode's credential file, `~/.local/share/opencode/auth.json`.
//!
//! Keys do not live in `opencode.json` — `opencode auth login` writes them here,
//! keyed by provider id, as `{"type": "api", "key": "..."}`. OAuth logins land in
//! the same file with refresh and access tokens instead. ConfAI reads keys from
//! here so a health check is made with the credential opencode would actually
//! use, and edits entries in place so an OAuth session is never trampled.

use std::path::PathBuf;

use anyhow::{Context, Result};
use serde_json::{json, Map, Value};

use crate::agent::json;

/// How a provider authenticates, as recorded in `auth.json`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Method {
    /// A plain API key ConfAI can read and replace.
    Api,
    /// An OAuth session. ConfAI shows it but will not edit it.
    OAuth,
    /// Something newer than this build knows about.
    Other,
}

impl Method {
    fn parse(raw: &str) -> Self {
        match raw {
            "api" => Method::Api,
            "oauth" => Method::OAuth,
            _ => Method::Other,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Method::Api => "api",
            Method::OAuth => "oauth",
            Method::Other => "other",
        }
    }

    /// Whether replacing this entry with an API key would destroy a session.
    pub fn is_editable(self) -> bool {
        self == Method::Api
    }
}

/// The credential file, loaded for editing.
#[derive(Debug)]
pub struct AuthStore {
    path: PathBuf,
    entries: Map<String, Value>,
    dirty: bool,
}

impl AuthStore {
    pub fn load() -> Result<Self> {
        let path = auth_path()?;
        let root = json::load(&path)?;
        let entries = match root {
            Value::Object(map) => map,
            _ => Map::new(),
        };
        Ok(Self { path, entries, dirty: false })
    }

    /// An empty store that writes nowhere, for tests and for the case where the
    /// home directory cannot be located.
    pub fn empty(path: PathBuf) -> Self {
        Self { path, entries: Map::new(), dirty: false }
    }

    pub fn path(&self) -> &PathBuf {
        &self.path
    }

    pub fn method(&self, provider_id: &str) -> Option<Method> {
        let entry = self.entries.get(provider_id)?;
        Some(Method::parse(entry.get("type").and_then(Value::as_str).unwrap_or_default()))
    }

    /// The API key for `provider_id`, if it is stored as one.
    ///
    /// OAuth access tokens are deliberately not returned: they expire, opencode
    /// refreshes them, and copying one into a config would create a credential
    /// that silently goes stale.
    pub fn key(&self, provider_id: &str) -> Option<String> {
        if self.method(provider_id)? != Method::Api {
            return None;
        }
        self.entries
            .get(provider_id)?
            .get("key")?
            .as_str()
            .map(str::to_owned)
    }

    /// Store `key` for `provider_id`.
    ///
    /// Refuses to overwrite an OAuth session, because replacing one with a bare
    /// key logs the user out of a provider they never asked to be logged out of.
    pub fn set_key(&mut self, provider_id: &str, key: &str) -> Result<()> {
        if let Some(method) = self.method(provider_id) {
            if !method.is_editable() {
                anyhow::bail!(
                    "opencode holds a {} session for {provider_id:?}; \
                     run `opencode auth logout {provider_id}` before setting a key",
                    method.as_str()
                );
            }
        }

        match self.entries.get_mut(provider_id).and_then(Value::as_object_mut) {
            // Edit in place so any field opencode added survives.
            Some(entry) => {
                entry.insert("key".into(), json!(key));
            }
            None => {
                self.entries
                    .insert(provider_id.to_string(), json!({ "type": "api", "key": key }));
            }
        }
        self.dirty = true;
        Ok(())
    }

    pub fn remove(&mut self, provider_id: &str) -> bool {
        let removed = self.entries.remove(provider_id).is_some();
        self.dirty |= removed;
        removed
    }

    /// Whether anything needs writing, so a read-only command never touches a
    /// file full of credentials.
    pub fn dirty(&self) -> bool {
        self.dirty
    }

    pub fn save(&self) -> Result<()> {
        if !self.dirty {
            return Ok(());
        }
        let value = Value::Object(self.entries.clone());
        json::write(&self.path, &value)
            .with_context(|| format!("writing {}", self.path.display()))
    }
}

/// opencode keeps data under `~/.local/share` on every platform, including
/// Windows, so the usual per-OS data directory would look in the wrong place.
pub fn auth_path() -> Result<PathBuf> {
    let base = match std::env::var_os("XDG_DATA_HOME") {
        Some(explicit) => PathBuf::from(explicit),
        None => dirs::home_dir()
            .context("locating the home directory")?
            .join(".local")
            .join("share"),
    };
    Ok(base.join("opencode").join("auth.json"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn store(entries: Value) -> AuthStore {
        let Value::Object(map) = entries else { panic!("test entries must be an object") };
        AuthStore { path: PathBuf::from("auth.json"), entries: map, dirty: false }
    }

    #[test]
    fn api_keys_are_readable() {
        let store = store(json!({ "vendor": { "type": "api", "key": "sk-live" } }));
        assert_eq!(store.method("vendor"), Some(Method::Api));
        assert_eq!(store.key("vendor").as_deref(), Some("sk-live"));
    }

    #[test]
    fn oauth_access_tokens_are_never_handed_out_as_keys() {
        let store = store(json!({
            "anthropic": { "type": "oauth", "access": "at-123", "refresh": "rt-123" }
        }));
        assert_eq!(store.method("anthropic"), Some(Method::OAuth));
        assert!(store.key("anthropic").is_none());
    }

    #[test]
    fn setting_a_key_refuses_to_end_an_oauth_session() {
        let mut store = store(json!({ "anthropic": { "type": "oauth", "refresh": "rt" } }));
        let err = store.set_key("anthropic", "sk-new").unwrap_err().to_string();

        assert!(err.contains("oauth") && err.contains("logout"), "{err}");
        assert!(!store.dirty(), "a refused write must not mark the store dirty");
        assert_eq!(store.method("anthropic"), Some(Method::OAuth));
    }

    #[test]
    fn setting_a_key_keeps_fields_opencode_added() {
        let mut store =
            store(json!({ "vendor": { "type": "api", "key": "old", "label": "keep me" } }));
        store.set_key("vendor", "new").unwrap();

        assert_eq!(store.key("vendor").as_deref(), Some("new"));
        assert_eq!(store.entries["vendor"]["label"], json!("keep me"));
        assert!(store.dirty());
    }

    #[test]
    fn a_new_provider_gets_a_complete_record() {
        let mut store = store(json!({}));
        store.set_key("byesu", "sk-1").unwrap();

        assert_eq!(store.entries["byesu"], json!({ "type": "api", "key": "sk-1" }));
    }

    #[test]
    fn an_unknown_method_is_treated_as_uneditable() {
        let store = store(json!({ "future": { "type": "passkey" } }));
        assert_eq!(store.method("future"), Some(Method::Other));
        assert!(!Method::Other.is_editable());
        assert!(store.key("future").is_none());
    }

    #[test]
    fn a_clean_store_is_never_written() {
        let store = store(json!({ "a": { "type": "api", "key": "k" } }));
        assert!(!store.dirty());
        // Would fail on the bogus path if it actually wrote.
        store.save().unwrap();
    }

    #[test]
    fn removing_reports_whether_anything_went() {
        let mut store = store(json!({ "a": { "type": "api", "key": "k" } }));
        assert!(store.remove("a"));
        assert!(store.dirty());
        assert!(!store.remove("a"));
    }
}
