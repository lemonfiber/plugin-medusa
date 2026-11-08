use std::collections::{BTreeMap, BTreeSet};

use async_trait::async_trait;
use lemonfiber_ports::http::Method;
use lemonfiber_ports::media::Kind;
use lemonfiber_ports::service::stage::Stage;
use lemonfiber_ports::service::{
    CuratedSeason, CuratedTitle, DownloadedEpisode, Downloading, Failure, Fulfilling, Grabbed,
    TitleRequest,
};
use serde::Deserialize;
use serde_json::{json, Map, Value};

use super::series::{BACKLOG, SERIES};
use super::wire::{Part, Series, UNASKED, WANTED};
use super::{unsupported, Medusa};

const SKIPPED: i64 = 5;

const UNAIRED: &str = "Unaired";

const WANTED_NAME: &str = "Wanted";

const SKIPPED_NAME: &str = "Skipped";

const REMOVING_FILES: &str = "remove-files=true";

const SAVED: u64 = 200;

const ARRIVAL_ROUNDS: usize = 30;

const BEING_ADDED: &str = "isBeingAdded";

const NO_UNQUEUE: &str = "Medusa cannot take a download out of the client holding it";

#[derive(Deserialize)]
struct QueueStatus {
    #[serde(default)]
    action: String,
    #[serde(default)]
    active: bool,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Adding {
    #[serde(default)]
    show_queue_status: Vec<QueueStatus>,
}

impl Adding {
    fn done(&self) -> bool {
        !self
            .show_queue_status
            .iter()
            .any(|step| step.action == BEING_ADDED && step.active)
    }
}

fn monitored(part: &Part, later: &str) -> bool {
    !UNASKED.contains(&part.status.as_str()) && (part.status != UNAIRED || later == WANTED_NAME)
}

fn curated(series: &Series, reference: i64, parts: &[Part]) -> CuratedTitle {
    let later = series.config.default_episode_status.as_str();
    let mut seasons: BTreeMap<u32, CuratedSeason> = BTreeMap::new();
    for part in parts {
        let season = seasons.entry(part.season).or_insert(CuratedSeason {
            number: part.season,
            monitored: false,
            files: 0,
            episodes: 0,
        });
        season.episodes += 1;
        season.files += u32::from(!part.file.location.is_empty());
        season.monitored |= monitored(part, later);
    }
    CuratedTitle {
        reference,
        id: Some(reference),
        title: series.title.clone(),
        year: series.year.start.map(u32::from),
        monitored: !series.config.paused,
        files: seasons.values().map(|season| season.files).sum(),
        seasons: seasons.into_values().collect(),
    }
}

fn wanted(parts: &[Part], seasons: &[u32]) -> Map<String, Value> {
    parts
        .iter()
        .filter(|part| seasons.contains(&part.season))
        .filter(|part| UNASKED.contains(&part.status.as_str()) || part.status == UNAIRED)
        .map(|part| (part.slug.clone(), json!({ "status": WANTED })))
        .collect()
}

impl Medusa {
    async fn followed_as(&self, tvdb: i64) -> Result<Option<Series>, Failure> {
        Ok(self
            .series()
            .await?
            .into_iter()
            .find(|one| one.tvdb() == Some(tvdb)))
    }

    async fn held_as(&self, tvdb: i64) -> Result<Option<CuratedTitle>, Failure> {
        if tvdb <= 0 {
            return Ok(None);
        }
        match self.followed_as(tvdb).await? {
            Some(series) => Ok(Some(self.curated_from(&series, tvdb).await?)),
            None => Ok(None),
        }
    }

    async fn held_series(&self, tvdb: i64) -> Result<Series, Failure> {
        self.followed_as(tvdb).await?.ok_or_else(|| {
            self.endpoint
                .refused(&format!("Medusa holds no series with TheTVDB id {tvdb}"))
        })
    }

    async fn curated_from(&self, series: &Series, reference: i64) -> Result<CuratedTitle, Failure> {
        let parts = self.episodes(&series.id.slug, None).await?;
        Ok(curated(series, reference, &parts))
    }

    async fn refuse_what_it_lacks(&self, request: &TitleRequest) -> Result<(), Failure> {
        let folder = request.root_folder.trim_end_matches('/');
        if !self
            .main()
            .await?
            .folders()
            .iter()
            .any(|held| held.trim_end_matches('/') == folder)
        {
            return Err(self.endpoint.refused(&format!(
                "Medusa holds no root folder {}",
                request.root_folder
            )));
        }
        if !self
            .presets()
            .await?
            .iter()
            .any(|preset| preset.value == request.quality_profile)
        {
            return Err(self.endpoint.refused(&format!(
                "Medusa holds no quality profile {}",
                request.quality_profile
            )));
        }
        Ok(())
    }

    async fn arrived(&self, tvdb: i64) -> Result<Series, Failure> {
        for _ in 0..ARRIVAL_ROUNDS {
            tokio::time::sleep(self.pause).await;
            if let Some(series) = self.followed_as(tvdb).await? {
                let adding: Adding = self
                    .read(
                        &format!("{SERIES}/{}?detailed=true", series.id.slug),
                        "Medusa did not say whether it had finished adding the series",
                    )
                    .await?;
                if adding.done() {
                    return Ok(series);
                }
            }
        }
        Err(self.endpoint.refused(&format!(
            "Medusa had not finished adding the series with TheTVDB id {tvdb} in time"
        )))
    }

    async fn added(&self, request: &TitleRequest) -> Result<Series, Failure> {
        self.follow(
            request.reference,
            &request.root_folder,
            request.quality_profile,
            &[
                ("status", json!(SKIPPED)),
                ("seasonFolders", json!(true)),
                ("anime", json!(request.anime)),
            ],
        )
        .await?;
        let series = self.arrived(request.reference).await?;
        let later = if request.later_seasons {
            WANTED_NAME
        } else {
            SKIPPED_NAME
        };
        self.done(
            Method::Patch,
            &format!("{SERIES}/{}", series.id.slug),
            &json!({ "config": { "defaultEpisodeStatus": later } }),
        )
        .await?;
        Ok(series)
    }

    async fn watching(&self, series: &Series) -> Result<(), Failure> {
        if series.config.paused {
            self.done(
                Method::Patch,
                &format!("{SERIES}/{}", series.id.slug),
                &json!({ "config": { "paused": false } }),
            )
            .await?;
        }
        Ok(())
    }

    async fn want(&self, series: &Series, seasons: &[u32]) -> Result<(), Failure> {
        let parts = self.episodes(&series.id.slug, None).await?;
        let patch = wanted(&parts, seasons);
        if patch.is_empty() {
            return Ok(());
        }
        let response = self
            .ask(
                Method::Patch,
                &format!("{SERIES}/{}/episodes", series.id.slug),
                Some(&Value::Object(patch)),
            )
            .await?;
        let saved: BTreeMap<String, Value> = self
            .endpoint
            .decode(&response, "Medusa did not say which episodes it now wants")?;
        let refused: Vec<&str> = saved
            .iter()
            .filter(|(_, said)| said.get("status").and_then(Value::as_u64) != Some(SAVED))
            .map(|(episode, _)| episode.as_str())
            .collect();
        if refused.is_empty() {
            Ok(())
        } else {
            Err(self
                .endpoint
                .refused(&format!("Medusa would not want {}", refused.join(", "))))
        }
    }

    async fn backlog(&self, series: &Series) -> Result<(), Failure> {
        self.done(Method::Put, BACKLOG, &json!({ "showSlug": series.id.slug }))
            .await
    }
}

#[async_trait]
impl Fulfilling for Medusa {
    async fn catalogued(
        &self,
        kind: Kind,
        reference: i64,
    ) -> Result<Option<CuratedTitle>, Failure> {
        Self::filing(kind)?;
        if let Some(held) = self.held_as(reference).await? {
            return Ok(Some(held));
        }
        match &self.lookup {
            Some(lookup) if reference > 0 => lookup.series(reference).await,
            _ => Ok(None),
        }
    }

    async fn titles(&self, kind: Kind) -> Result<Vec<CuratedTitle>, Failure> {
        Self::filing(kind)?;
        let mut titles = Vec::new();
        for series in self.series().await? {
            if let Some(reference) = series.tvdb() {
                titles.push(self.curated_from(&series, reference).await?);
            }
        }
        Ok(titles)
    }

    async fn held_title(&self, kind: Kind, id: i64) -> Result<Option<CuratedTitle>, Failure> {
        Self::filing(kind)?;
        self.held_as(id).await
    }

    async fn request(&self, kind: Kind, request: &TitleRequest) -> Result<CuratedTitle, Failure> {
        Self::filing(kind)?;
        if request.reference <= 0 {
            return Err(self
                .endpoint
                .refused("a series is requested by its TheTVDB id, and this request names none"));
        }
        self.refuse_what_it_lacks(request).await?;
        let series = match self.followed_as(request.reference).await? {
            Some(series) => {
                self.watching(&series).await?;
                series
            }
            None => self.added(request).await?,
        };
        self.want(&series, &request.seasons).await?;
        if request.search {
            self.backlog(&series).await?;
        }
        let series = self.held_series(request.reference).await?;
        self.curated_from(&series, request.reference).await
    }

    async fn search(&self, kind: Kind, id: i64) -> Result<(), Failure> {
        Self::filing(kind)?;
        let series = self.held_series(id).await?;
        self.backlog(&series).await
    }

    async fn remove(&self, kind: Kind, id: i64) -> Result<(), Failure> {
        Self::filing(kind)?;
        let series = self.held_series(id).await?;
        let response = self
            .ask(
                Method::Delete,
                &format!("{SERIES}/{}?{REMOVING_FILES}", series.id.slug),
                None,
            )
            .await?;
        self.endpoint.expect_success(&response)
    }

    async fn downloading(&self, kind: Kind) -> Result<Vec<Downloading>, Failure> {
        Self::filing(kind)?;
        let flights = self.in_flight().await?;
        if flights.is_empty() {
            return Ok(Vec::new());
        }
        let held = self.tvdb_by_slug().await?;
        let slugs: BTreeSet<&str> = flights.iter().map(|flight| flight.slug.as_str()).collect();
        let mut named: BTreeMap<(String, u32, u32), String> = BTreeMap::new();
        for slug in slugs.into_iter().filter(|slug| held.contains_key(*slug)) {
            for part in self.episodes(slug, None).await? {
                named.insert((slug.to_owned(), part.season, part.number), part.title);
            }
        }
        Ok(flights
            .into_iter()
            .filter_map(|flight| {
                let title_id = *held.get(&flight.slug)?;
                let title = named
                    .get(&(flight.slug.clone(), flight.season, flight.episode))
                    .cloned()
                    .unwrap_or_default();
                Some(Downloading {
                    title_id,
                    release: flight.title,
                    bytes: flight.size,
                    left: if flight.stage == Stage::Downloaded {
                        0
                    } else {
                        flight.size
                    },
                    expected: None,
                    state: flight.standing,
                    download_id: flight.download_id,
                    episode: Some(DownloadedEpisode {
                        season: flight.season,
                        number: flight.episode,
                        title,
                    }),
                })
            })
            .collect())
    }

    async fn unqueue(&self, kind: Kind, _id: i64, _blocklist: bool) -> Result<String, Failure> {
        Self::filing(kind)?;
        Err(unsupported(NO_UNQUEUE))
    }

    async fn grab(&self, kind: Kind, id: i64, release: &str) -> Result<Grabbed, Failure> {
        Self::filing(kind)?;
        let series = self.held_series(id).await?;
        self.grabbed_again(&series.id.slug, release).await
    }
}

#[cfg(test)]
mod tests;
