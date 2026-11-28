//! lemonfiber's adapter for Medusa: the `indexer.pull` and `library.curate` contracts,
//! spoken in front of one Medusa and translated to its own API, with TVmaze or TheTVDB asked
//! for a series Medusa does not hold where the operator switched that on.

pub mod adapter;
pub mod lookup;
pub mod medusa;
pub mod thetvdb;
pub mod tvmaze;
