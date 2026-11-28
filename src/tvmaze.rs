//! TVmaze's public API, asked with no key for the name and seasons of a series Medusa does
//! not hold, and only where the operator switched that on.

use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use lemonfiber_adapter_kit::endpoint::Endpoint;
use lemonfiber_ports::http::{Http, Method, Request, Response};
use lemonfiber_ports::service::{CuratedTitle, Failure};
use serde::Deserialize;

use crate::lookup::{unheld, Lookup};

/// Where TVmaze's API answers.
pub const ADDRESS: &str = "https://api.tvmaze.com";

/// What every failure of TVmaze's is blamed on.
pub const SERVICE: &str = "tvmaze";

const LOOKUP: &str = "/lookup/shows?thetvdb=";

const SHOWS: &str = "/shows/";

const EPISODES: &str = "?embed=episodes";

const LOCATION: &str = "Location";

const MOVED: u16 = 301;

const NOT_FOUND: u16 = 404;

const TOO_MANY: u16 = 429;

const ATTEMPTS: u32 = 3;

const BACK_OFF: Duration = Duration::from_secs(2);

const ELSEWHERE: &str = "TVmaze did not name one of its own shows for the series";

/// TVmaze, asked again after a pause while it answers that it is asked too often.
pub struct TvMaze {
    endpoint: Endpoint,
    back_off: Duration,
}

#[derive(Deserialize)]
struct Show {
    name: String,
    #[serde(default)]
    premiered: Option<String>,
    externals: Externals,
    #[serde(rename = "_embedded")]
    embedded: Embedded,
}

#[derive(Deserialize)]
struct Externals {
    #[serde(default)]
    thetvdb: Option<i64>,
}

#[derive(Deserialize)]
struct Embedded {
    episodes: Vec<Episode>,
}

#[derive(Deserialize)]
struct Episode {
    season: u32,
    #[serde(default)]
    number: Option<u32>,
}

fn numbered(id: &str) -> Option<u64> {
    if id.is_empty() || !id.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    id.parse().ok()
}

impl TvMaze {
    /// TVmaze reached at `base` over `http`.
    #[must_use]
    pub fn new(http: Arc<dyn Http>, base: impl Into<String>) -> Self {
        Self {
            endpoint: Endpoint::new(http, base, SERVICE),
            back_off: BACK_OFF,
        }
    }

    /// The same TVmaze, asked again `back_off` after it answers that it is asked too often.
    #[must_use]
    pub fn backing_off(mut self, back_off: Duration) -> Self {
        self.back_off = back_off;
        self
    }

    async fn asked(&self, path: &str) -> Result<Response, Failure> {
        let request = Request {
            method: Method::Get,
            url: self.endpoint.url(path),
            headers: Vec::new(),
            body: None,
            pinned: None,
            deadline: None,
        };
        let mut response = self.endpoint.send(&request).await?;
        for _ in 1..ATTEMPTS {
            if response.status != TOO_MANY {
                break;
            }
            tokio::time::sleep(self.back_off).await;
            response = self.endpoint.send(&request).await?;
        }
        Ok(response)
    }

    fn show_in(&self, moved: &Response) -> Result<u64, Failure> {
        let shows = self.endpoint.url(SHOWS);
        moved
            .header(LOCATION)
            .and_then(|location| location.strip_prefix(shows.as_str()))
            .and_then(numbered)
            .ok_or_else(|| self.endpoint.refused(ELSEWHERE))
    }
}

#[async_trait]
impl Lookup for TvMaze {
    async fn series(&self, id: i64) -> Result<Option<CuratedTitle>, Failure> {
        if id <= 0 {
            return Ok(None);
        }
        let looked = self.asked(&format!("{LOOKUP}{id}")).await?;
        match looked.status {
            NOT_FOUND => return Ok(None),
            MOVED => {}
            _ => return Err(self.endpoint.refusal(&looked)),
        }
        let show = self.show_in(&looked)?;
        let answer = self.asked(&format!("{SHOWS}{show}{EPISODES}")).await?;
        let show: Show = self
            .endpoint
            .decode(&answer, "TVmaze's show and its episodes could not be read")?;
        if show.externals.thetvdb != Some(id) {
            return Err(self.endpoint.refused(&format!(
                "TVmaze's show for the series names another TheTVDB id than {id}"
            )));
        }
        if show.name.trim().is_empty() {
            return Err(self.endpoint.refused("TVmaze names the series nothing"));
        }
        let mut seasons: BTreeMap<u32, u32> = BTreeMap::new();
        for episode in &show.embedded.episodes {
            if episode.number.is_some() {
                *seasons.entry(episode.season).or_default() += 1;
            }
        }
        Ok(Some(unheld(
            id,
            show.name,
            show.premiered.as_deref(),
            seasons,
        )))
    }
}

#[cfg(test)]
mod tests;
