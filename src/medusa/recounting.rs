//! Medusa telling the core what happened: an episode it imported and a grab or an import
//! that failed, read from its history newer than the cursor, the history row's id.

use std::collections::BTreeMap;
use std::sync::{Arc, Weak};

use async_trait::async_trait;
use lemonfiber_adapter_kit::recounting::{Entry, History, Recounter};
use lemonfiber_ports::service::{Event, Failure, Happened, Heard, Recounting};
use serde_json::Value;

use super::queue::{newest_history, DOWNLOADED, FAILED, HISTORY_UNREAD};
use super::wire::HistoryRow;
use super::{Medusa, PAGE};

/// Said where `events` is asked of a Medusa no recounter was handed.
const UNRECOUNTED: &str = "this adapter was started with nowhere to keep its cursor";

impl Medusa {
    /// Tell the core what happened through `recounter` from now on, where nothing was
    /// handed before.
    pub fn recounted_by(&self, recounter: Recounter) {
        let _ = self.recounter.set(recounter);
    }

    /// Every history row whose id follows `after`, oldest first, and the newest id
    /// read: rows are read newest first, a page at a time, until one older than the
    /// newest row at or before `after`.
    async fn history_after(&self, after: u64) -> Result<Followed, Failure> {
        let mut followed = Followed::default();
        let mut boundary = None;
        'pages: for page in 1.. {
            let rows: Vec<HistoryRow> = self
                .read(
                    &format!("/history?{}&page={page}", newest_history(PAGE)),
                    HISTORY_UNREAD,
                )
                .await?;
            let full = rows.len() == PAGE;
            for row in rows {
                followed.newest = followed.newest.max(row.id);
                if row.id > after {
                    followed.rows.push(row);
                    continue;
                }
                let at = instant(&row.action_date);
                match boundary {
                    None => boundary = Some(at),
                    Some(newest) if at < newest => break 'pages,
                    Some(_) => {}
                }
            }
            if !full {
                break;
            }
        }
        Ok(followed.in_order())
    }
}

#[async_trait]
impl Recounting for Medusa {
    async fn events(
        &self,
        address: Option<&str>,
        pin: Option<&str>,
        key: Option<&str>,
        every: Option<u32>,
    ) -> Result<Option<Heard>, Failure> {
        let recounter = self
            .recounter
            .get()
            .ok_or_else(|| self.endpoint.refused(UNRECOUNTED))?;
        recounter.events(address, pin, key, every).await
    }
}

/// Medusa's history, holding Medusa without keeping it alive.
pub struct Happenings(pub Weak<Medusa>);

impl Happenings {
    /// Medusa, while the adapter holds it.
    fn medusa(&self) -> Result<Arc<Medusa>, Failure> {
        self.0.upgrade().ok_or_else(|| Failure::Unavailable {
            service: super::SERVICE.to_owned(),
        })
    }
}

/// The history rows following a cursor, and the newest id read on the way.
#[derive(Default)]
struct Followed {
    rows: Vec<HistoryRow>,
    newest: u64,
}

impl Followed {
    fn in_order(mut self) -> Self {
        self.rows.sort_by_key(|row| row.id);
        self
    }
}

/// When Medusa wrote a history row, as the number it writes it as.
fn instant(at: &Value) -> u64 {
    match at {
        Value::Number(at) => at.as_u64().unwrap_or_default(),
        Value::String(at) => at.trim().parse().unwrap_or_default(),
        _ => 0,
    }
}

impl HistoryRow {
    fn happened(&self) -> Option<Happened> {
        match self.status {
            DOWNLOADED => Some(Happened::Imported),
            FAILED => Some(Happened::Failed),
            _ => None,
        }
    }

    fn event(&self, held: &BTreeMap<String, i64>) -> Option<Event> {
        Some(Event {
            event: self.happened()?,
            reference: self
                .show_slug
                .as_ref()
                .and_then(|slug| held.get(slug))
                .map(ToString::to_string),
            name: self.episode_title.clone(),
            reason: None,
            cleared: false,
        })
    }
}

#[async_trait]
impl History for Happenings {
    async fn newest(&self) -> Result<String, Failure> {
        Ok(self
            .medusa()?
            .history_after(u64::MAX)
            .await?
            .newest
            .to_string())
    }

    async fn after(&self, cursor: &str) -> Result<Option<Vec<Entry>>, Failure> {
        let Ok(after) = cursor.parse::<u64>() else {
            return Ok(None);
        };
        let medusa = self.medusa()?;
        let followed = medusa.history_after(after).await?;
        if followed.newest < after {
            return Ok(None);
        }
        let held = if followed.rows.iter().any(|row| row.happened().is_some()) {
            medusa.tvdb_by_slug().await?
        } else {
            BTreeMap::new()
        };
        Ok(Some(
            followed
                .rows
                .iter()
                .map(|row| Entry {
                    position: row.id.to_string(),
                    event: row.event(&held),
                })
                .collect(),
        ))
    }
}

#[cfg(test)]
mod tests;
