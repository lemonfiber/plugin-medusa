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

const OF_USENET: &str = "newznab";

const OF_TORRENTS: &str = "torznab";

const STANDING: [&str; 2] = [OF_USENET, OF_TORRENTS];

const TORRENT: &str = "torrent";

const FOUND_NONE: u16 = 500;

const SWITCHES: [&str; 4] = [
    "/config/enabled",
    "/config/search/backlog/enabled",
    "/config/search/manual/enabled",
    "/config/search/daily/enabled",
];

const KEY_AT: &str = "/config/apikey";

const URL_AT: &str = "/config/url";

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
    let managed = !text(provider, "/manager").is_empty();
    let built_in = provider.get("default").and_then(Value::as_bool) == Some(true);
    let url = text(provider, URL_AT);
    if managed || built_in || url.is_empty() || !STANDING.contains(&text(provider, "/subType")) {
        return None;
    }
    Some(KnownAggregator {
        id: text(provider, "/id").to_owned(),
        url: url.to_owned(),
        keyed: !text(provider, KEY_AT).is_empty(),
    })
}

fn switched_on(provider: &mut Value) {
    for at in SWITCHES {
        if let Some(flag) = provider.pointer_mut(at) {
            *flag = Value::Bool(true);
        }
    }
}

struct Gaps<'a> {
    missing: Vec<&'a Indexer>,
    lost: Vec<String>,
    stale: BTreeSet<String>,
}

impl Gaps<'_> {
    fn none(&self) -> bool {
        self.missing.is_empty() && self.lost.is_empty() && self.stale.is_empty()
    }
}

fn gaps<'a>(indexers: &'a [Indexer], providers: &[Value], url: &str, key: &str) -> Gaps<'a> {
    let base = url.trim_end_matches('/');
    let reached = |indexer: &Indexer| format!("{base}/{}/api", indexer.id);
    let given: Vec<&Value> = providers
        .iter()
        .filter(|provider| text(provider, "/manager") == PROWLARR)
        .collect();
    let held: BTreeSet<&str> = given
        .iter()
        .map(|provider| text(provider, URL_AT))
        .collect();
    let wanted: BTreeSet<String> = indexers.iter().map(reached).collect();
    Gaps {
        missing: indexers
            .iter()
            .filter(|indexer| !held.contains(reached(indexer).as_str()))
            .collect(),
        lost: given
            .iter()
            .filter(|provider| !wanted.contains(text(provider, URL_AT)))
            .map(|provider| format!("{}/{}", text(provider, "/subType"), text(provider, "/id")))
            .collect(),
        stale: given
            .iter()
            .filter(|provider| wanted.contains(text(provider, URL_AT)))
            .filter(|provider| text(provider, KEY_AT) != key)
            .map(|provider| text(provider, "/id").to_owned())
            .collect(),
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

    async fn settle(
        &self,
        made: &BTreeSet<String>,
        stale: &BTreeSet<String>,
        key: &str,
    ) -> Result<(), Failure> {
        if made.is_empty() && stale.is_empty() {
            return Ok(());
        }
        let mut providers = self.providers().await?;
        for provider in &mut providers {
            let id = text(provider, "/id").to_owned();
            if made.contains(&id) {
                switched_on(provider);
            }
            if stale.contains(&id) {
                if let Some(held) = provider.pointer_mut(KEY_AT) {
                    *held = json!(key);
                }
            }
        }
        self.done(Method::Post, PROVIDERS, &json!({ "providers": providers }))
            .await
    }

    async fn prowlarr_answers(&self, url: &str, key: &str) -> Result<bool, Failure> {
        let asked = json!({ "type": "TEST", "url": url, "apikey": key });
        Ok(self
            .ask(Method::Post, OPERATION, Some(&asked))
            .await?
            .is_success())
    }

    async fn prowlarr_holds(&self, url: &str, key: &str) -> Result<Option<Vec<Indexer>>, Failure> {
        if !self.prowlarr_answers(url, key).await? {
            return Ok(None);
        }
        let asked = json!({ "type": "GETINDEXERS", "url": url, "apikey": key });
        let response = self.ask(Method::Post, OPERATION, Some(&asked)).await?;
        if response.status == FOUND_NONE {
            let emptied = self.prowlarr_answers(url, key).await?;
            return Ok(emptied.then(Vec::new));
        }
        let held: Vec<Indexer> = self.endpoint.decode(
            &response,
            "Prowlarr's indexers, as Medusa read them, could not be read",
        )?;
        Ok(Some(
            held.into_iter().filter(|indexer| indexer.enable).collect(),
        ))
    }

    async fn level_with(&self, url: &str, key: &str, providers: &[Value]) -> Result<bool, Failure> {
        let held = self.prowlarr_holds(url, key).await?;
        Ok(held.is_some_and(|indexers| gaps(&indexers, providers, url, key).none()))
    }

    async fn pull_prowlarr(&self, aggregator: &AggregatorAt) -> Result<(), Failure> {
        let (url, key) = (aggregator.url.as_str(), aggregator.key.as_str());
        self.configure(&json!({ "providers": { PROWLARR: { "url": url, "apikey": key } } }))
            .await?;
        let Some(indexers) = self.prowlarr_holds(url, key).await? else {
            return Err(self
                .endpoint
                .refused(&format!("Medusa could not reach Prowlarr at {url}")));
        };
        let providers = self.providers().await?;
        let gaps = gaps(&indexers, &providers, url, key);
        for lost in &gaps.lost {
            let response = self
                .ask(Method::Delete, &format!("{PROVIDERS}/{lost}"), None)
                .await?;
            self.endpoint.expect_success(&response)?;
        }
        let mut made = BTreeSet::new();
        for indexer in &gaps.missing {
            let kind = if indexer.protocol == TORRENT {
                OF_TORRENTS
            } else {
                OF_USENET
            };
            made.insert(
                self.made(
                    FROM_PROWLARR,
                    &json!({ "subType": kind, "id": indexer.id, "name": indexer.name }),
                )
                .await?,
            );
        }
        self.settle(&made, &gaps.stale, key).await
    }

    async fn pull_nzbhydra2(&self, aggregator: &AggregatorAt) -> Result<(), Failure> {
        let (url, key) = (aggregator.url.as_str(), aggregator.key.as_str());
        let providers = self.providers().await?;
        let held = providers
            .iter()
            .find(|provider| standing(provider).is_some_and(|known| known.url == url));
        if let Some(held) = held {
            if text(held, KEY_AT) == key {
                return Ok(());
            }
            let stale = BTreeSet::from([text(held, "/id").to_owned()]);
            return self.settle(&BTreeSet::new(), &stale, key).await;
        }
        let made = self
            .made(
                NEWZNAB,
                &json!({ "name": aggregator.name, "url": url, "apikey": key }),
            )
            .await?;
        self.settle(&BTreeSet::from([made]), &BTreeSet::new(), key)
            .await
    }
}

#[async_trait]
impl PullsIndexers for Medusa {
    async fn aggregators(&self) -> Result<Vec<KnownAggregator>, Failure> {
        let main: Value = self
            .read(super::CONFIGURATION, "Medusa's settings could not be read")
            .await?;
        let url = text(&main, "/providers/prowlarr/url");
        let key = text(&main, "/providers/prowlarr/apikey");
        let providers = self.providers().await?;
        let read_from = !url.is_empty() && self.level_with(url, key, &providers).await?;
        let prowlarr = read_from.then(|| KnownAggregator {
            id: PROWLARR.to_owned(),
            url: url.to_owned(),
            keyed: !key.is_empty(),
        });
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
