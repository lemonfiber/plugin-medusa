use std::collections::{BTreeMap, BTreeSet};

use async_trait::async_trait;
use lemonfiber_ports::media::Kind;
use lemonfiber_ports::service::stage::{Outcome, HISTORY_HORIZON};
use lemonfiber_ports::service::{
    Failure, FoundItem, ItemPart, Pipeline, QueueItem, StuckItem, TraceEvent,
};
use serde_json::Value;

use super::queue::{newest_history, DOWNLOADED, FAILED, SNATCHED};
use super::wire::{HistoryRow, UNASKED};
use super::Medusa;

fn outcome(status: i64) -> Option<Outcome> {
    if SNATCHED.contains(&status) {
        Some(Outcome::Grabbed)
    } else if status == DOWNLOADED {
        Some(Outcome::Imported)
    } else if status == FAILED {
        Some(Outcome::DownloadFailed)
    } else {
        None
    }
}

fn said(at: &Value) -> String {
    match at {
        Value::String(at) => at.clone(),
        Value::Null => String::new(),
        other => other.to_string(),
    }
}

impl Medusa {
    async fn numbered(&self, slug: &str) -> Result<BTreeMap<(u32, u32), i64>, Failure> {
        Ok(self
            .episodes(slug, None)
            .await?
            .into_iter()
            .filter_map(|one| one.id.tvdb.map(|id| ((one.season, one.number), id)))
            .collect())
    }
}

#[async_trait]
impl Pipeline for Medusa {
    async fn library(&self, kind: Kind) -> Result<Vec<FoundItem>, Failure> {
        Self::filing(kind)?;
        Ok(self
            .series()
            .await?
            .into_iter()
            .filter_map(|one| {
                Some(FoundItem {
                    id: one.tvdb()?,
                    title: one.title,
                    year: one.year.start.filter(|year| *year > 0),
                    monitored: !one.config.paused,
                })
            })
            .collect())
    }

    async fn find_items(&self, kind: Kind, term: &str) -> Result<Vec<FoundItem>, Failure> {
        let needle = term.to_lowercase();
        Ok(self
            .library(kind)
            .await?
            .into_iter()
            .filter(|item| item.title.to_lowercase().contains(&needle))
            .collect())
    }

    async fn item_history(&self, kind: Kind, id: i64) -> Result<Vec<TraceEvent>, Failure> {
        Self::filing(kind)?;
        let Some(slug) = self.slug_of(id).await? else {
            return Ok(Vec::new());
        };
        let rows: Vec<HistoryRow> = self
            .read(
                &format!("/history/{slug}?{}", newest_history(HISTORY_HORIZON)),
                "Medusa's history could not be read",
            )
            .await?;
        let numbered = self.numbered(&slug).await?;
        Ok(rows
            .into_iter()
            .filter_map(|row| {
                outcome(row.status).map(|outcome| TraceEvent {
                    outcome,
                    at: said(&row.action_date),
                    part: numbered.get(&(row.season, row.episode)).copied(),
                })
            })
            .collect())
    }

    async fn item_queue(&self, kind: Kind, id: i64) -> Result<Vec<QueueItem>, Failure> {
        Self::filing(kind)?;
        let Some(slug) = self.slug_of(id).await? else {
            return Ok(Vec::new());
        };
        let flights: Vec<_> = self
            .in_flight()
            .await?
            .into_iter()
            .filter(|flight| flight.slug == slug)
            .collect();
        if flights.is_empty() {
            return Ok(Vec::new());
        }
        let numbered = self.numbered(&slug).await?;
        Ok(flights
            .into_iter()
            .map(|flight| QueueItem {
                part: numbered.get(&(flight.season, flight.episode)).copied(),
                stage: flight.stage,
                stuck: flight.stuck(),
            })
            .collect())
    }

    async fn item_parts(
        &self,
        kind: Kind,
        id: i64,
        season: Option<u32>,
    ) -> Result<Vec<ItemPart>, Failure> {
        Self::filing(kind)?;
        let Some(slug) = self.slug_of(id).await? else {
            return Ok(Vec::new());
        };
        Ok(self
            .episodes(&slug, season)
            .await?
            .into_iter()
            .filter_map(|one| {
                Some(ItemPart {
                    id: one.id.tvdb?,
                    season: one.season,
                    number: one.number,
                    monitored: !UNASKED.contains(&one.status.as_str()),
                    has_file: !one.file.location.is_empty(),
                    title: one.title,
                })
            })
            .collect())
    }

    async fn stuck_items(&self, kind: Kind) -> Result<Vec<StuckItem>, Failure> {
        Self::filing(kind)?;
        let mut seen = BTreeSet::new();
        Ok(self
            .in_flight()
            .await?
            .into_iter()
            .filter(|flight| flight.stuck() && !flight.show_title.is_empty())
            .filter(|flight| seen.insert(flight.show_title.clone()))
            .map(|flight| StuckItem {
                title: flight.show_title,
                stage: flight.stage,
            })
            .collect())
    }
}

#[cfg(test)]
mod tests;
