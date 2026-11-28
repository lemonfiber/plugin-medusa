//! What this adapter says it is, and the kit that serves its contracts.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use lemonfiber_adapter_kit::recounting::Recounter;
use lemonfiber_adapter_kit::{Kit, Readiness};
use lemonfiber_contract::adapter::{About, Release};
use lemonfiber_contract::capabilities::{indexer, library};
use lemonfiber_ports::narration::Narrator;

use crate::medusa::{Happenings, Medusa, IMPORT_PATIENCE};
use crate::thetvdb::SignIn;

/// The variable of the adapter's environment holding how many minutes an import may wait
/// before it is reported stuck, `0` reporting none.
pub const IMPORT_STUCK_AFTER: &str = "IMPORT_STUCK_AFTER_MINUTES";

const MINUTE: u64 = 60;

/// The variable of the adapter's environment saying where a series Medusa does not hold is
/// looked up: nowhere where it is `off`, TVmaze where it is `tvmaze`, and TheTVDB where it
/// is `on`, under a key, or `on-with-pin`, under a key and a subscriber PIN.
pub const THETVDB_LOOKUP: &str = "THETVDB_LOOKUP";

/// The variable of the adapter's environment holding the operator's TheTVDB key.
pub const THETVDB_KEY: &str = "THETVDB_KEY";

/// The variable of the adapter's environment holding the operator's TheTVDB subscriber
/// PIN.
pub const THETVDB_PIN: &str = "THETVDB_PIN";

const LOOKUP_OFF: &str = "off";

const LOOKUP_TVMAZE: &str = "tvmaze";

const LOOKUP_ON: &str = "on";

const LOOKUP_ON_WITH_PIN: &str = "on-with-pin";

/// Where Medusa answers, by its service id and the port it listens on.
pub const ADDRESS: &str = "http://medusa:8081";

/// The part of `upstream.json` holding the key Medusa answers its API to.
pub const API_KEY: &str = "api_key";

/// The upstream this adapter fronts, as its project names itself.
pub const UPSTREAM: &str = "Medusa";

include!(concat!(env!("OUT_DIR"), "/fronted.rs"));

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

/// Where a series Medusa does not hold is looked up.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Looking {
    /// Nowhere: a series Medusa does not hold is none.
    Off,
    /// TVmaze, which takes no key.
    TvMaze,
    /// TheTVDB, signed in to as this says.
    TheTvdb(SignIn),
}

/// Where a series Medusa does not hold is looked up, as `lookup`, `key` and `pin` say:
/// nowhere where `lookup` says nothing.
///
/// # Errors
///
/// Where `lookup` names no state the setting takes, or TheTVDB is looked up in and the
/// key, or the PIN it is looked up with, is missing.
pub fn looking(
    lookup: Option<&str>,
    key: Option<&str>,
    pin: Option<&str>,
) -> Result<Looking, String> {
    let given = |value: Option<&str>, named: &str| {
        value
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_owned)
            .ok_or_else(|| format!("{THETVDB_LOOKUP} is on, and {named} holds nothing"))
    };
    match lookup.map_or(LOOKUP_OFF, str::trim) {
        LOOKUP_OFF => Ok(Looking::Off),
        LOOKUP_TVMAZE => Ok(Looking::TvMaze),
        LOOKUP_ON => Ok(Looking::TheTvdb(SignIn {
            key: given(key, THETVDB_KEY)?,
            pin: None,
        })),
        LOOKUP_ON_WITH_PIN => Ok(Looking::TheTvdb(SignIn {
            key: given(key, THETVDB_KEY)?,
            pin: Some(given(pin, THETVDB_PIN)?),
        })),
        other => Err(format!(
            "{THETVDB_LOOKUP} is {other:?}, not {LOOKUP_OFF}, {LOOKUP_TVMAZE}, {LOOKUP_ON} or {LOOKUP_ON_WITH_PIN}"
        )),
    }
}

/// The contracts, served from `medusa` under `key`, ready while Medusa answers, and
/// answering the request gate under each key the core wrote for it in `configuration`.
#[must_use]
pub fn kit(key: String, medusa: Arc<Medusa>, configuration: impl Into<PathBuf>) -> Kit {
    serving(Kit::new(key, about()), medusa, configuration.into())
}

/// The contracts as `kit` serves them, telling the core what happened with the cursor
/// kept beneath `configuration` and what it says said to `narrator`.
#[must_use]
pub fn recounting_kit(
    key: String,
    medusa: Arc<Medusa>,
    configuration: impl Into<PathBuf>,
    narrator: Arc<dyn Narrator>,
) -> Kit {
    let configuration = configuration.into();
    let recounter = Recounter::new(
        crate::medusa::SERVICE,
        &configuration,
        Arc::new(Happenings(Arc::downgrade(&medusa))),
        narrator,
    );
    medusa.recounted_by(recounter.clone());
    serving(
        Kit::new(key, about()).recounted_by(recounter),
        medusa,
        configuration,
    )
}

/// `kit` ready while Medusa answers, answering the request gate under each key the core
/// wrote for it in `configuration`, and serving the contracts from `medusa`.
fn serving(kit: Kit, medusa: Arc<Medusa>, configuration: PathBuf) -> Kit {
    let asked = Arc::clone(&medusa);
    let ready: Readiness = Arc::new(move || {
        let asked = Arc::clone(&asked);
        Box::pin(async move { asked.answers().await })
    });
    kit.ready_when(ready)
        .asked_by_the_gate(configuration)
        .serving(indexer::pull::served(Arc::clone(&medusa)))
        .serving(library::curate::served(medusa))
}

#[cfg(test)]
mod tests;
