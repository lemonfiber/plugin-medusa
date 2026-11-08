use std::collections::BTreeSet;

use lemonfiber_adapter_kit::endpoint::form_encoded;
use lemonfiber_ports::digest::sha256;
use lemonfiber_ports::http::Method;
use lemonfiber_ports::service::stage::HISTORY_HORIZON;
use lemonfiber_ports::service::{Failure, Grabbed};
use serde::Deserialize;
use serde_json::json;

use super::catalogue::{searched_by_hand, MANUAL, NOT_FOUND};
use super::queue::{newest_history, SNATCHED};
use super::series::PROVIDERS;
use super::wire::HistoryRow;
use super::Medusa;

const FAILED_RELEASES: &str = "/internal/getFailed?limit=0";

const LIFT_FAILED: &str = "/internal/removeFailed";

const PICK: &str = "/home/pickManualSearch";

const RESULTS_PAGE: &str = "1000";

const PICKED: &str = "success";

const NZB: &str = ".nzb";

#[derive(Deserialize)]
struct FailedRelease {
    id: i64,
    #[serde(default)]
    release: String,
}

#[derive(Deserialize)]
struct Offered {
    #[serde(default)]
    identifier: String,
    #[serde(default)]
    release: String,
}

#[derive(Deserialize)]
struct Picked {
    #[serde(default)]
    result: String,
}

pub(crate) fn fingerprint(release: &str) -> Option<String> {
    (!release.is_empty()).then(|| sha256(release))
}

fn failed_name(release: &str) -> String {
    release
        .strip_suffix(NZB)
        .unwrap_or(release)
        .chars()
        .map(|one| {
            if one.is_alphanumeric() || one == '_' {
                one
            } else {
                '_'
            }
        })
        .collect()
}

fn episode_slug(season: u32, episode: u32) -> String {
    format!("s{season:02}e{episode:02}")
}

impl Medusa {
    pub(crate) async fn grabbed_again(
        &self,
        slug: &str,
        release: &str,
    ) -> Result<Grabbed, Failure> {
        let rows: Vec<HistoryRow> = self
            .read(
                &format!("/history/{slug}?{}", newest_history(HISTORY_HORIZON)),
                "Medusa's history could not be read",
            )
            .await?;
        let snatched: Vec<HistoryRow> = rows
            .into_iter()
            .filter(|row| {
                SNATCHED.contains(&row.status)
                    && fingerprint(&row.resource).as_deref() == Some(release)
            })
            .collect();
        let Some(first) = snatched.first() else {
            return Ok(Grabbed::Gone);
        };
        self.lifted(&first.resource).await?;
        let episodes: BTreeSet<(u32, u32)> = snatched
            .iter()
            .map(|row| (row.season, row.episode))
            .collect();
        let season = first.season;
        let (segment, watched) = if episodes.len() > 1 {
            (
                json!({ "showSlug": slug, "season": [format!("s{season:02}")] }),
                episode_slug(season, 1),
            )
        } else {
            let one = episode_slug(season, first.episode);
            (json!({ "showSlug": slug, "episodes": [one] }), one)
        };
        self.done(Method::Put, MANUAL, &segment).await?;
        if !self.finished(slug, &watched).await? {
            return Err(self
                .endpoint
                .refused("Medusa's search for the release did not finish in time"));
        }
        let Some((provider, identifier)) = self.offered(slug, season, release).await? else {
            return Ok(Grabbed::Gone);
        };
        let session = self.signed_in().await?;
        let said = self
            .page(
                &session,
                &format!(
                    "{PICK}?{}",
                    form_encoded(&[("provider", &provider), ("identifier", &identifier)])
                ),
            )
            .await?;
        if serde_json::from_str::<Picked>(&said).is_ok_and(|picked| picked.result == PICKED) {
            Ok(Grabbed::Grabbed)
        } else {
            Err(self
                .endpoint
                .refused("Medusa did not snatch the release a provider offered"))
        }
    }

    async fn lifted(&self, resource: &str) -> Result<(), Failure> {
        let name = failed_name(resource);
        let failed: Vec<FailedRelease> = self
            .read(
                FAILED_RELEASES,
                "Medusa's failed releases could not be read",
            )
            .await?;
        let lifted: Vec<i64> = failed
            .into_iter()
            .filter(|one| one.release == name)
            .map(|one| one.id)
            .collect();
        if lifted.is_empty() {
            return Ok(());
        }
        self.done(Method::Post, LIFT_FAILED, &json!({ "remove": lifted }))
            .await
    }

    async fn offered(
        &self,
        slug: &str,
        season: u32,
        release: &str,
    ) -> Result<Option<(String, String)>, Failure> {
        let query = form_encoded(&[
            ("showslug", slug),
            ("season", &season.to_string()),
            ("limit", RESULTS_PAGE),
        ]);
        for provider in self.providers().await?.iter().filter_map(searched_by_hand) {
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
            let offers: Vec<Offered> = self
                .endpoint
                .decode(&response, "what a provider found could not be read")?;
            if let Some(offer) = offers
                .into_iter()
                .find(|offer| fingerprint(&offer.release).as_deref() == Some(release))
            {
                return Ok(Some((provider, offer.identifier)));
            }
        }
        Ok(None)
    }
}

#[cfg(test)]
mod tests;
