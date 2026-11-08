use lemonfiber_fixtures::http::Fake;
use lemonfiber_ports::digest::sha256;
use lemonfiber_ports::media::Kind;
use lemonfiber_ports::service::stage::{Outcome, Stage};
use lemonfiber_ports::service::{
    FoundItem, ItemPart, Pipeline as _, QueueItem, StuckItem, TraceEvent,
};
use serde_json::{json, Value};

use crate::medusa::tests::{json, medusa, so, unsupported, why};

fn series() -> Value {
    json!([
        { "id": { "tvdb": 75932, "slug": "tvdb75932" }, "title": "Fawlty Towers", "year": { "start": 1975 }, "config": { "paused": false } },
        { "id": { "tvdb": null, "slug": "tmdb2207" }, "externals": { "tvdb": 81189 }, "title": "Breaking Bad", "year": { "start": 0 }, "config": { "paused": true } },
        { "id": { "slug": "tvmaze1" }, "title": "Nowhere" },
    ])
}

fn episodes() -> Value {
    json!([
        { "id": { "tvdb": 180_445 }, "season": 1, "episode": 1, "title": "A Touch of Class", "status": "Downloaded", "file": { "location": "/data/tv/a.mkv" } },
        { "id": { "tvdb": 180_446 }, "season": 1, "episode": 2, "title": "The Builders", "status": "Wanted", "file": { "location": "" } },
        { "id": { "tvdb": 180_447 }, "season": 1, "episode": 3, "title": "The Wedding Party", "status": "Skipped" },
        { "id": {}, "season": 1, "episode": 4, "title": "Unnumbered", "status": "Ignored" },
    ])
}

fn history_row(
    status: i64,
    episode: u32,
    client: Option<&[i64]>,
    slug: &str,
    title: &str,
) -> Value {
    json!({
        "status": status,
        "actionDate": 20_261_106_193_809_i64,
        "resource": "release",
        "season": 1,
        "episode": episode,
        "infoHash": "hash",
        "showSlug": slug,
        "showTitle": title,
        "clientStatus": client.map(|bits| json!({ "status": bits })),
    })
}

fn tracing() -> std::sync::Arc<Fake> {
    Fake::by_path(vec![
        ("/episodes", json(200, &episodes())),
        (
            "/history/tvdb75932",
            json(
                200,
                &json!([
                    history_row(4, 1, None, "tvdb75932", "Fawlty Towers"),
                    history_row(2, 2, Some(&[2]), "tvdb75932", "Fawlty Towers"),
                    history_row(11, 9, None, "tvdb75932", "Fawlty Towers"),
                    history_row(10, 1, None, "tvdb75932", "Fawlty Towers"),
                    { "status": 2, "actionDate": "yesterday", "season": 1, "episode": 3 },
                    { "status": 2, "season": 1, "episode": 3 },
                ]),
            ),
        ),
        (
            "/history?",
            json(
                200,
                &json!([
                    history_row(2, 2, Some(&[2]), "tvdb75932", "Fawlty Towers"),
                    history_row(2, 3, Some(&[16]), "tvdb75932", "Fawlty Towers"),
                    history_row(2, 1, Some(&[1]), "tmdb2207", "Breaking Bad"),
                    history_row(2, 2, Some(&[1]), "tmdb2207", "Breaking Bad"),
                    history_row(2, 3, Some(&[16]), "tvmaze1", ""),
                ]),
            ),
        ),
        ("/series", json(200, &series())),
    ])
}

#[tokio::test]
async fn the_series_it_follows_are_found_by_their_thetvdb_id() {
    let fake = tracing();
    assert_eq!(
        medusa(&fake).library(Kind::Tv).await.ok(),
        Some(vec![
            FoundItem {
                id: 75932,
                title: "Fawlty Towers".to_owned(),
                year: Some(1975),
                monitored: true,
            },
            FoundItem {
                id: 81189,
                title: "Breaking Bad".to_owned(),
                year: None,
                monitored: false,
            },
        ])
    );
    assert_eq!(
        medusa(&fake)
            .find_items(Kind::Tv, "fawlty")
            .await
            .ok()
            .map(|found| found.into_iter().map(|one| one.id).collect::<Vec<_>>()),
        Some(vec![75932])
    );
}

#[tokio::test]
async fn films_are_not_traced() {
    let fake = Fake::silent();
    let medusa = medusa(&fake);
    let film = || so(unsupported("it files no film"));
    assert_eq!(why(medusa.library(Kind::Movies).await), film());
    assert_eq!(why(medusa.item_history(Kind::Movies, 1).await), film());
    assert_eq!(why(medusa.item_queue(Kind::Movies, 1).await), film());
    assert_eq!(why(medusa.item_parts(Kind::Movies, 1, None).await), film());
    assert_eq!(why(medusa.stuck_items(Kind::Movies).await), film());
    assert!(fake.requests().is_empty());
}

#[tokio::test]
async fn a_series_it_does_not_follow_has_no_history_queue_or_parts() {
    let fake = tracing();
    let medusa = medusa(&fake);
    assert_eq!(
        medusa.item_history(Kind::Tv, 0).await.ok(),
        Some(Vec::new())
    );
    assert_eq!(medusa.item_queue(Kind::Tv, 0).await.ok(), Some(Vec::new()));
    assert_eq!(
        medusa.item_parts(Kind::Tv, 0, None).await.ok(),
        Some(Vec::new())
    );
    assert!(!fake.asked_for("/history"));
    assert!(!fake.asked_for("/episodes"));
}

#[tokio::test]
async fn the_history_of_a_series_names_each_grab_download_and_failure_by_its_episode() {
    let fake = tracing();
    assert_eq!(
        medusa(&fake).item_history(Kind::Tv, 75932).await.ok(),
        Some(vec![
            TraceEvent {
                outcome: Outcome::Imported,
                at: "20261106193809".to_owned(),
                part: Some(180_445),
            },
            TraceEvent {
                outcome: Outcome::Grabbed,
                at: "20261106193809".to_owned(),
                part: Some(180_446),
            },
            TraceEvent {
                outcome: Outcome::DownloadFailed,
                at: "20261106193809".to_owned(),
                part: None,
            },
            TraceEvent {
                outcome: Outcome::Grabbed,
                at: "yesterday".to_owned(),
                part: Some(180_447),
            },
            TraceEvent {
                outcome: Outcome::Grabbed,
                at: String::new(),
                part: Some(180_447),
            },
        ])
    );
    assert!(fake.asked_for("/history/tvdb75932?limit=100&sort="));
}

#[tokio::test]
async fn what_is_in_flight_for_a_series_is_named_by_its_episode() {
    let fake = tracing();
    assert_eq!(
        medusa(&fake).item_queue(Kind::Tv, 75932).await.ok(),
        Some(vec![
            QueueItem {
                part: Some(180_446),
                stage: Stage::Downloading,
                stuck: false,
            },
            QueueItem {
                part: Some(180_447),
                stage: Stage::Downloading,
                stuck: true,
            },
        ])
    );
    let idle = Fake::by_path(vec![
        ("/history?", json(200, &json!([]))),
        ("/series", json(200, &series())),
    ]);
    assert_eq!(
        medusa(&idle).item_queue(Kind::Tv, 75932).await.ok(),
        Some(Vec::new())
    );
    assert!(!idle.asked_for("/episodes"));
}

#[tokio::test]
async fn the_episodes_of_a_series_are_its_parts() {
    let fake = tracing();
    assert_eq!(
        medusa(&fake)
            .item_parts(Kind::Tv, 75932, Some(1))
            .await
            .ok(),
        Some(vec![
            ItemPart {
                id: 180_445,
                season: 1,
                number: 1,
                title: "A Touch of Class".to_owned(),
                monitored: true,
                has_file: true,
            },
            ItemPart {
                id: 180_446,
                season: 1,
                number: 2,
                title: "The Builders".to_owned(),
                monitored: true,
                has_file: false,
            },
            ItemPart {
                id: 180_447,
                season: 1,
                number: 3,
                title: "The Wedding Party".to_owned(),
                monitored: false,
                has_file: false,
            },
        ])
    );
    assert!(fake.asked_for("/series/tvdb75932/episodes?season=1&limit=1000&page=1"));
}

#[tokio::test]
async fn each_series_stuck_in_flight_is_named_once() {
    let fake = tracing();
    assert_eq!(
        medusa(&fake).stuck_items(Kind::Tv).await.ok(),
        Some(vec![
            StuckItem {
                title: "Fawlty Towers".to_owned(),
                stage: Stage::Downloading,
                queued: None,
                title_id: Some(75932),
                release: Some(sha256("release")),
            },
            StuckItem {
                title: "Breaking Bad".to_owned(),
                stage: Stage::Downloading,
                queued: None,
                title_id: Some(81189),
                release: Some(sha256("release")),
            },
        ])
    );
}

#[tokio::test]
async fn a_stuck_download_with_no_release_name_or_series_held_names_neither() {
    let fake = Fake::by_path(vec![
        (
            "/history?",
            json(
                200,
                &json!([{ "status": 2, "season": 1, "episode": 1, "showSlug": "tvdb1", "showTitle": "Gone", "clientStatus": { "status": [16] } }]),
            ),
        ),
        ("/series", json(200, &series())),
    ]);
    assert_eq!(
        medusa(&fake).stuck_items(Kind::Tv).await.ok().map(|all| all
            .into_iter()
            .map(|one| (one.title_id, one.release))
            .collect::<Vec<_>>()),
        Some(vec![(None, None)])
    );
}

#[tokio::test]
async fn with_nothing_stuck_no_series_is_read() {
    let fake = Fake::by_path(vec![("/history?", json(200, &json!([])))]);
    assert_eq!(
        medusa(&fake).stuck_items(Kind::Tv).await.ok(),
        Some(Vec::new())
    );
    assert!(!fake.asked_for("/series"));
}
