use std::collections::BTreeSet;

use async_trait::async_trait;
use lemonfiber_ports::http::Method;
use lemonfiber_ports::service::{AggregatorAt, Failure, KnownAggregator, PullsIndexers};
use serde::Deserialize;
use serde_json::{json, Value};

use super::series::PROVIDERS;
use super::{unsupported, Medusa};

const PROWLARR: &str = "prowlarr";

const READS_SERVARR: &str = "servarr";

const READS_NZBHYDRA2: &str = "nzbhydra2";

const OPERATION: &str = "/providers/prowlarr/operation";

const FROM_PROWLARR: &str = "/providers/prowlarr";

const NEWZNAB: &str = "/providers/newznab";

const STANDING: [&str; 2] = ["newznab", "torznab"];

const TORRENT: &str = "torrent";

const FOUND_NONE: u16 = 500;

const SWITCHES: [&str; 4] = [
    "/config/enabled",
    "/config/search/backlog/enabled",
    "/config/search/manual/enabled",
    "/config/search/daily/enabled",
];

const PULLS_FROM: &str =
    "it pulls its indexers from Prowlarr, read in the servarr API, and from NZBHydra2";

#[derive(Deserialize)]
struct Indexer {
    id: i64,
    #[serde(default)]
    name: String,
    #[serde(default)]
    protocol: String,
    #[serde(default)]
    enable: bool,
}

#[derive(Deserialize)]
struct Made {
    result: Value,
}

fn text<'a>(value: &'a Value, at: &str) -> &'a str {
    value
        .pointer(at)
        .and_then(Value::as_str)
        .unwrap_or_default()
}

fn standing(provider: &Value) -> Option<KnownAggregator> {
    let managed = provider
        .get("manager")
        .is_some_and(|manager| !manager.is_null());
    let built_in = provider.get("default").and_then(Value::as_bool) == Some(true);
    if managed || built_in || !STANDING.contains(&text(provider, "/subType")) {
        return None;
    }
    Some(KnownAggregator {
        id: text(provider, "/id").to_owned(),
        url: text(provider, "/config/url").to_owned(),
        keyed: !text(provider, "/config/apikey").is_empty(),
    })
}

fn switched_on(provider: &mut Value) {
    for at in SWITCHES {
        if let Some(flag) = provider.pointer_mut(at) {
            *flag = Value::Bool(true);
        }
    }
}

impl Medusa {
    async fn made(&self, path: &str, body: &Value) -> Result<String, Failure> {
        let response = self.ask(Method::Post, path, Some(body)).await?;
        let made: Made = self
            .endpoint
            .decode(&response, "Medusa did not say which provider it made")?;
        Ok(text(&made.result, "/id").to_owned())
    }

    async fn switch_on(&self, made: &BTreeSet<String>) -> Result<(), Failure> {
        if made.is_empty() {
            return Ok(());
        }
        let mut providers = self.providers().await?;
        for provider in &mut providers {
            if made.contains(text(provider, "/id")) {
                switched_on(provider);
            }
        }
        self.done(Method::Post, PROVIDERS, &json!({ "providers": providers }))
            .await
    }

    async fn prowlarr_indexers(&self, aggregator: &AggregatorAt) -> Result<Vec<Indexer>, Failure> {
        let asked =
            |kind: &str| json!({ "type": kind, "url": aggregator.url, "apikey": aggregator.key });
        self.done(Method::Post, OPERATION, &asked("TEST")).await?;
        let response = self
            .ask(Method::Post, OPERATION, Some(&asked("GETINDEXERS")))
            .await?;
        if response.status == FOUND_NONE {
            return Ok(Vec::new());
        }
        self.endpoint.decode(
            &response,
            "Prowlarr's indexers, as Medusa read them, could not be read",
        )
    }

    async fn pull_prowlarr(&self, aggregator: &AggregatorAt) -> Result<(), Failure> {
        self.configure(&json!({
            "providers": { PROWLARR: { "url": aggregator.url, "apikey": aggregator.key } }
        }))
        .await?;
        let indexers = self.prowlarr_indexers(aggregator).await?;
        let held: BTreeSet<String> = self
            .providers()
            .await?
            .iter()
            .filter(|provider| text(provider, "/manager") == PROWLARR)
            .map(|provider| text(provider, "/config/url").to_owned())
            .collect();
        let base = aggregator.url.trim_end_matches('/');
        let mut made = BTreeSet::new();
        for indexer in indexers.into_iter().filter(|indexer| indexer.enable) {
            if held.contains(&format!("{base}/{}/api", indexer.id)) {
                continue;
            }
            let kind = if indexer.protocol == TORRENT {
                "torznab"
            } else {
                "newznab"
            };
            made.insert(
                self.made(
                    FROM_PROWLARR,
                    &json!({ "subType": kind, "id": indexer.id, "name": indexer.name }),
                )
                .await?,
            );
        }
        self.switch_on(&made).await
    }

    async fn pull_nzbhydra2(&self, aggregator: &AggregatorAt) -> Result<(), Failure> {
        let made = self
            .made(
                NEWZNAB,
                &json!({ "name": aggregator.name, "url": aggregator.url, "apikey": aggregator.key }),
            )
            .await?;
        self.switch_on(&BTreeSet::from([made])).await
    }
}

#[async_trait]
impl PullsIndexers for Medusa {
    async fn aggregators(&self) -> Result<Vec<KnownAggregator>, Failure> {
        let main: Value = self
            .read(super::CONFIGURATION, "Medusa's settings could not be read")
            .await?;
        let prowlarr = Some(KnownAggregator {
            id: PROWLARR.to_owned(),
            url: text(&main, "/providers/prowlarr/url").to_owned(),
            keyed: !text(&main, "/providers/prowlarr/apikey").is_empty(),
        })
        .filter(|known| !known.url.is_empty());
        let providers = self.providers().await?;
        Ok(prowlarr
            .into_iter()
            .chain(providers.iter().filter_map(standing))
            .collect())
    }

    async fn add_aggregator(&self, aggregator: &AggregatorAt) -> Result<(), Failure> {
        match aggregator.protocol.0.as_str() {
            READS_SERVARR => self.pull_prowlarr(aggregator).await,
            READS_NZBHYDRA2 => self.pull_nzbhydra2(aggregator).await,
            _ => Err(unsupported(PULLS_FROM)),
        }
    }
}

#[cfg(test)]
mod tests;
