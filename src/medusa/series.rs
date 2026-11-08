use std::collections::BTreeMap;

use lemonfiber_ports::http::Method;
use lemonfiber_ports::service::Failure;
use serde_json::{json, Map, Value};

use super::wire::{Consts, Main, Part, Preset, Qualities, Series};
use super::Medusa;

pub(crate) const SERIES: &str = "/series";

pub(crate) const BACKLOG: &str = "/search/backlog";

pub(crate) const PROVIDERS: &str = "/providers";

const CONSTS: &str = "/config/consts";

impl Medusa {
    pub(crate) async fn series(&self) -> Result<Vec<Series>, Failure> {
        self.every(SERIES, "the series Medusa follows could not be read")
            .await
    }

    pub(crate) async fn followed(&self) -> Result<Vec<serde_json::Value>, Failure> {
        self.every(SERIES, "the series Medusa follows could not be read")
            .await
    }

    pub(crate) async fn tvdb_by_slug(&self) -> Result<BTreeMap<String, i64>, Failure> {
        Ok(self
            .series()
            .await?
            .into_iter()
            .filter_map(|series| Some((series.id.slug.clone(), series.tvdb()?)))
            .collect())
    }

    pub(crate) async fn slug_of(&self, tvdb: i64) -> Result<Option<String>, Failure> {
        Ok(self
            .series()
            .await?
            .into_iter()
            .find(|one| one.tvdb() == Some(tvdb))
            .map(|one| one.id.slug))
    }

    pub(crate) async fn episodes(
        &self,
        slug: &str,
        season: Option<u32>,
    ) -> Result<Vec<Part>, Failure> {
        let season = season.map_or_else(String::new, |number| format!("?season={number}"));
        self.every(
            &format!("{SERIES}/{slug}/episodes{season}"),
            "the episodes could not be read",
        )
        .await
    }

    pub(crate) async fn main(&self) -> Result<Main, Failure> {
        self.read(super::CONFIGURATION, "Medusa's settings could not be read")
            .await
    }

    pub(crate) async fn presets(&self) -> Result<Vec<Preset>, Failure> {
        let consts: Consts = self
            .read(CONSTS, "Medusa's quality presets could not be read")
            .await?;
        Ok(consts
            .qualities
            .presets
            .into_iter()
            .filter(|preset| !preset.name.is_empty())
            .collect())
    }

    pub(crate) async fn providers(&self) -> Result<Vec<Value>, Failure> {
        self.read(PROVIDERS, "Medusa's search providers could not be read")
            .await
    }

    pub(crate) async fn follow(
        &self,
        tvdb: i64,
        folder: &str,
        quality: i64,
        chosen: &[(&str, Value)],
    ) -> Result<(), Failure> {
        let qualities = Qualities::of(quality);
        let mut options = Map::from_iter([
            ("rootDir".to_owned(), json!(folder)),
            (
                "quality".to_owned(),
                json!({ "allowed": qualities.allowed, "preferred": qualities.preferred }),
            ),
        ]);
        options.extend(
            chosen
                .iter()
                .map(|(at, value)| ((*at).to_owned(), value.clone())),
        );
        self.done(
            Method::Post,
            SERIES,
            &json!({ "id": { "tvdb": tvdb }, "options": options }),
        )
        .await
    }
}
