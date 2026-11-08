use lemonfiber_fixtures::http::{Answer, Fake};
use lemonfiber_ports::digest::sha256;
use lemonfiber_ports::http::Method;
use lemonfiber_ports::media::Kind;
use lemonfiber_ports::service::{Fulfilling as _, Grabbed};
use serde_json::{json, Value};

use super::failed_name;
use crate::medusa::tests::{asked, json, medusa, refused, so, unsupported, why};

const RELEASE: &str = "Fawlty.Towers.S01E02.720p-GRP";

fn fingerprint() -> String {
    sha256(RELEASE)
}

fn series() -> Answer {
    json(
        200,
        &json!([{ "id": { "tvdb": 75932, "slug": "tvdb75932" }, "title": "Fawlty Towers", "config": {} }]),
    )
}

fn snatch(status: i64, episode: u32, resource: &str) -> Value {
    json!({ "status": status, "season": 1, "episode": episode, "resource": resource })
}

fn providers() -> Answer {
    json(
        200,
        &json!([
            { "id": "one", "config": { "enabled": true, "search": { "manual": { "enabled": true } } } },
            { "id": "two", "config": { "enabled": true, "search": { "manual": { "enabled": true } } } },
            { "id": "off", "config": { "enabled": false, "search": { "manual": { "enabled": true } } } },
        ]),
    )
}

fn searched(episode: &str, status: &str) -> Answer {
    json(
        200,
        &json!({ "results": [{ "episode": { "slug": episode }, "search": { "status": status } }] }),
    )
}

fn signed_in() -> Answer {
    Answer::Headed {
        status: 302,
        headers: vec![("Set-Cookie".to_owned(), "medusa_user=s; Path=/".to_owned())],
        body: String::new(),
    }
}

fn not_found() -> Answer {
    json(404, &json!({ "error": "Provider cache results not found" }))
}

struct Script {
    history: Value,
    failed: Value,
    searched: Answer,
    two: Answer,
    picked: Answer,
}

impl Default for Script {
    fn default() -> Self {
        Self {
            history: json!([snatch(4, 1, "Fawlty.Towers.S01E01"), snatch(2, 2, RELEASE)]),
            failed: json!([
                { "id": 3, "release": "Fawlty_Towers_S01E02_720p_GRP" },
                { "id": 4, "release": "Fawlty_Towers_S01E01" },
            ]),
            searched: searched("s01e02", "finished"),
            two: json(
                200,
                &json!([
                    { "identifier": "x", "release": "Fawlty.Towers.S01E02.1080p-GRP" },
                    { "identifier": "abc", "release": RELEASE },
                ]),
            ),
            picked: json(200, &json!({ "result": "success" })),
        }
    }
}

fn scripted(script: Script) -> std::sync::Arc<Fake> {
    Fake::by_rules(vec![
        (Some(Method::Get), "/series?", series()),
        (None, "/history/tvdb75932", json(200, &script.history)),
        (None, "/internal/getFailed", json(200, &script.failed)),
        (None, "/internal/removeFailed", json(200, &json!({}))),
        (
            Some(Method::Put),
            "/search/manual",
            Answer::reply(202, "\"started\""),
        ),
        (None, "/search/tvdb75932", script.searched),
        (None, "/providers/one/results", not_found()),
        (None, "/providers/two/results", script.two),
        (None, "/providers", providers()),
        (Some(Method::Post), "/login/", signed_in()),
        (None, "/home/pickManualSearch", script.picked),
    ])
}

fn paths(fake: &Fake) -> Vec<String> {
    asked(fake).into_iter().map(|(_, path, _)| path).collect()
}

#[tokio::test]
async fn a_release_is_lifted_from_the_failed_list_searched_for_by_its_episode_and_snatched_where_a_provider_offers_it(
) {
    let fake = scripted(Script::default());
    assert_eq!(
        medusa(&fake)
            .grab(Kind::Tv, 75932, &fingerprint())
            .await
            .ok(),
        Some(Grabbed::Grabbed)
    );
    let said = asked(&fake);
    assert!(said.contains(&(
        Method::Post,
        "/api/v2/internal/removeFailed".to_owned(),
        Some(json!({ "remove": [3] }))
    )));
    assert!(said.contains(&(
        Method::Put,
        "/api/v2/search/manual".to_owned(),
        Some(json!({ "showSlug": "tvdb75932", "episodes": ["s01e02"] }))
    )));
    let paths = paths(&fake);
    assert!(paths.contains(&"/api/v2/internal/getFailed?limit=0".to_owned()));
    assert!(paths.contains(
        &"/api/v2/providers/two/results?showslug=tvdb75932&season=1&limit=1000".to_owned()
    ));
    assert!(!fake.asked_for("/providers/off/results"));
    assert_eq!(
        paths.last().map(String::as_str),
        Some("/home/pickManualSearch?provider=two&identifier=abc")
    );
}

#[tokio::test]
async fn a_release_snatched_for_several_episodes_is_searched_for_by_its_season() {
    let fake = scripted(Script {
        history: json!([
            snatch(2, 1, RELEASE),
            snatch(2, 2, RELEASE),
            snatch(2, 2, RELEASE)
        ]),
        searched: searched("S01E01", "finished"),
        two: not_found(),
        ..Script::default()
    });
    assert_eq!(
        medusa(&fake)
            .grab(Kind::Tv, 75932, &fingerprint())
            .await
            .ok(),
        Some(Grabbed::Gone)
    );
    assert!(asked(&fake).contains(&(
        Method::Put,
        "/api/v2/search/manual".to_owned(),
        Some(json!({ "showSlug": "tvdb75932", "season": ["s01"] }))
    )));
    assert!(!fake.asked_for("/login/"));
}

#[tokio::test]
async fn a_release_its_history_never_snatched_is_gone_and_nothing_is_lifted_or_searched() {
    for history in [
        json!([snatch(4, 2, RELEASE), snatch(2, 2, "Other")]),
        json!([snatch(2, 2, "")]),
    ] {
        let fake = scripted(Script {
            history,
            ..Script::default()
        });
        assert_eq!(
            medusa(&fake)
                .grab(Kind::Tv, 75932, &fingerprint())
                .await
                .ok(),
            Some(Grabbed::Gone)
        );
        assert!(!fake.asked_for("/internal/"));
        assert!(!fake.asked_for("/search/"));
    }
}

#[tokio::test]
async fn a_release_on_no_failed_list_lifts_nothing_and_one_no_provider_offers_is_gone() {
    let fake = scripted(Script {
        failed: json!([{ "id": 4, "release": "Fawlty_Towers_S01E01" }]),
        two: json(200, &json!([{ "identifier": "x", "release": "Other" }])),
        ..Script::default()
    });
    assert_eq!(
        medusa(&fake)
            .grab(Kind::Tv, 75932, &fingerprint())
            .await
            .ok(),
        Some(Grabbed::Gone)
    );
    assert!(!fake.asked_for("/internal/removeFailed"));
    assert!(!fake.asked_for("/login/"));
}

#[tokio::test]
async fn a_search_that_does_not_finish_in_time_or_a_snatch_medusa_does_not_take_is_refused() {
    let fake = scripted(Script {
        searched: searched("s01e02", "searching"),
        ..Script::default()
    });
    assert_eq!(
        why(medusa(&fake).grab(Kind::Tv, 75932, &fingerprint()).await),
        so(refused(
            "Medusa's search for the release did not finish in time"
        ))
    );
    for picked in [
        json(200, &json!({ "result": "failure" })),
        Answer::reply(200, "<html>Error</html>"),
    ] {
        let fake = scripted(Script {
            picked,
            ..Script::default()
        });
        assert_eq!(
            why(medusa(&fake).grab(Kind::Tv, 75932, &fingerprint()).await),
            so(refused(
                "Medusa did not snatch the release a provider offered"
            ))
        );
    }
}

#[tokio::test]
async fn a_failed_list_or_provider_medusa_cannot_answer_is_a_failure() {
    for broken in ["/internal/getFailed", "/providers/two/results"] {
        let script = Script::default();
        let fake = Fake::by_rules(vec![
            (None, broken, Answer::reply(500, "")),
            (Some(Method::Get), "/series?", series()),
            (None, "/history/tvdb75932", json(200, &script.history)),
            (None, "/internal/getFailed", json(200, &script.failed)),
            (None, "/internal/removeFailed", json(200, &json!({}))),
            (
                Some(Method::Put),
                "/search/manual",
                Answer::reply(202, "\"started\""),
            ),
            (None, "/search/tvdb75932", script.searched),
            (None, "/providers/one/results", not_found()),
            (None, "/providers", providers()),
        ]);
        assert!(
            medusa(&fake)
                .grab(Kind::Tv, 75932, &fingerprint())
                .await
                .is_err(),
            "{broken}"
        );
    }
}

#[tokio::test]
async fn a_series_it_does_not_hold_is_refused_before_anything_is_lifted_or_searched() {
    let fake = scripted(Script::default());
    assert_eq!(
        why(medusa(&fake).grab(Kind::Tv, 7, &fingerprint()).await),
        so(refused("Medusa holds no series with TheTVDB id 7"))
    );
    assert!(!fake.asked_for("/history/"));
}

#[tokio::test]
async fn medusa_takes_no_download_out_of_its_client_and_files_no_films() {
    let fake = Fake::silent();
    let medusa = medusa(&fake);
    assert_eq!(
        why(medusa.unqueue(Kind::Tv, 1, true).await),
        so(unsupported(
            "Medusa cannot take a download out of the client holding it"
        ))
    );
    let film = || so(unsupported("it files no film"));
    assert_eq!(why(medusa.unqueue(Kind::Movies, 1, false).await), film());
    assert_eq!(why(medusa.grab(Kind::Movies, 1, "f").await), film());
    assert!(fake.requests().is_empty());
}

#[test]
fn a_release_is_named_on_the_failed_list_as_medusa_names_it() {
    assert_eq!(failed_name("Show.S01E01-GRP.nzb"), "Show_S01E01_GRP");
    assert_eq!(failed_name("Café Ü 1"), "Café_Ü_1");
    assert_eq!(failed_name("Show_S01.nzb.mkv"), "Show_S01_nzb_mkv");
}
