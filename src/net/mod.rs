//! Talking to the outside world: providers themselves, and the models.dev catalogue.

pub mod catalog;
pub mod probe;
pub mod registry;

use std::time::Duration;

use crate::domain::{Model, Provider};

/// Default budget for a single `/v1/models` call.
pub const DEFAULT_TIMEOUT: Duration = Duration::from_secs(10);

/// Ask a provider what it serves and turn the answer into models an agent can store.
///
/// `/v1/models` gives ids and nothing else; models.dev supplies the context and
/// output limits that opencode needs in order to offer a model at all.
pub fn discover_models(provider: &Provider, timeout: Duration, refresh_catalog: bool) -> Discovery {
    let Some(base_url) = provider.base_url.as_deref() else {
        return Discovery {
            probe: None,
            models: Vec::new(),
            catalog_error: None,
            unknown_to_catalog: Vec::new(),
        };
    };

    let probe = probe::probe(base_url, provider.api_key.as_deref(), provider.wire_api, timeout);

    let (catalog, catalog_error) = match catalog::Catalog::load(refresh_catalog) {
        Ok(catalog) => (Some(catalog), None),
        Err(err) => (catalog::Catalog::cached_only(), Some(err.to_string())),
    };

    let mut unknown_to_catalog = Vec::new();
    let models = probe
        .models
        .iter()
        .map(|id| {
            let facts = catalog.as_ref().and_then(|c| c.lookup(id));
            if facts.is_none() {
                unknown_to_catalog.push(id.clone());
            }
            Model {
                id: id.clone(),
                display_name: facts.and_then(|f| f.name.clone()),
                context_limit: facts.and_then(|f| f.context),
                output_limit: facts.and_then(|f| f.output),
            }
        })
        .collect();

    Discovery { probe: Some(probe), models, catalog_error, unknown_to_catalog }
}

/// What [`discover_models`] found.
pub struct Discovery {
    pub probe: Option<probe::Probe>,
    /// Models the provider serves, enriched with limits where models.dev knows them.
    pub models: Vec<Model>,
    /// Why the catalogue could not be refreshed, if it could not be.
    pub catalog_error: Option<String>,
    /// Model ids models.dev has never heard of, so limits had to be left blank.
    pub unknown_to_catalog: Vec<String>,
}
