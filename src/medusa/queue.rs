use std::collections::{BTreeMap, BTreeSet};

use async_trait::async_trait;
use lemonfiber_adapter_kit::endpoint::form_encoded;
use lemonfiber_ports::service::stage::Stage;
use lemonfiber_ports::service::{Failure, Queue, Queued, Queues};

use super::wire::HistoryRow;
use super::Medusa;

pub(crate) const SNATCHED: [i64; 3] = [2, 9, 12];

pub(crate) const DOWNLOADED: i64 = 4;

pub(crate) const FAILED: i64 = 11;

const SETTLING: [i64; 3] = [DOWNLOADED, 6, FAILED];

const PAUSED: i64 = 1;

const DOWNLOADING: i64 = 1 << 1;

const FETCHED: i64 = 1 << 2;

const SEEDED: i64 = 1 << 3;

const FAILED_IN_CLIENT: i64 = 1 << 4;

const ABORTED: i64 = 1 << 5;

const EXTRACTING: i64 = 1 << 6;

const COMPLETED: i64 = 1 << 7;

const POSTPROCESSED: i64 = 1 << 8;

const SEED_ACTION: i64 = 1 << 9;

const REMOVED: i64 = 1 << 10;

const OVER: i64 = POSTPROCESSED | ABORTED | REMOVED | SEED_ACTION;

const HISTORY_DEPTH: usize = 1000;

pub(crate) fn newest_history(depth: usize) -> String {
    format!(
        "limit={depth}&{}",
        form_encoded(&[("sort", r#"[{"field":"date","type":"desc"}]"#)])
    )
}

type Episode = (String, u32, u32);

pub(crate) struct InFlight {
    pub(crate) slug: String,
    pub(crate) season: u32,
    pub(crate) episode: u32,
    pub(crate) show_title: String,
    pub(crate) title: String,
    pub(crate) status: &'static str,
    pub(crate) state: &'static str,
    pub(crate) stage: Stage,
    pub(crate) download_id: Option<String>,
    pub(crate) grabs: u32,
}

impl InFlight {
    pub(crate) fn stuck(&self) -> bool {
        self.status != "ok"
    }
}

fn reading(bits: Option<i64>) -> (&'static str, &'static str, Stage) {
    let bits = bits.unwrap_or_default();
    if bits & FAILED_IN_CLIENT != 0 {
        ("error", "failedPending", Stage::Downloading)
    } else if bits & (COMPLETED | FETCHED | SEEDED) != 0 {
        ("ok", "importPending", Stage::Downloaded)
    } else if bits & PAUSED != 0 {
        ("warning", "downloading", Stage::Downloading)
    } else if bits & (DOWNLOADING | EXTRACTING) != 0 {
        ("ok", "downloading", Stage::Downloading)
    } else {
        ("ok", "queued", Stage::Grabbed)
    }
}

pub(crate) fn in_flight(rows: Vec<HistoryRow>) -> Vec<InFlight> {
    let mut settled: BTreeSet<Episode> = BTreeSet::new();
    let mut at: BTreeMap<Episode, usize> = BTreeMap::new();
    let mut flights: Vec<InFlight> = Vec::new();
    for row in rows {
        let key = (
            row.show_slug.clone().unwrap_or_default(),
            row.season,
            row.episode,
        );
        if SETTLING.contains(&row.status) {
            settled.insert(key);
            continue;
        }
        if !SNATCHED.contains(&row.status) || settled.contains(&key) {
            continue;
        }
        if let Some(flight) = at.get(&key).and_then(|index| flights.get_mut(*index)) {
            flight.grabs += 1;
            continue;
        }
        let bits = row
            .client_status
            .as_ref()
            .map(|client| client.status.iter().fold(0, |all, one| all | one));
        if bits.is_some_and(|bits| bits & OVER != 0) {
            settled.insert(key);
            continue;
        }
        let (status, said, reached) = reading(bits);
        at.insert(key, flights.len());
        flights.push(InFlight {
            slug: row.show_slug.unwrap_or_default(),
            season: row.season,
            episode: row.episode,
            show_title: row.show_title,
            title: row.resource,
            status,
            state: said,
            stage: reached,
            download_id: row.info_hash.filter(|hash| !hash.is_empty()),
            grabs: 1,
        });
    }
    flights
}

impl Medusa {
    pub(crate) async fn in_flight(&self) -> Result<Vec<InFlight>, Failure> {
        let rows: Vec<HistoryRow> = self
            .read(
                &format!("/history?{}", newest_history(HISTORY_DEPTH)),
                "Medusa's history could not be read",
            )
            .await?;
        Ok(in_flight(rows))
    }
}

#[async_trait]
impl Queues for Medusa {
    async fn queue(&self) -> Result<Queue, Failure> {
        let flights = self.in_flight().await?;
        Ok(Queue {
            total: flights.len(),
            items: flights
                .into_iter()
                .map(|flight| Queued {
                    title: flight.title,
                    status: flight.status.to_owned(),
                    state: flight.state.to_owned(),
                    message: None,
                    download_id: flight.download_id,
                    grabs: flight.grabs,
                })
                .collect(),
        })
    }
}

#[cfg(test)]
mod tests;
