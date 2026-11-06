use std::sync::Arc;

use axum::body::Body;
use axum::http::{header, Request, StatusCode};
use lemonfiber_contract::adapter::{about_path, ready_path, About};
use lemonfiber_fixtures::http::{Answer, Fake};
use tower::ServiceExt as _;

use super::{about, kit, ADDRESS, DIGEST, RELEASE, UPSTREAM};
use crate::medusa::Medusa;

fn a_key() -> String {
    ('e'..='t').collect()
}

fn medusa(fake: &Arc<Fake>) -> Arc<Medusa> {
    Arc::new(Medusa::new(
        fake.clone(),
        ADDRESS,
        Arc::new(('a'..='p').collect::<String>()),
        Arc::new(('q'..='x').collect::<String>()),
        Arc::new(('A'..='P').collect::<String>()),
    ))
}

async fn answered_by(
    fake: &Arc<Fake>,
    method: &str,
    path: &str,
    key: Option<&str>,
    body: &str,
) -> (StatusCode, Vec<u8>) {
    let mut request = Request::builder().method(method).uri(path);
    if let Some(key) = key {
        request = request.header(header::AUTHORIZATION, format!("Bearer {key}"));
    }
    let Ok(request) = request.body(Body::from(body.to_owned())) else {
        return (StatusCode::IM_A_TEAPOT, Vec::new());
    };
    let Ok(response) = kit(a_key(), medusa(fake)).router().oneshot(request).await;
    let status = response.status();
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .map(|bytes| bytes.to_vec())
        .unwrap_or_default();
    (status, bytes)
}

fn kind_of(body: &[u8]) -> Option<serde_json::Value> {
    serde_json::from_slice::<serde_json::Value>(body)
        .ok()
        .and_then(|refusal| refusal.get("type").cloned())
}

#[test]
fn it_speaks_the_pull_and_the_curator_contracts_and_names_the_release_its_recordings_were_taken_from(
) {
    let said = about();
    assert_eq!(said.speaks, ["indexer.pull@1", "library.curate@1"]);
    assert_eq!(said.upstream, UPSTREAM);
    assert_eq!(
        said.releases
            .iter()
            .map(|release| (release.version.as_str(), release.digest.as_str()))
            .collect::<Vec<_>>(),
        [(RELEASE, DIGEST)]
    );
}

#[tokio::test]
async fn about_is_answered_under_the_plugin_s_key() {
    let key = a_key();
    let (status, body) = answered_by(&Fake::silent(), "GET", &about_path(), Some(&key), "").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(serde_json::from_slice::<About>(&body).ok(), Some(about()));
}

#[tokio::test]
async fn nothing_is_answered_without_the_key_or_under_another() {
    for key in [None, Some("not-the-plugin-s-key")] {
        for (method, path) in [
            ("POST", "/lemonfiber/indexer.pull/v1/aggregators".to_owned()),
            ("POST", "/lemonfiber/library.curate/v1/identity".to_owned()),
            ("GET", about_path()),
        ] {
            let fake = Fake::silent();
            assert_eq!(
                answered_by(&fake, method, &path, key, "{}").await.0,
                StatusCode::UNAUTHORIZED
            );
            assert!(fake.requests().is_empty());
        }
    }
}

#[tokio::test]
async fn an_unreachable_medusa_is_answered_as_not_answering() {
    let key = a_key();
    let (status, body) = answered_by(
        &Fake::silent(),
        "POST",
        "/lemonfiber/library.curate/v1/identity",
        Some(&key),
        "{}",
    )
    .await;
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(kind_of(&body), Some(serde_json::json!("unavailable")));
}

#[tokio::test]
async fn each_contract_is_answered_from_what_medusa_says() {
    let key = a_key();
    let fake = Fake::by_path(vec![
        ("/config/system/release", Answer::reply(200, "\"1.0.27\"")),
        (
            "/config/main",
            Answer::reply(200, r#"{"providers":{"prowlarr":{"url":"","apikey":""}}}"#),
        ),
        ("/providers", Answer::reply(200, "[]")),
    ]);
    let (status, body) = answered_by(
        &fake,
        "POST",
        "/lemonfiber/library.curate/v1/identity",
        Some(&key),
        "{}",
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        serde_json::from_slice::<serde_json::Value>(&body).ok(),
        Some(serde_json::json!({ "name": "Medusa", "version": "1.0.27" }))
    );
    let (status, body) = answered_by(
        &fake,
        "POST",
        "/lemonfiber/indexer.pull/v1/aggregators",
        Some(&key),
        "{}",
    )
    .await;
    assert_eq!(
        (status, body.as_slice()),
        (StatusCode::OK, b"[]".as_slice())
    );
}

#[tokio::test]
async fn a_capability_it_does_not_speak_is_refused_as_unknown() {
    let key = a_key();
    let (status, _) = answered_by(
        &Fake::silent(),
        "POST",
        "/lemonfiber/download.usenet/v1/transfers",
        Some(&key),
        "{}",
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn it_is_ready_while_medusa_shows_its_sign_in_page() {
    let key = a_key();
    let up = Fake::always(Answer::reply(200, "<!DOCTYPE html>"));
    let (status, body) = answered_by(&up, "GET", &ready_path(), Some(&key), "").await;
    assert_eq!(
        (status, body.as_slice()),
        (StatusCode::OK, b"true".as_slice())
    );
    assert!(up
        .requests()
        .iter()
        .all(|asked| asked.url == "http://medusa:8081/login/"));
    for down in [Fake::silent(), Fake::always(Answer::reply(503, ""))] {
        let (status, body) = answered_by(&down, "GET", &ready_path(), Some(&key), "").await;
        assert_eq!(
            (status, body.as_slice()),
            (StatusCode::OK, b"false".as_slice())
        );
    }
}
