//! TheTVDB, asked under the operator's own key for the name and seasons of a series Medusa
//! does not hold, and only where the operator switched that on.

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex, PoisonError};

use lemonfiber_adapter_kit::endpoint::Endpoint;
use lemonfiber_ports::http::{Http, Method, Request, Response, JSON};
use lemonfiber_ports::service::{CuratedSeason, CuratedTitle, Failure};
use serde::Deserialize;
use serde_json::json;

/// Where TheTVDB's API answers.
pub const ADDRESS: &str = "https://api4.thetvdb.com/v4";

/// What every failure of TheTVDB's is blamed on.
pub const SERVICE: &str = "thetvdb";

const LOGIN: &str = "/login";

const AUTHORIZATION: &str = "Authorization";

const CONTENT_TYPE: &str = "Content-Type";

const PAGE: usize = 500;

const PAGES: u32 = 20;

const NOT_FOUND: u16 = 404;

const UNAUTHORISED: u16 = 401;

const YEAR_DIGITS: usize = 4;

const WITHHELD: &str = "withheld";

/// The key TheTVDB is signed in to with, and the subscriber PIN a user-supported key
/// needs beside it.
#[derive(Clone, PartialEq, Eq)]
pub struct SignIn {
    /// The operator's own API key.
    pub key: String,
    /// The operator's subscriber PIN, where their key needs one.
    pub pin: Option<String>,
}

impl std::fmt::Debug for SignIn {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("SignIn")
            .field("key", &WITHHELD)
            .field("pin", &self.pin.as_ref().map(|_| WITHHELD))
            .finish()
    }
}

/// TheTVDB, signed in to as `SignIn` says, keeping the token it hands over until it
/// refuses it.
pub struct TheTvdb {
    endpoint: Endpoint,
    sign_in: SignIn,
    token: Mutex<Option<String>>,
}

#[derive(Deserialize)]
struct Signed {
    data: Token,
}

#[derive(Deserialize)]
struct Token {
    token: String,
}

#[derive(Deserialize)]
struct Paged {
    data: Page,
}

#[derive(Deserialize)]
struct Page {
    series: Named,
    #[serde(default)]
    episodes: Vec<Aired>,
}

#[derive(Deserialize)]
struct Named {
    #[serde(default)]
    name: String,
    #[serde(default)]
    year: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Aired {
    #[serde(default)]
    season_number: u32,
}

fn counted(seasons: &mut BTreeMap<u32, u32>, page: &Page) {
    for aired in &page.episodes {
        *seasons.entry(aired.season_number).or_default() += 1;
    }
}

impl TheTvdb {
    /// TheTVDB reached at `base` over `http`, signed in to as `sign_in` says.
    #[must_use]
    pub fn new(http: Arc<dyn Http>, base: impl Into<String>, sign_in: SignIn) -> Self {
        Self {
            endpoint: Endpoint::new(http, base, SERVICE),
            sign_in,
            token: Mutex::new(None),
        }
    }

    fn held(&self) -> Option<String> {
        self.token
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }

    async fn signed_in(&self) -> Result<String, Failure> {
        let body = match &self.sign_in.pin {
            Some(pin) => json!({ "apikey": self.sign_in.key, "pin": pin }),
            None => json!({ "apikey": self.sign_in.key }),
        };
        let response = self
            .endpoint
            .send(&Request {
                method: Method::Post,
                url: self.endpoint.url(LOGIN),
                headers: vec![(CONTENT_TYPE.to_owned(), JSON.to_owned())],
                body: Some(body.to_string()),
                pinned: None,
            })
            .await?;
        let signed: Signed = self
            .endpoint
            .decode(&response, "TheTVDB did not hand over a token")?;
        *self.token.lock().unwrap_or_else(PoisonError::into_inner) =
            Some(signed.data.token.clone());
        Ok(signed.data.token)
    }

    async fn asked(&self, path: &str, token: String) -> Result<Response, Failure> {
        self.endpoint
            .send(&Request {
                method: Method::Get,
                url: self.endpoint.url(path),
                headers: vec![(AUTHORIZATION.to_owned(), format!("Bearer {token}"))],
                body: None,
                pinned: None,
            })
            .await
    }

    async fn page(&self, id: i64, page: u32) -> Result<Option<Page>, Failure> {
        let path = format!("/series/{id}/episodes/default?page={page}");
        let token = match self.held() {
            Some(token) => token,
            None => self.signed_in().await?,
        };
        let mut response = self.asked(&path, token).await?;
        if response.status == UNAUTHORISED {
            response = self.asked(&path, self.signed_in().await?).await?;
        }
        if response.status == NOT_FOUND {
            return Ok(None);
        }
        let paged: Paged = self.endpoint.decode(
            &response,
            "TheTVDB's episodes of the series could not be read",
        )?;
        Ok(Some(paged.data))
    }

    /// The series TheTVDB knows as `id`, with the episodes of each of its seasons, or none
    /// where it knows none.
    ///
    /// # Errors
    ///
    /// Returns [`Failure`] when TheTVDB is unreachable, refuses the key, or answers in a
    /// shape it does not read.
    pub async fn series(&self, id: i64) -> Result<Option<CuratedTitle>, Failure> {
        let Some(first) = self.page(id, 0).await? else {
            return Ok(None);
        };
        let mut seasons = BTreeMap::new();
        counted(&mut seasons, &first);
        let mut full = first.episodes.len() >= PAGE;
        for page in 1..PAGES {
            if !full {
                break;
            }
            let Some(next) = self.page(id, page).await? else {
                break;
            };
            counted(&mut seasons, &next);
            full = next.episodes.len() >= PAGE;
        }
        Ok(Some(CuratedTitle {
            reference: id,
            id: None,
            title: first.series.name,
            year: first
                .series
                .year
                .as_deref()
                .and_then(|year| year.get(..YEAR_DIGITS))
                .and_then(|year| year.parse().ok()),
            monitored: false,
            files: 0,
            seasons: seasons
                .into_iter()
                .map(|(number, episodes)| CuratedSeason {
                    number,
                    monitored: false,
                    files: 0,
                    episodes,
                })
                .collect(),
        }))
    }
}

#[cfg(test)]
mod tests;
