//! Medusa, the series curator, asked through its own API v2 with the key lemonfiber wrote
//! for this adapter, and through the pages its own interface asks where that API has no
//! answer.

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;

use lemonfiber_adapter_kit::credential::Holding;
use lemonfiber_adapter_kit::endpoint::{form_encoded, Endpoint};
use lemonfiber_adapter_kit::recounting::Recounter;
use lemonfiber_ports::http::{Http, Method, Request, Response, JSON};
use lemonfiber_ports::media::TitleKind;
use lemonfiber_ports::service::{Failure, Record};
use serde_json::Value;

mod again;
mod carrying;
mod catalogue;
mod clients;
mod fulfilling;
mod importing;
mod pulling;
mod queue;
mod recounting;
mod series;
mod tracing;
mod wire;

pub use recounting::Happenings;

/// What every failure is blamed on.
pub const SERVICE: &str = "medusa";

const API: &str = "/api/v2";

const SIGN_IN: &str = "/login/";

const RELEASE: &str = "/config/system/release";

const KEY_HEADER: &str = "X-Api-Key";

const CONTENT_TYPE: &str = "Content-Type";

const COOKIE: &str = "Cookie";

const SET_COOKIE: &str = "set-cookie";

const FORM: &str = "application/x-www-form-urlencoded";

const CONFIGURATION: &str = "/config/main";

const PAGE: usize = 1000;

const SEARCH_WAIT: Duration = Duration::from_secs(1);

const SEARCH_ROUNDS: usize = 10;

/// How long an import may wait before it is reported stuck, unless the adapter is told
/// otherwise: twelve passes of the download handler the adapter sets running every five
/// minutes.
pub const IMPORT_PATIENCE: Duration = Duration::from_secs(60 * 60);

/// A client for one Medusa, presenting the key `held` supplies on every call to its API,
/// and signing in to its own pages as the account `username` and `password` supply.
pub struct Medusa {
    endpoint: Endpoint,
    held: Arc<dyn Holding>,
    username: Arc<dyn Holding>,
    password: Arc<dyn Holding>,
    pause: Duration,
    patience: Option<Duration>,
    pending: Mutex<BTreeMap<String, tokio::time::Instant>>,
    lookup: Option<Arc<crate::thetvdb::TheTvdb>>,
    recounter: OnceLock<Recounter>,
}

impl Medusa {
    /// The Medusa reached at `base` over `http`, asked with the key `held` supplies and
    /// signed in to as the account `username` and `password` supply.
    #[must_use]
    pub fn new(
        http: Arc<dyn Http>,
        base: impl Into<String>,
        held: Arc<dyn Holding>,
        username: Arc<dyn Holding>,
        password: Arc<dyn Holding>,
    ) -> Self {
        Self {
            endpoint: Endpoint::new(http, base, SERVICE),
            held,
            username,
            password,
            pause: SEARCH_WAIT,
            patience: Some(IMPORT_PATIENCE),
            pending: Mutex::new(BTreeMap::new()),
            lookup: None,
            recounter: OnceLock::new(),
        }
    }

    /// The same Medusa, asking `lookup` for a series it does not hold, or nothing where
    /// there is none.
    #[must_use]
    pub fn looking_up(mut self, lookup: Option<Arc<crate::thetvdb::TheTvdb>>) -> Self {
        self.lookup = lookup;
        self
    }

    /// The same Medusa, reporting an import stuck once it has waited `patience`, or never
    /// where there is none.
    #[must_use]
    pub fn waiting_on_imports(mut self, patience: Option<Duration>) -> Self {
        self.patience = patience;
        self
    }

    /// The same Medusa, looking again at a search it started after `pause` each time.
    #[must_use]
    pub fn pausing(mut self, pause: Duration) -> Self {
        self.pause = pause;
        self
    }

    /// Whether Medusa's API says which release it runs.
    pub async fn answers(&self) -> bool {
        self.release().await.is_ok()
    }

    pub(crate) async fn release(&self) -> Result<String, Failure> {
        self.read(RELEASE, "Medusa did not say which release it runs")
            .await
    }

    fn held(&self, holding: &dyn Holding, what: &str) -> Result<String, Failure> {
        holding.key().map_err(|why| {
            self.endpoint
                .refused(&format!("this adapter holds no {what} for Medusa: {why}"))
        })
    }

    fn keyed(&self, method: Method, path: &str, body: Option<&Value>) -> Result<Request, Failure> {
        let key = self.held(self.held.as_ref(), "key")?;
        let mut headers = vec![(KEY_HEADER.to_owned(), key)];
        if body.is_some() {
            headers.push((CONTENT_TYPE.to_owned(), JSON.to_owned()));
        }
        Ok(Request {
            method,
            url: self.endpoint.url(&format!("{API}{path}")),
            headers,
            body: body.map(Value::to_string),
            pinned: None,
        })
    }

    async fn ask(
        &self,
        method: Method,
        path: &str,
        body: Option<&Value>,
    ) -> Result<Response, Failure> {
        self.endpoint.send(&self.keyed(method, path, body)?).await
    }

    async fn read<T: serde::de::DeserializeOwned>(
        &self,
        path: &str,
        what: &str,
    ) -> Result<T, Failure> {
        let response = self.ask(Method::Get, path, None).await?;
        self.endpoint.decode(&response, what)
    }

    async fn done(&self, method: Method, path: &str, body: &Value) -> Result<(), Failure> {
        let response = self.ask(method, path, Some(body)).await?;
        self.endpoint.expect_success(&response)
    }

    async fn every<T: serde::de::DeserializeOwned>(
        &self,
        path: &str,
        what: &str,
    ) -> Result<Vec<T>, Failure> {
        let joined = if path.contains('?') { '&' } else { '?' };
        let mut all = Vec::new();
        for page in 1.. {
            let one: Vec<T> = self
                .read(&format!("{path}{joined}limit={PAGE}&page={page}"), what)
                .await?;
            let full = one.len() == PAGE;
            all.extend(one);
            if !full {
                break;
            }
        }
        Ok(all)
    }

    async fn configure(&self, patch: &Value) -> Result<(), Failure> {
        let response = self.ask(Method::Patch, CONFIGURATION, Some(patch)).await?;
        let taken: Value = self
            .endpoint
            .decode(&response, "Medusa did not say what it took")?;
        if &taken == patch {
            Ok(())
        } else {
            Err(self
                .endpoint
                .refused("Medusa did not take every setting it was given"))
        }
    }

    async fn signed_in(&self) -> Result<String, Failure> {
        let username = self.held(self.username.as_ref(), "account name")?;
        let password = self.held(self.password.as_ref(), "password")?;
        let request = Request {
            method: Method::Post,
            url: self.endpoint.url(SIGN_IN),
            headers: vec![(CONTENT_TYPE.to_owned(), FORM.to_owned())],
            body: Some(form_encoded(&[
                ("username", &username),
                ("password", &password),
            ])),
            pinned: None,
        };
        let response = self.endpoint.send(&request).await?;
        let cookies: Vec<&str> = response
            .headers
            .iter()
            .filter(|(name, _)| name.eq_ignore_ascii_case(SET_COOKIE))
            .filter_map(|(_, value)| value.split(';').next())
            .map(str::trim)
            .filter(|cookie| !cookie.is_empty())
            .collect();
        if cookies.is_empty() {
            Err(self.endpoint.unauthorised())
        } else {
            Ok(cookies.join("; "))
        }
    }

    async fn page(&self, session: &str, path: &str) -> Result<String, Failure> {
        let request = Request {
            method: Method::Get,
            url: self.endpoint.url(path),
            headers: vec![(COOKIE.to_owned(), session.to_owned())],
            body: None,
            pinned: None,
        };
        let response = self.endpoint.send(&request).await?;
        self.endpoint.expect_success(&response)?;
        Ok(response.body)
    }

    fn filing(kind: impl Into<TitleKind>) -> Result<(), Failure> {
        match kind.into() {
            TitleKind::Tv => Ok(()),
            other => Err(unsupported(&format!("it files no {}", other.noun()))),
        }
    }

    fn holding(record: Record) -> Result<(), Failure> {
        if record == Record::Series {
            Ok(())
        } else {
            Err(unsupported(&format!("it holds no {}", record.plural())))
        }
    }
}

/// What Medusa cannot do, answered as an operation it does not offer.
fn unsupported(detail: &str) -> Failure {
    Failure::Unsupported {
        service: SERVICE.to_owned(),
        detail: detail.to_owned(),
    }
}

#[cfg(test)]
mod tests;
