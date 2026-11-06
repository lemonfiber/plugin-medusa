use std::sync::Arc;
use std::time::Duration;

use lemonfiber_fixtures::http::{Answer, Fake};
use lemonfiber_ports::http::Method;
use lemonfiber_ports::media::Kind;
use lemonfiber_ports::service::{Failure, Record};
use serde_json::{json, Value};

use super::Medusa;

pub(super) const BASE: &str = "http://medusa:8081";

pub(super) const KEY: &str = "abcdefghijklmnop";

pub(super) const USERNAME: &str = "qrstuvwx";

pub(super) const PASSWORD: &str = "ABCDEFGHIJKLMNOP";

pub(super) struct Missing;

impl lemonfiber_adapter_kit::credential::Holding for Missing {
    fn key(&self) -> Result<String, String> {
        Err("upstream.json could not be read: not found".to_owned())
    }
}

pub(super) fn medusa(fake: &Arc<Fake>) -> Medusa {
    Medusa::new(
        fake.clone(),
        BASE,
        Arc::new(KEY.to_owned()),
        Arc::new(USERNAME.to_owned()),
        Arc::new(PASSWORD.to_owned()),
    )
    .pausing(Duration::ZERO)
}

pub(super) fn asked(fake: &Fake) -> Vec<(Method, String, Option<Value>)> {
    fake.requests()
        .into_iter()
        .map(|one| {
            (
                one.method,
                one.url.trim_start_matches(BASE).to_owned(),
                one.body
                    .as_deref()
                    .and_then(|body| serde_json::from_str(body).ok()),
            )
        })
        .collect()
}

pub(super) fn json(status: u16, body: &Value) -> Answer {
    Answer::reply(status, body.to_string())
}

pub(super) fn why<T>(result: Result<T, Failure>) -> String {
    match result {
        Ok(_) => "answered".to_owned(),
        Err(failure) => format!("{failure:?}"),
    }
}

pub(super) fn so(failure: Failure) -> String {
    why(Err::<(), _>(failure))
}

pub(super) fn unsupported(detail: &str) -> Failure {
    Failure::Unsupported {
        service: "medusa".to_owned(),
        detail: detail.to_owned(),
    }
}

pub(super) fn refused(detail: &str) -> Failure {
    Failure::Refused {
        service: "medusa".to_owned(),
        detail: detail.to_owned(),
    }
}

#[tokio::test]
async fn it_answers_while_its_sign_in_page_is_shown_to_a_caller_presenting_nothing() {
    let up = Fake::always(Answer::reply(200, "<!DOCTYPE html>"));
    assert!(medusa(&up).answers().await);
    assert_eq!(
        up.requests()
            .iter()
            .map(|one| (one.url.clone(), one.headers.is_empty()))
            .collect::<Vec<_>>(),
        [(format!("{BASE}/login/"), true)]
    );
    for down in [Fake::silent(), Fake::always(Answer::reply(302, ""))] {
        assert!(!medusa(&down).answers().await);
    }
}

#[tokio::test]
async fn every_call_to_its_api_carries_the_key_and_a_body_says_it_is_json() {
    let fake = Fake::always(json(200, &json!({ "rootDirs": ["0", "/tv"] })));
    assert!(medusa(&fake)
        .configure(&json!({ "rootDirs": ["0", "/tv"] }))
        .await
        .is_ok());
    let request = fake.request();
    assert_eq!(
        request.as_ref().map(|one| one.url.as_str()),
        Some("http://medusa:8081/api/v2/config/main")
    );
    assert_eq!(
        request.map(|one| one.headers),
        Some(vec![
            ("X-Api-Key".to_owned(), KEY.to_owned()),
            ("Content-Type".to_owned(), "application/json".to_owned()),
        ])
    );
}

#[tokio::test]
async fn a_setting_medusa_did_not_take_is_refused() {
    let fake = Fake::always(json(200, &json!({})));
    assert_eq!(
        why(medusa(&fake)
            .configure(&json!({ "rootDirs": ["0", "/tv"] }))
            .await),
        so(refused("Medusa did not take every setting it was given"))
    );
}

#[tokio::test]
async fn no_key_is_named_where_none_is_held_and_nothing_is_asked() {
    let fake = Fake::always(json(200, &json!([])));
    let held = Medusa::new(
        fake.clone(),
        BASE,
        Arc::new(Missing),
        Arc::new(USERNAME.to_owned()),
        Arc::new(PASSWORD.to_owned()),
    );
    assert_eq!(
        why(held.series().await),
        so(refused(
            "this adapter holds no key for Medusa: upstream.json could not be read: not found"
        ))
    );
    assert!(fake.requests().is_empty());
}

#[tokio::test]
async fn a_list_is_read_page_by_page_until_a_page_is_not_full() {
    let full: Vec<Value> = (0..1000)
        .map(|id| json!({ "title": id.to_string() }))
        .collect();
    let fake = Fake::by_path(vec![
        ("page=1", json(200, &Value::Array(full))),
        ("page=2", json(200, &json!([{ "title": "last" }]))),
    ]);
    let read = medusa(&fake).series().await.unwrap_or_default();
    assert_eq!(read.len(), 1001);
    assert_eq!(
        asked(&fake)
            .into_iter()
            .map(|(_, path, _)| path)
            .collect::<Vec<_>>(),
        [
            "/api/v2/series?limit=1000&page=1",
            "/api/v2/series?limit=1000&page=2"
        ]
    );
}

#[tokio::test]
async fn it_signs_in_with_the_account_and_carries_every_cookie_medusa_set() {
    let fake = Fake::always(Answer::Headed {
        status: 302,
        headers: vec![
            (
                "Set-Cookie".to_owned(),
                "medusa_user=\"signed\"; expires=Thu; Path=/".to_owned(),
            ),
            ("Location".to_owned(), "/home/".to_owned()),
            ("set-cookie".to_owned(), "_xsrf=token; Path=/".to_owned()),
        ],
        body: String::new(),
    });
    assert_eq!(
        medusa(&fake).signed_in().await.ok(),
        Some("medusa_user=\"signed\"; _xsrf=token".to_owned())
    );
    let request = fake.request();
    assert_eq!(
        request.as_ref().map(|one| (one.method, one.url.as_str())),
        Some((Method::Post, "http://medusa:8081/login/"))
    );
    assert_eq!(
        request.and_then(|one| one.body),
        Some(format!("username={USERNAME}&password={PASSWORD}"))
    );
}

#[tokio::test]
async fn a_sign_in_that_sets_no_cookie_is_a_refused_account() {
    let fake = Fake::always(Answer::reply(302, ""));
    assert_eq!(
        why(medusa(&fake).signed_in().await),
        so(Failure::Unauthorised {
            service: "medusa".to_owned()
        })
    );
}

#[tokio::test]
async fn no_account_held_is_named_and_nothing_is_asked() {
    let fake = Fake::always(Answer::reply(302, ""));
    for (username, password, missing) in [
        (
            Arc::new(Missing) as Arc<dyn lemonfiber_adapter_kit::credential::Holding>,
            Arc::new(PASSWORD.to_owned()) as Arc<dyn lemonfiber_adapter_kit::credential::Holding>,
            "account name",
        ),
        (Arc::new(USERNAME.to_owned()), Arc::new(Missing), "password"),
    ] {
        let held = Medusa::new(
            fake.clone(),
            BASE,
            Arc::new(KEY.to_owned()),
            username,
            password,
        );
        assert_eq!(
            why(held.signed_in().await),
            so(refused(&format!(
                "this adapter holds no {missing} for Medusa: upstream.json could not be read: not found"
            )))
        );
    }
    assert!(fake.requests().is_empty());
}

#[tokio::test]
async fn a_page_is_asked_with_the_session_and_its_words_are_read() {
    let fake = Fake::always(Answer::reply(200, "Success"));
    assert_eq!(
        medusa(&fake)
            .page("medusa_user=x", "/home/testTorrent")
            .await
            .ok(),
        Some("Success".to_owned())
    );
    assert_eq!(
        fake.request().map(|one| one.headers),
        Some(vec![("Cookie".to_owned(), "medusa_user=x".to_owned())])
    );
    let refusing = Fake::always(Answer::reply(302, ""));
    assert!(medusa(&refusing)
        .page("medusa_user=x", "/home/testTorrent")
        .await
        .is_err());
}

#[test]
fn it_files_series_and_follows_series_alone() {
    assert!(Medusa::filing(Kind::Tv).is_ok());
    assert_eq!(
        why(Medusa::filing(Kind::Movies)),
        so(unsupported("it files no film"))
    );
    assert!(Medusa::holding(Record::Series).is_ok());
    assert_eq!(
        why(Medusa::holding(Record::Film)),
        so(unsupported("it holds no films"))
    );
}
