use lemonfiber_fixtures::http::{Answer, Fake};
use lemonfiber_ports::http::Method;
use lemonfiber_ports::service::{Carried, Carrying as _, Record};
use serde_json::{json, Value};

use crate::medusa::tests::{asked, json, medusa, refused, so, unsupported, why};

fn presets() -> Answer {
    json(
        200,
        &json!({ "qualities": { "presets": [
        { "value": 6, "name": "SD" },
        { "value": 328, "name": "HD720p" },
    ] } }),
    )
}

fn followed() -> Value {
    json!([
        { "id": { "tvdb": 75932, "slug": "tvdb75932" }, "title": "Fawlty Towers",
          "config": { "rootDir": "/data/tv", "qualities": { "allowed": [2, 4], "preferred": [] } } },
        { "id": { "tvdb": 81189, "slug": "tvdb81189" }, "title": "Breaking Bad",
          "config": { "rootDir": "", "qualities": { "allowed": [8], "preferred": [32] } } },
        { "id": { "slug": "tvdb0" }, "title": "" },
        "not a series",
    ])
}

#[tokio::test]
async fn the_series_it_follows_are_carried_with_their_preset_and_folder_by_name() {
    let fake = Fake::by_path(vec![
        ("/config/consts", presets()),
        ("/series", json(200, &followed())),
    ]);
    let carried = medusa(&fake).records(Record::Series).await.ok();
    assert_eq!(
        carried.as_ref().map(|all| all
            .iter()
            .map(|one| (
                one.name.as_str(),
                one.profile.as_deref(),
                one.folder.as_deref()
            ))
            .collect::<Vec<_>>()),
        Some(vec![
            ("Fawlty Towers", Some("SD"), Some("/data/tv")),
            ("Breaking Bad", None, None),
        ])
    );
    assert_eq!(
        carried
            .and_then(|all| all.into_iter().next())
            .and_then(|one| serde_json::from_str::<Value>(&one.rest).ok())
            .and_then(|rest| rest.pointer("/id/tvdb").cloned()),
        Some(json!(75932))
    );
}

#[tokio::test]
async fn only_series_are_carried() {
    let fake = Fake::silent();
    assert_eq!(
        why(medusa(&fake).records(Record::Film).await),
        so(unsupported("it holds no films"))
    );
    assert_eq!(
        why(medusa(&fake)
            .carry(
                Record::Artist,
                &Carried {
                    name: String::new(),
                    profile: None,
                    folder: None,
                    rest: String::new()
                }
            )
            .await),
        so(unsupported("it holds no artists"))
    );
    assert!(fake.requests().is_empty());
}

fn carrying() -> std::sync::Arc<Fake> {
    Fake::by_route(vec![
        (Method::Get, "/config/consts", presets()),
        (
            Method::Get,
            "/config/main",
            json(
                200,
                &json!({ "rootDirs": ["0", "/data/tv"], "showDefaults": { "quality": 6 } }),
            ),
        ),
        (Method::Post, "/series", Answer::reply(201, "{}")),
    ])
}

fn posted(fake: &Fake) -> Option<Value> {
    asked(fake)
        .into_iter()
        .find(|(method, _, _)| *method == Method::Post)
        .and_then(|(_, _, body)| body)
}

#[tokio::test]
async fn a_series_from_sonarr_is_followed_by_its_thetvdb_id_at_the_preset_named() {
    let fake = carrying();
    assert!(medusa(&fake)
        .carry(
            Record::Series,
            &Carried {
                name: "Fawlty Towers".to_owned(),
                profile: Some("HD720p".to_owned()),
                folder: Some("/tv".to_owned()),
                rest: json!({ "tvdbId": 75932, "monitored": false, "seasonFolder": false })
                    .to_string(),
            },
        )
        .await
        .is_ok());
    assert_eq!(
        posted(&fake),
        Some(json!({
            "id": { "tvdb": 75932 },
            "options": {
                "rootDir": "/tv",
                "quality": { "allowed": [8, 64, 256], "preferred": [] },
                "paused": true,
                "seasonFolders": false,
            },
        }))
    );
}

#[tokio::test]
async fn a_series_from_medusa_is_followed_into_the_default_folder_at_the_default_quality() {
    let fake = carrying();
    assert!(medusa(&fake)
        .carry(
            Record::Series,
            &Carried {
                name: "Fawlty Towers".to_owned(),
                profile: None,
                folder: None,
                rest: json!({ "id": { "tvdb": null }, "externals": { "tvdb": 75932 }, "config": { "paused": false } }).to_string(),
            },
        )
        .await
        .is_ok());
    assert_eq!(
        posted(&fake),
        Some(json!({
            "id": { "tvdb": 75932 },
            "options": {
                "rootDir": "/data/tv",
                "quality": { "allowed": [2, 4], "preferred": [] },
                "paused": false,
                "seasonFolders": true,
            },
        }))
    );
}

#[tokio::test]
async fn a_series_with_no_thetvdb_id_an_unknown_preset_or_nowhere_to_go_is_refused() {
    let fake = carrying();
    assert_eq!(
        why(medusa(&fake)
            .carry(
                Record::Series,
                &Carried {
                    name: "conformance".to_owned(),
                    profile: None,
                    folder: None,
                    rest: String::new(),
                },
            )
            .await),
        so(refused("conformance carries no TheTVDB id to follow it by"))
    );
    assert_eq!(
        why(medusa(&fake)
            .carry(
                Record::Series,
                &Carried {
                    name: "x".to_owned(),
                    profile: Some("Ultra".to_owned()),
                    folder: None,
                    rest: json!({ "tvdbId": 1 }).to_string(),
                },
            )
            .await),
        so(refused("no quality preset called Ultra"))
    );
    let nowhere = Fake::always(json(200, &json!({ "rootDirs": [] })));
    assert_eq!(
        why(medusa(&nowhere)
            .carry(
                Record::Series,
                &Carried {
                    name: "x".to_owned(),
                    profile: None,
                    folder: None,
                    rest: json!({ "tvdbId": 1 }).to_string(),
                },
            )
            .await),
        so(refused("series has no root folder configured yet"))
    );
    assert!(posted(&fake).is_none());
}
