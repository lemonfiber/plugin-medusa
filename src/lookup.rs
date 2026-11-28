//! Where a series Medusa does not hold is looked up by its TheTVDB id, where the operator
//! switched a lookup on.

use std::collections::BTreeMap;

use async_trait::async_trait;
use lemonfiber_ports::service::{CuratedSeason, CuratedTitle, Failure};

/// A catalogue beyond the machine that names a series by its TheTVDB id.
#[async_trait]
pub trait Lookup: Send + Sync {
    /// The series the catalogue knows as `id`, with the episodes of each of its seasons,
    /// or none where it knows none.
    ///
    /// # Errors
    ///
    /// Returns [`Failure`] when the catalogue is unreachable, refuses the adapter, or
    /// answers in a shape that cannot tell which series it is.
    async fn series(&self, id: i64) -> Result<Option<CuratedTitle>, Failure>;
}

const YEAR_DIGITS: usize = 4;

/// The series `reference`, called `title`, first out in the year `dated` begins with and
/// holding the episodes `seasons` counts of each season, as nothing Medusa holds.
pub(crate) fn unheld(
    reference: i64,
    title: String,
    dated: Option<&str>,
    seasons: BTreeMap<u32, u32>,
) -> CuratedTitle {
    CuratedTitle {
        reference,
        id: None,
        title,
        year: dated
            .and_then(|dated| dated.get(..YEAR_DIGITS))
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
    }
}
