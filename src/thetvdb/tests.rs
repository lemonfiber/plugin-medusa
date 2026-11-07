use std::sync::Arc;

use lemonfiber_fixtures::http::{Answer, Fake};
use lemonfiber_ports::http::Method;
use lemonfiber_ports::service::{CuratedSeason, CuratedTitle, Failure};
use serde_json::{json, Value};

use super::{SignIn, TheTvdb};

const BASE: &str = "https://api4.thetvdb.com/v4";

fn thetvdb(fake: &Arc<Fake>, pin: Option<&str>) -> TheTvdb {
    TheTvdb::new(
        fake.clone(),
        BASE,
        SignIn {
            key: "operator-key".to_owned(),
            pin: pin.map(str::to_owned),
        },
    )
}

fn json(status: u16, body: &Value) -> Answer {
    Answer::reply(status, body.to_string())
}

fn token(token: &str) -> Answer {
    json(
        200,
        &json!({ "status": "success", "data": { "token": token } }),
    )
}

fn aired(seasons: &[(u32, usize)]) -> Value {
    Value::Array(
        seasons
            .iter()
            .flat_map(|(season, count)| {
                (0..*count)
                    .map(move |number| json!({ "seasonNumber": season, "number": number + 1 }))
            })
            .collect(),
    )
}

fn page(year: &Value, episodes: &Value) -> Answer {
    json(
        200,
        &json!({ "status": "success", "data": { "series": { "id": 81189, "name": "Breaking Bad", "year": year }, "episodes": episodes } }),
    )
}

fn asked(fake: &Fake) -> Vec<(Method, String, Option<Value>, Option<String>)> {
    fake.requests()
        .into_iter()
        .map(|one| {
            let signed = one
                .headers
                .iter()
                .find(|(name, _)| name == "Authorization")
                .map(|(_, value)| value.clone());
            (
                one.method,
                one.url.trim_start_matches(BASE).to_owned(),
                one.body
                    .as_deref()
                    .and_then(|body| serde_json::from_str(body).ok()),
                signed,
            )
        })
        .collect()
}

fn season(number: u32, episodes: u32) -> CuratedSeason {
    CuratedSeason {
        number,
        monitored: false,
        files: 0,
        episodes,
    }
}

#[tokio::test]
async fn a_series_it_knows_is_named_with_the_episodes_of_each_season() {
    let fake = Fake::by_route_in_turn(vec![
        (Method::Post, "/login", vec![token("first")]),
        (
            Method::Get,
            "/series/81189/episodes/default",
            vec![page(&json!("2008"), &aired(&[(0, 2), (1, 7), (2, 13)]))],
        ),
    ]);
    assert_eq!(
        thetvdb(&fake, None).series(81189).await.ok(),
        Some(Some(CuratedTitle {
            reference: 81189,
            id: None,
            title: "Breaking Bad".to_owned(),
            year: Some(2008),
            monitored: false,
            files: 0,
            seasons: vec![season(0, 2), season(1, 7), season(2, 13)],
        }))
    );
    assert_eq!(
        asked(&fake),
        [
            (
                Method::Post,
                "/login".to_owned(),
                Some(json!({ "apikey": "operator-key" })),
                None
            ),
            (
                Method::Get,
                "/series/81189/episodes/default?page=0".to_owned(),
                None,
                Some("Bearer first".to_owned())
            ),
        ]
    );
}

#[tokio::test]
async fn a_key_needing_a_pin_signs_in_with_it_once_and_keeps_its_token() {
    let fake = Fake::by_route_in_turn(vec![
        (Method::Post, "/login", vec![token("first")]),
        (
            Method::Get,
            "/episodes/default",
            vec![page(&Value::Null, &json!([]))],
        ),
    ]);
    let thetvdb = thetvdb(&fake, Some("1234"));
    let named = thetvdb.series(81189).await.ok().flatten();
    assert_eq!(
        named.map(|named| (named.year, named.seasons)),
        Some((None, Vec::new()))
    );
    assert!(thetvdb.series(81189).await.is_ok());
    let signed_in: Vec<Option<Value>> = asked(&fake)
        .into_iter()
        .filter(|(method, _, _, _)| *method == Method::Post)
        .map(|(_, _, body, _)| body)
        .collect();
    assert_eq!(
        signed_in,
        [Some(json!({ "apikey": "operator-key", "pin": "1234" }))]
    );
}

#[tokio::test]
async fn a_token_it_refuses_is_traded_for_a_new_one_once() {
    let fake = Fake::by_route_in_turn(vec![
        (
            Method::Post,
            "/login",
            vec![token("first"), token("second")],
        ),
        (
            Method::Get,
            "/episodes/default",
            vec![
                page(&json!("2008"), &json!([])),
                json(401, &json!({ "status": "failure" })),
                page(&json!("2008"), &json!([])),
            ],
        ),
    ]);
    let thetvdb = thetvdb(&fake, None);
    assert!(thetvdb.series(81189).await.is_ok());
    assert!(thetvdb.series(81189).await.is_ok());
    let signed: Vec<Option<String>> = asked(&fake)
        .into_iter()
        .filter(|(method, _, _, _)| *method == Method::Get)
        .map(|(_, _, _, signed)| signed)
        .collect();
    assert_eq!(
        signed,
        [
            Some("Bearer first".to_owned()),
            Some("Bearer first".to_owned()),
            Some("Bearer second".to_owned()),
        ]
    );
}

#[tokio::test]
async fn a_series_with_more_episodes_than_a_page_is_read_page_by_page() {
    let fake = Fake::by_route_in_turn(vec![
        (Method::Post, "/login", vec![token("first")]),
        (
            Method::Get,
            "page=0",
            vec![page(&json!("1989"), &aired(&[(1, 500)]))],
        ),
        (
            Method::Get,
            "page=1",
            vec![page(&json!("1989"), &aired(&[(1, 3), (2, 497)]))],
        ),
        (
            Method::Get,
            "page=2",
            vec![json(404, &json!({ "status": "failure" }))],
        ),
    ]);
    assert_eq!(
        thetvdb(&fake, None)
            .series(71663)
            .await
            .ok()
            .flatten()
            .map(|named| named.seasons),
        Some(vec![season(1, 503), season(2, 497)])
    );
}

#[tokio::test]
async fn a_series_it_does_not_know_is_none() {
    let fake = Fake::by_route_in_turn(vec![
        (Method::Post, "/login", vec![token("first")]),
        (
            Method::Get,
            "/episodes/default",
            vec![json(404, &json!({ "status": "failure" }))],
        ),
    ]);
    assert_eq!(thetvdb(&fake, None).series(1).await.ok(), Some(None));
}

#[tokio::test]
async fn a_key_it_refuses_is_a_failure_and_no_series_is_asked() {
    let fake = Fake::by_route_in_turn(vec![(
        Method::Post,
        "/login",
        vec![json(401, &json!({ "status": "failure" }))],
    )]);
    assert!(matches!(
        thetvdb(&fake, None).series(81189).await,
        Err(Failure::Unauthorised { .. } | Failure::Refused { .. })
    ));
    assert_eq!(asked(&fake).len(), 1);
}

#[tokio::test]
async fn episodes_in_another_shape_are_a_failure() {
    let fake = Fake::by_route_in_turn(vec![
        (Method::Post, "/login", vec![token("first")]),
        (
            Method::Get,
            "/episodes/default",
            vec![json(200, &json!({ "data": [] }))],
        ),
    ]);
    assert!(thetvdb(&fake, None).series(81189).await.is_err());
}

#[test]
fn a_sign_in_shows_neither_its_key_nor_its_pin() {
    let shown = format!(
        "{:?}",
        SignIn {
            key: "operator-key".to_owned(),
            pin: Some("1234".to_owned()),
        }
    );
    assert!(
        !shown.contains("operator-key") && !shown.contains("1234"),
        "{shown}"
    );
}
