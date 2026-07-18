//! Shared JSON plumbing for the agents that store config as JSON.
//!
//! `serde_json` is built with `preserve_order`, so keys keep the order the user
//! wrote them in and edits stay local instead of shuffling the whole file.

use std::path::Path;

use anyhow::{bail, Context, Result};
use serde_json::{Map, Value};

use crate::store;

/// Parse a config file, treating an absent or blank file as an empty object.
pub fn load(path: &Path) -> Result<Value> {
    let text = store::read_or_empty(path)?;
    if text.trim().is_empty() {
        return Ok(Value::Object(Map::new()));
    }
    if has_comment(&text) {
        bail!(
            "{} contains JSON comments, which ConfAI cannot rewrite without dropping them; \
             remove them or edit this file by hand",
            path.display()
        );
    }
    serde_json::from_str(&text).with_context(|| format!("parsing {}", path.display()))
}

/// Serialise with a trailing newline, matching what editors and formatters leave behind.
pub fn render(value: &Value) -> String {
    let mut text = serde_json::to_string_pretty(value).unwrap_or_else(|_| "{}".to_string());
    text.push('\n');
    text
}

/// Borrow `root[key]` as an object, creating it when missing.
pub fn object_mut<'a>(root: &'a mut Value, key: &str) -> Result<&'a mut Map<String, Value>> {
    let holder = root
        .as_object_mut()
        .context("config root is not a JSON object")?
        .entry(key.to_string())
        .or_insert_with(|| Value::Object(Map::new()));

    holder
        .as_object_mut()
        .with_context(|| format!("`{key}` is not a JSON object"))
}

/// Borrow `root[key]` as an object without creating it.
pub fn object<'a>(root: &'a Value, key: &str) -> Option<&'a Map<String, Value>> {
    root.get(key)?.as_object()
}

/// Read a nested string, e.g. `string_at(provider, ["options", "baseURL"])`.
pub fn string_at(value: &Value, path: &[&str]) -> Option<String> {
    let mut cursor = value;
    for step in path {
        cursor = cursor.get(step)?;
    }
    cursor.as_str().map(str::to_owned)
}

/// Set `map[key]` to `value`, or remove the key when `value` is `None`.
pub fn set_or_clear(map: &mut Map<String, Value>, key: &str, value: Option<Value>) {
    match value {
        Some(value) => {
            map.insert(key.to_string(), value);
        }
        None => {
            map.remove(key);
        }
    }
}

/// Detect `//` and `/* */` outside of string literals.
///
/// Rewriting a commented file with `serde_json` would silently delete the
/// comments, so ConfAI refuses instead.
fn has_comment(text: &str) -> bool {
    let bytes = text.as_bytes();
    let mut in_string = false;
    let mut escaped = false;

    for i in 0..bytes.len() {
        let byte = bytes[i];
        if in_string {
            match byte {
                _ if escaped => escaped = false,
                b'\\' => escaped = true,
                b'"' => in_string = false,
                _ => {}
            }
            continue;
        }
        match byte {
            b'"' => in_string = true,
            b'/' if matches!(bytes.get(i + 1), Some(b'/') | Some(b'*')) => return true,
            _ => {}
        }
    }
    false
}

/// Convenience wrapper used by every JSON backend's `save`.
pub fn write(path: &Path, value: &Value) -> Result<()> {
    store::write_atomic(path, &render(value))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn comments_are_detected_only_outside_strings() {
        assert!(has_comment("{\n  // note\n}"));
        assert!(has_comment("{ /* note */ }"));
        assert!(!has_comment(r#"{"url": "https://byesu.com/v1"}"#));
        assert!(!has_comment(r#"{"escaped": "a\"//b"}"#));
    }

    #[test]
    fn key_order_survives_a_load_render_round_trip() {
        let text = "{\n  \"zeta\": 1,\n  \"alpha\": 2\n}";
        let value: Value = serde_json::from_str(text).unwrap();
        let out = render(&value);
        assert!(out.find("zeta").unwrap() < out.find("alpha").unwrap(), "{out}");
        assert!(out.ends_with('\n'));
    }

    #[test]
    fn nested_strings_resolve_through_missing_levels() {
        let value = json!({ "options": { "baseURL": "https://byesu.com/v1" } });
        assert_eq!(string_at(&value, &["options", "baseURL"]).as_deref(), Some("https://byesu.com/v1"));
        assert!(string_at(&value, &["options", "apiKey"]).is_none());
        assert!(string_at(&value, &["missing", "baseURL"]).is_none());
    }

    #[test]
    fn set_or_clear_removes_on_none() {
        let mut map = Map::new();
        set_or_clear(&mut map, "a", Some(json!(1)));
        assert_eq!(map.get("a"), Some(&json!(1)));
        set_or_clear(&mut map, "a", None);
        assert!(map.is_empty());
    }
}
