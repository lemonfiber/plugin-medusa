use async_trait::async_trait;
use lemonfiber_ports::service::{Carried, Carrying, Failure, Record};
use serde_json::{json, Value};

use super::wire::Series;
use super::Medusa;

const TVDB_AT: [&str; 3] = ["/id/tvdb", "/externals/tvdb", "/tvdbId"];

const PAUSED_AT: &str = "/config/paused";

const MONITORED_AT: &str = "/monitored";

const SEASON_FOLDERS_AT: [&str; 2] = ["/config/seasonFolders", "/seasonFolder"];

fn tvdb(record: &Value) -> Option<i64> {
    TVDB_AT
        .iter()
        .find_map(|at| record.pointer(at).and_then(Value::as_i64))
        .filter(|tvdb| *tvdb > 0)
}

fn paused(record: &Value) -> bool {
    record
        .pointer(PAUSED_AT)
        .and_then(Value::as_bool)
        .or_else(|| {
            record
                .pointer(MONITORED_AT)
                .and_then(Value::as_bool)
                .map(|monitored| !monitored)
        })
        .unwrap_or(false)
}

fn season_folders(record: &Value) -> bool {
    SEASON_FOLDERS_AT
        .iter()
        .find_map(|at| record.pointer(at).and_then(Value::as_bool))
        .unwrap_or(true)
}

#[async_trait]
impl Carrying for Medusa {
    async fn records(&self, kind: Record) -> Result<Vec<Carried>, Failure> {
        Self::holding(kind)?;
        let followed = self.followed().await?;
        let presets = self.presets().await?;
        Ok(followed
            .into_iter()
            .filter_map(|record| {
                let series: Series = serde_json::from_value(record.clone()).ok()?;
                let combined = series.config.qualities.combined();
                Some(Carried {
                    name: series.title,
                    profile: presets
                        .iter()
                        .find(|preset| preset.value == combined)
                        .map(|preset| preset.name.clone()),
                    folder: Some(series.config.root_dir).filter(|folder| !folder.is_empty()),
                    rest: record.to_string(),
                })
            })
            .filter(|carried| !carried.name.is_empty())
            .collect())
    }

    async fn carry(&self, kind: Record, item: &Carried) -> Result<(), Failure> {
        Self::holding(kind)?;
        let record: Value = serde_json::from_str(&item.rest).unwrap_or(Value::Null);
        let Some(tvdb) = tvdb(&record) else {
            return Err(self.endpoint.refused(&format!(
                "{} carries no TheTVDB id to follow it by",
                item.name
            )));
        };
        let held = self.main().await?;
        let quality = match &item.profile {
            Some(wanted) => self
                .presets()
                .await?
                .into_iter()
                .find(|preset| &preset.name == wanted)
                .map(|preset| preset.value)
                .ok_or_else(|| {
                    self.endpoint
                        .refused(&format!("no quality preset called {wanted}"))
                })?,
            None => held.show_defaults.quality,
        };
        let Some(folder) = item.folder.clone().or_else(|| held.default_folder()) else {
            return Err(self
                .endpoint
                .refused("series has no root folder configured yet"));
        };
        self.follow(
            tvdb,
            &folder,
            quality,
            [
                ("paused", json!(paused(&record))),
                ("seasonFolders", json!(season_folders(&record))),
            ],
        )
        .await
    }
}

#[cfg(test)]
mod tests;
