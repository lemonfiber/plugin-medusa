//! What this adapter says it is, and the kit that serves its contracts.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use lemonfiber_adapter_kit::{Kit, Readiness};
use lemonfiber_contract::adapter::{About, Release};
use lemonfiber_contract::capabilities::{indexer, library};

use crate::medusa::{Medusa, IMPORT_PATIENCE};
use crate::thetvdb::SignIn;

/// The variable of the adapter's environment holding how many minutes an import may wait
/// before it is reported stuck, `0` reporting none.
pub const IMPORT_STUCK_AFTER: &str = "IMPORT_STUCK_AFTER_MINUTES";

const MINUTE: u64 = 60;

/// The variable of the adapter's environment saying whether TheTVDB is asked for a series
/// Medusa does not hold: `off`, `on` under a key, or `on-with-pin` under a key and a
/// subscriber PIN.
pub const THETVDB_LOOKUP: &str = "THETVDB_LOOKUP";

/// The variable of the adapter's environment holding the operator's TheTVDB key.
pub const THETVDB_KEY: &str = "THETVDB_KEY";

/// The variable of the adapter's environment holding the operator's TheTVDB subscriber
/// PIN.
pub const THETVDB_PIN: &str = "THETVDB_PIN";

const LOOKUP_OFF: &str = "off";

const LOOKUP_ON: &str = "on";

const LOOKUP_ON_WITH_PIN: &str = "on-with-pin";

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

/// How long an import may wait before it is reported stuck, as `minutes` says: the
/// default where it says nothing, and never where it says `0`.
///
/// # Errors
///
/// Where `minutes` is not a whole number of minutes.
pub fn patience(minutes: Option<&str>) -> Result<Option<Duration>, String> {
    let Some(minutes) = minutes else {
        return Ok(Some(IMPORT_PATIENCE));
    };
    let minutes: u64 = minutes.trim().parse().map_err(|_| {
        format!("{IMPORT_STUCK_AFTER} is {minutes:?}, not a whole number of minutes")
    })?;
    Ok((minutes > 0).then(|| Duration::from_secs(minutes.saturating_mul(MINUTE))))
}

/// How TheTVDB is signed in to, as `lookup`, `key` and `pin` say, or nothing where the
/// lookup is off, which it is where `lookup` says nothing.
///
/// # Errors
///
/// Where `lookup` names no state the setting takes, or the lookup is on and the key, or
/// the PIN it is on with, is missing.
pub fn thetvdb(
    lookup: Option<&str>,
    key: Option<&str>,
    pin: Option<&str>,
) -> Result<Option<SignIn>, String> {
    let given = |value: Option<&str>, named: &str| {
        value
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_owned)
            .ok_or_else(|| format!("{THETVDB_LOOKUP} is on, and {named} holds nothing"))
    };
    match lookup.map_or(LOOKUP_OFF, str::trim) {
        LOOKUP_OFF => Ok(None),
        LOOKUP_ON => Ok(Some(SignIn {
            key: given(key, THETVDB_KEY)?,
            pin: None,
        })),
        LOOKUP_ON_WITH_PIN => Ok(Some(SignIn {
            key: given(key, THETVDB_KEY)?,
            pin: Some(given(pin, THETVDB_PIN)?),
        })),
        other => Err(format!(
            "{THETVDB_LOOKUP} is {other:?}, not {LOOKUP_OFF}, {LOOKUP_ON} or {LOOKUP_ON_WITH_PIN}"
        )),
    }
}

/// The contracts, served from `medusa` under `key`, ready while Medusa answers, and
/// answering the request gate under each key the core wrote for it in `configuration`.
#[must_use]
pub fn kit(key: String, medusa: Arc<Medusa>, configuration: impl Into<PathBuf>) -> Kit {
    let asked = Arc::clone(&medusa);
    let ready: Readiness = Arc::new(move || {
        let asked = Arc::clone(&asked);
        Box::pin(async move { asked.answers().await })
    });
    Kit::new(key, about())
        .ready_when(ready)
        .asked_by_the_gate(configuration)
        .serving(indexer::pull::served(Arc::clone(&medusa)))
        .serving(library::curate::served(medusa))
}

#[cfg(test)]
mod tests;
