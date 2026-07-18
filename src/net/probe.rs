//! Ask a provider what it serves, and whether it is up at all.

use std::time::{Duration, Instant};

use serde::Deserialize;

use crate::domain::WireApi;

/// Outcome of a single `GET {base_url}/models`.
#[derive(Debug, Clone)]
pub struct Probe {
    pub url: String,
    pub status: Option<u16>,
    pub latency: Duration,
    pub models: Vec<String>,
    pub error: Option<String>,
}

impl Probe {
    pub fn alive(&self) -> bool {
        self.error.is_none() && matches!(self.status, Some(code) if (200..300).contains(&code))
    }

    /// One-line verdict for listings.
    pub fn summary(&self) -> String {
        if let Some(err) = &self.error {
            return format!("down ({err})");
        }
        match self.status {
            Some(code) if (200..300).contains(&code) => format!(
                "up {}ms, {} model{}",
                self.latency.as_millis(),
                self.models.len(),
                if self.models.len() == 1 { "" } else { "s" }
            ),
            Some(401) | Some(403) => format!("reachable but rejected the key (HTTP {})", self.status.unwrap()),
            Some(code) => format!("HTTP {code}"),
            None => "no response".to_string(),
        }
    }
}

/// Both OpenAI and Anthropic return `{"data": [{"id": ...}]}`; some gateways
/// answer with a bare array instead.
#[derive(Deserialize)]
#[serde(untagged)]
enum ModelsResponse {
    Wrapped { data: Vec<ModelEntry> },
    Bare(Vec<ModelEntry>),
}

#[derive(Deserialize)]
struct ModelEntry {
    #[serde(alias = "name")]
    id: String,
}

impl ModelsResponse {
    fn into_ids(self) -> Vec<String> {
        let mut ids: Vec<String> = match self {
            ModelsResponse::Wrapped { data } => data,
            ModelsResponse::Bare(data) => data,
        }
        .into_iter()
        .map(|entry| entry.id)
        .collect();
        ids.sort();
        ids.dedup();
        ids
    }
}

/// `GET {base_url}/models`, timing the round trip and parsing the model list.
///
/// Never returns an error: an unreachable provider is a result to display, not
/// a failure of the command the user asked for.
pub fn probe(base_url: &str, api_key: Option<&str>, wire_api: Option<WireApi>, timeout: Duration) -> Probe {
    let url = models_url(base_url);
    let started = Instant::now();

    let agent: ureq::Agent = ureq::Agent::config_builder()
        .timeout_global(Some(timeout))
        .user_agent(concat!("confai/", env!("CARGO_PKG_VERSION")))
        .build()
        .into();

    let mut request = agent.get(&url);
    if let Some(key) = api_key.filter(|k| !k.is_empty()) {
        request = request.header("Authorization", &format!("Bearer {key}"));
        if wire_api == Some(WireApi::Anthropic) {
            request = request.header("x-api-key", key);
        }
    }
    if wire_api == Some(WireApi::Anthropic) {
        request = request.header("anthropic-version", "2023-06-01");
    }

    let (status, models, error) = match request.call() {
        Ok(mut response) => {
            let status = response.status().as_u16();
            let models = response
                .body_mut()
                .read_json::<ModelsResponse>()
                .map(ModelsResponse::into_ids)
                .unwrap_or_default();
            (Some(status), models, None)
        }
        Err(ureq::Error::StatusCode(code)) => (Some(code), Vec::new(), None),
        Err(err) => (None, Vec::new(), Some(terse(&err.to_string()))),
    };

    Probe { url, status, latency: started.elapsed(), models, error }
}

/// Append `/models` to a base URL without doubling or dropping separators.
fn models_url(base_url: &str) -> String {
    let trimmed = base_url.trim_end_matches('/');
    if trimmed.ends_with("/models") {
        trimmed.to_string()
    } else {
        format!("{trimmed}/models")
    }
}

/// Transport errors arrive as long chains; the first clause is the useful part.
fn terse(message: &str) -> String {
    message.split(&[':', ';'][..]).next().unwrap_or(message).trim().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn models_url_handles_every_base_url_shape() {
        assert_eq!(models_url("https://byesu.com/v1"), "https://byesu.com/v1/models");
        assert_eq!(models_url("https://byesu.com/v1/"), "https://byesu.com/v1/models");
        assert_eq!(models_url("https://byesu.com/v1/models"), "https://byesu.com/v1/models");
        assert_eq!(models_url("http://localhost:1337/v1"), "http://localhost:1337/v1/models");
    }

    #[test]
    fn parses_wrapped_and_bare_model_lists() {
        let wrapped: ModelsResponse =
            serde_json::from_str(r#"{"data":[{"id":"b"},{"id":"a"},{"id":"a"}]}"#).unwrap();
        assert_eq!(wrapped.into_ids(), vec!["a", "b"]);

        let bare: ModelsResponse = serde_json::from_str(r#"[{"id":"gpt-5.5"}]"#).unwrap();
        assert_eq!(bare.into_ids(), vec!["gpt-5.5"]);
    }

    #[test]
    fn alive_requires_a_2xx_and_no_transport_error() {
        let base = Probe {
            url: "u".into(),
            status: Some(200),
            latency: Duration::ZERO,
            models: vec![],
            error: None,
        };
        assert!(base.alive());
        assert!(!Probe { status: Some(401), ..base.clone() }.alive());
        assert!(!Probe { error: Some("dns".into()), ..base }.alive());
    }
}
