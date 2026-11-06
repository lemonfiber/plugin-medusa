//! What this adapter says it is, and the kit that serves its contracts.

use std::sync::Arc;

use lemonfiber_adapter_kit::{Kit, Readiness};
use lemonfiber_contract::adapter::{About, Release};
use lemonfiber_contract::capabilities::{indexer, library};

use crate::medusa::Medusa;

/// Where Medusa answers, by its service id and the port it listens on.
pub const ADDRESS: &str = "http://medusa:8081";

/// The part of `upstream.json` holding the key Medusa answers its API to.
pub const API_KEY: &str = "api_key";

/// The upstream this adapter fronts, as its project names itself.
pub const UPSTREAM: &str = "Medusa";

/// The Medusa release this adapter's recordings were taken from.
pub const RELEASE: &str = "v1.0.27-ls297";

/// The digest of the image that release's recordings were taken from.
pub const DIGEST: &str = "sha256:aad89df58a047bab3521a695f0508f2e89fc9ba1f1196866dc8e9a810fc3ae5d";

/// What this adapter says it is: the contracts it speaks and the release it supports.
#[must_use]
pub fn about() -> About {
    About {
        speaks: vec![
            lemonfiber_contract::spoken(indexer::pull::CAPABILITY, indexer::pull::MAJOR),
            lemonfiber_contract::spoken(library::curate::CAPABILITY, library::curate::MAJOR),
        ],
        upstream: UPSTREAM.to_owned(),
        releases: vec![Release {
            version: RELEASE.to_owned(),
            digest: DIGEST.to_owned(),
        }],
    }
}

/// The contracts, served from `medusa` under `key`, ready while Medusa answers.
#[must_use]
pub fn kit(key: String, medusa: Arc<Medusa>) -> Kit {
    let asked = Arc::clone(&medusa);
    let ready: Readiness = Arc::new(move || {
        let asked = Arc::clone(&asked);
        Box::pin(async move { asked.answers().await })
    });
    Kit::new(key, about())
        .ready_when(ready)
        .serving(indexer::pull::served(Arc::clone(&medusa)))
        .serving(library::curate::served(medusa))
}

#[cfg(test)]
mod tests;
