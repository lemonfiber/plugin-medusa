use async_trait::async_trait;
use lemonfiber_adapter_kit::endpoint::form_encoded;
use lemonfiber_ports::http::Method;
use lemonfiber_ports::media::{Format, Kind};
use lemonfiber_ports::service::{
    AddPlan, Added, Catalogue, CatalogueEntry, Failure, Maintenance, MusicQuality, QualityReleases,
    ReleaseProbe,
};
use serde::Deserialize;
use serde_json::{json, Value};

use super::series::PROVIDERS;
use super::wire::Qualities;
use super::{unsupported, Medusa};

const SEARCHED: &str = "/internal/searchIndexersForShowName";

const TVDB: &str = "1";

const WANTED_LIST: &str = "/internal/getEpisodeBacklog?status=wanted&period=all";

const BACKLOG: &str = "/search/backlog";

const MANUAL: &str = "/search/manual";

const WANTED: i64 = 3;

const FINISHED: &str = "finished";

const NOT_FOUND: u16 = 404;

const TITLE_AT: usize = 4;

const REFERENCE_AT: usize = 3;

const FIRST_AIRED_AT: usize = 5;

const HELD_AT: usize = 8;

const YEAR_DIGITS: usize = 4;

#[derive(Deserialize)]
struct Searched {
    #[serde(default)]
    results: Vec<Vec<Value>>,
}

fn entry(found: &[Value]) -> Option<CatalogueEntry> {
    let reference = found.get(REFERENCE_AT)?.as_i64()?;
    let held = found
        .get(HELD_AT)
        .is_some_and(|held| !matches!(held, Value::Bool(false) | Value::Null));
    Some(CatalogueEntry {
        title: found.get(TITLE_AT)?.as_str()?.to_owned(),
        year: found
            .get(FIRST_AIRED_AT)
            .and_then(Value::as_str)
            .and_then(|aired| aired.get(..YEAR_DIGITS))
            .and_then(|year| year.parse().ok()),
        reference,
        held_as: held.then_some(reference),
    })
}

#[derive(Deserialize)]
struct BacklogEpisode {
    #[serde(default)]
    season: u32,
    #[serde(default)]
    episode: u32,
    #[serde(default)]
    slug: String,
}

#[derive(Deserialize)]
struct BacklogSeries {
    #[serde(default)]
    slug: String,
    #[serde(default)]
    quality: i64,
    #[serde(default)]
    episodes: Vec<BacklogEpisode>,
}

#[derive(Deserialize, Default)]
struct SearchedEpisode {
    #[serde(default)]
    slug: String,
}

#[derive(Deserialize)]
struct Searching {
    #[serde(default)]
    episode: SearchedEpisode,
    #[serde(default)]
    searchstatus: String,
}

#[derive(Deserialize)]
struct Searches {
    #[serde(default)]
    results: Vec<Searching>,
}

#[derive(Deserialize)]
struct Release {
    #[serde(default)]
    quality: i64,
}

fn searching(provider: &Value) -> bool {
    provider.pointer("/config/enabled").and_then(Value::as_bool) == Some(true)
}

fn searched_by_hand(provider: &Value) -> Option<String> {
    let manual = provider
        .pointer("/config/search/manual/enabled")
        .and_then(Value::as_bool)
        == Some(true);
    provider
        .get("id")
        .and_then(Value::as_str)
        .filter(|_| searching(provider) && manual)
        .map(str::to_owned)
}

impl Medusa {
    async fn finished(&self, series: &str, episode: &str) -> Result<bool, Failure> {
        for _ in 0..super::SEARCH_ROUNDS {
            tokio::time::sleep(self.pause).await;
            let searches: Searches = self
                .read(
                    &format!("/search/{series}"),
                    "Medusa did not say how its search is going",
                )
                .await?;
            let ours: Vec<&Searching> = searches
                .results
                .iter()
                .filter(|one| one.episode.slug == episode)
                .collect();
            if !ours.is_empty() && ours.iter().all(|one| one.searchstatus == FINISHED) {
                return Ok(true);
            }
        }
        Ok(false)
    }

    async fn found(&self, series: &str, wanted: &BacklogEpisode) -> Result<Vec<i64>, Failure> {
        let providers = self.providers().await?;
        let query = form_encoded(&[
            ("showslug", series),
            ("season", &wanted.season.to_string()),
            ("episode", &wanted.episode.to_string()),
        ]);
        let mut qualities = Vec::new();
        for provider in providers.iter().filter_map(searched_by_hand) {
            let response = self
                .ask(
                    Method::Get,
                    &format!("{PROVIDERS}/{provider}/results?{query}"),
                    None,
                )
                .await?;
            if response.status == NOT_FOUND {
                continue;
            }
            let releases: Vec<Release> = self
                .endpoint
                .decode(&response, "what a provider found could not be read")?;
            qualities.extend(releases.into_iter().map(|release| release.quality));
        }
        Ok(qualities)
    }
}

#[async_trait]
impl Catalogue for Medusa {
    async fn lookup(&self, kind: Kind, term: &str) -> Result<Vec<CatalogueEntry>, Failure> {
        Self::filing(kind)?;
        let query = form_encoded(&[("query", term), ("indexerId", TVDB)]);
        let searched: Searched = self
            .read(
                &format!("{SEARCHED}?{query}"),
                "what Medusa found could not be read",
            )
            .await?;
        Ok(searched
            .results
            .iter()
            .filter_map(|found| entry(found))
            .collect())
    }

    async fn add_plan(&self, kind: Kind) -> Result<AddPlan, Failure> {
        Self::filing(kind)?;
        let held = self.main().await?;
        let Some(root_folder) = held.default_folder() else {
            return Err(self.endpoint.refused(&format!(
                "{} has no root folder configured yet",
                kind.noun()
            )));
        };
        Ok(AddPlan {
            root_folder,
            quality_profile: held.show_defaults.quality,
        })
    }

    async fn add(
        &self,
        kind: Kind,
        entry: &CatalogueEntry,
        plan: &AddPlan,
    ) -> Result<Added, Failure> {
        Self::filing(kind)?;
        if entry.reference <= 0 {
            return Err(self
                .endpoint
                .refused("a series is added by its TheTVDB id, and this entry names none"));
        }
        self.follow(
            entry.reference,
            &plan.root_folder,
            plan.quality_profile,
            [("status", json!(WANTED)), ("seasonFolders", json!(true))],
        )
        .await?;
        Ok(Added {
            id: entry.reference,
            title: entry.title.clone(),
        })
    }

    async fn indexer_count(&self) -> Result<usize, Failure> {
        let providers = self.providers().await?;
        Ok(providers.iter().filter(|one| searching(one)).count())
    }
}

#[async_trait]
impl Maintenance for Medusa {
    async fn search_upgrades(&self, kind: Kind) -> Result<(), Failure> {
        Self::filing(kind)?;
        let response = self.ask(Method::Put, BACKLOG, None).await?;
        self.endpoint.expect_success(&response)
    }

    async fn wanting(&self) -> Result<bool, Failure> {
        let wanted: Vec<Value> = self
            .read(WANTED_LIST, "what Medusa wants could not be read")
            .await?;
        Ok(!wanted.is_empty())
    }
}

#[async_trait]
impl QualityReleases for Medusa {
    async fn probe_releases(&self, kind: Kind) -> Result<ReleaseProbe, Failure> {
        Self::filing(kind)?;
        let wanted: Vec<BacklogSeries> = self
            .read(WANTED_LIST, "what Medusa wants could not be read")
            .await?;
        let Some((series, episode)) = wanted
            .iter()
            .find_map(|series| series.episodes.first().map(|episode| (series, episode)))
        else {
            return Ok(ReleaseProbe::NothingWanted);
        };
        self.done(
            Method::Put,
            MANUAL,
            &json!({ "showSlug": series.slug, "episodes": [episode.slug] }),
        )
        .await?;
        if !self.finished(&series.slug, &episode.slug).await? {
            return Err(self
                .endpoint
                .refused("Medusa's search for releases did not finish in time"));
        }
        let found = self.found(&series.slug, episode).await?;
        let grabbed = Qualities::of(series.quality).wanted();
        Ok(if found.is_empty() {
            ReleaseProbe::NoneFound
        } else if found.iter().any(|quality| quality & grabbed != 0) {
            ReleaseProbe::Matching
        } else {
            ReleaseProbe::NoneMatch
        })
    }
}

#[async_trait]
impl MusicQuality for Medusa {
    async fn apply_music_format(&self, _format: Format) -> Result<(), Failure> {
        Err(unsupported("it files no music"))
    }
}

#[cfg(test)]
mod tests;
