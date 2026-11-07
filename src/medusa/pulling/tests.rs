use lemonfiber_fixtures::http::{Answer, Fake};
use lemonfiber_ports::http::Method;
use lemonfiber_ports::service::{AggregatorAt, KnownAggregator, Protocol, PullsIndexers as _};
use serde_json::{json, Value};

use crate::medusa::tests::{asked, json, medusa, refused, so, unsupported, why};

fn aggregator(protocol: &str, url: &str) -> AggregatorAt {
    AggregatorAt {
        name: "Prowlarr".to_owned(),
        url: url.to_owned(),
        protocol: Protocol(protocol.to_owned()),
        key: "the-aggregator-key".to_owned(),
    }
}

fn provider(
    id: &str,
    sub_type: &str,
    manager: &Value,
    url: &str,
    apikey: &str,
    default: bool,
) -> Value {
    json!({
        "id": id,
        "subType": sub_type,
        "manager": manager,
        "default": default,
        "config": {
            "enabled": false,
            "url": url,
            "apikey": apikey,
            "search": { "backlog": { "enabled": false }, "manual": { "enabled": false }, "daily": { "enabled": false } },
        },
    })
}

#[tokio::test]
async fn it_pulls_from_its_prowlarr_and_from_every_newznab_or_torznab_provider_of_its_own() {
    let fake = Fake::by_call(vec![
        (
            "/config/main",
            None,
            vec![json(
                200,
                &json!({ "providers": { "prowlarr": { "url": "http://prowlarr:9696", "apikey": "k" } } }),
            )],
        ),
        ("/providers/prowlarr/operation", Some("TEST"), connected()),
        (
            "/providers/prowlarr/operation",
            Some("GETINDEXERS"),
            vec![enabled(&[3])],
        ),
        (
            "/providers",
            None,
            vec![json(
                200,
                &json!([
                    provider(
                        "hydra",
                        "newznab",
                        &Value::Null,
                        "http://nzbhydra2:5076",
                        "k",
                        false
                    ),
                    provider(
                        "tracker",
                        "torznab",
                        &json!("prowlarr"),
                        "http://prowlarr:9696/3/api",
                        "k",
                        false
                    ),
                    provider(
                        "geek",
                        "newznab",
                        &Value::Null,
                        "https://api.nzbgeek.info/",
                        "",
                        true
                    ),
                    provider("rss", "torrentrss", &Value::Null, "http://rss/", "", false),
                    provider(
                        "keyless",
                        "torznab",
                        &Value::Null,
                        "http://jackett/",
                        "",
                        false
                    ),
                ]),
            )],
        ),
    ]);
    assert_eq!(
        medusa(&fake).aggregators().await.ok(),
        Some(vec![
            KnownAggregator {
                id: "prowlarr".to_owned(),
                url: "http://prowlarr:9696".to_owned(),
                keyed: true,
            },
            KnownAggregator {
                id: "hydra".to_owned(),
                url: "http://nzbhydra2:5076".to_owned(),
                keyed: true,
            },
            KnownAggregator {
                id: "keyless".to_owned(),
                url: "http://jackett/".to_owned(),
                keyed: false,
            },
        ])
    );
    let fresh = Fake::by_path(vec![
        (
            "/config/main",
            json(
                200,
                &json!({ "providers": { "prowlarr": { "url": "", "apikey": "" } } }),
            ),
        ),
        ("/providers", json(200, &json!([]))),
    ]);
    assert_eq!(medusa(&fresh).aggregators().await.ok(), Some(Vec::new()));
}

fn pulling(indexers: Answer) -> std::sync::Arc<Fake> {
    Fake::by_call(vec![
        (
            "/config/main",
            None,
            vec![json(
                200,
                &json!({ "providers": { "prowlarr": { "url": "http://prowlarr:9696/", "apikey": "the-aggregator-key" } } }),
            )],
        ),
        (
            "/providers/prowlarr/operation",
            Some("TEST"),
            vec![json(200, &json!("connected"))],
        ),
        (
            "/providers/prowlarr/operation",
            Some("GETINDEXERS"),
            vec![indexers],
        ),
        (
            "/providers/prowlarr",
            Some("\"id\":4"),
            vec![json(201, &json!({ "result": { "id": "usenet_four" } }))],
        ),
        (
            "/providers/prowlarr",
            Some("\"id\":5"),
            vec![json(201, &json!({ "result": { "id": "torrents_five" } }))],
        ),
        (
            "/providers",
            None,
            vec![
                json(
                    200,
                    &json!([provider(
                        "held",
                        "newznab",
                        &json!("prowlarr"),
                        "http://prowlarr:9696/3/api",
                        "k",
                        false
                    ),]),
                ),
                json(
                    200,
                    &json!([
                        provider(
                            "held",
                            "newznab",
                            &json!("prowlarr"),
                            "http://prowlarr:9696/3/api",
                            "k",
                            false
                        ),
                        provider(
                            "usenet_four",
                            "newznab",
                            &json!("prowlarr"),
                            "http://prowlarr:9696/4/api",
                            "k",
                            false
                        ),
                        provider(
                            "torrents_five",
                            "torznab",
                            &json!("prowlarr"),
                            "http://prowlarr:9696/5/api",
                            "k",
                            false
                        ),
                    ]),
                ),
                json(201, &json!({})),
            ],
        ),
    ])
}

#[tokio::test]
async fn prowlarr_is_linked_and_each_indexer_it_holds_not_yet_here_is_added_and_switched_on() {
    let fake = pulling(json(
        200,
        &json!([
            { "id": 3, "name": "Three", "protocol": "usenet", "enable": true },
            { "id": 4, "name": "Four", "protocol": "usenet", "enable": true },
            { "id": 5, "name": "Five", "protocol": "torrent", "enable": true },
            { "id": 6, "name": "Six", "protocol": "usenet", "enable": false },
        ]),
    ));
    assert!(medusa(&fake)
        .add_aggregator(&aggregator("servarr", "http://prowlarr:9696/"))
        .await
        .is_ok());
    let calls = asked(&fake);
    assert_eq!(
        calls.first(),
        Some(&(
            Method::Patch,
            "/api/v2/config/main".to_owned(),
            Some(
                json!({ "providers": { "prowlarr": { "url": "http://prowlarr:9696/", "apikey": "the-aggregator-key" } } })
            )
        ))
    );
    let made: Vec<Value> = calls
        .iter()
        .filter(|(_, path, _)| path == "/api/v2/providers/prowlarr")
        .filter_map(|(_, _, body)| body.clone())
        .collect();
    assert_eq!(
        made,
        [
            json!({ "subType": "newznab", "id": 4, "name": "Four" }),
            json!({ "subType": "torznab", "id": 5, "name": "Five" }),
        ]
    );
    let saved = calls
        .iter()
        .rev()
        .find(|(method, path, _)| *method == Method::Post && path == "/api/v2/providers")
        .and_then(|(_, _, body)| body.clone());
    let switched: Vec<(String, bool, bool)> = saved
        .and_then(|body| body.get("providers").cloned())
        .and_then(|all| all.as_array().cloned())
        .unwrap_or_default()
        .iter()
        .map(|one| {
            (
                one.get("id")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_owned(),
                one.pointer("/config/enabled").and_then(Value::as_bool) == Some(true),
                one.pointer("/config/search/manual/enabled")
                    .and_then(Value::as_bool)
                    == Some(true),
            )
        })
        .collect();
    assert_eq!(
        switched,
        [
            ("held".to_owned(), false, false),
            ("usenet_four".to_owned(), true, true),
            ("torrents_five".to_owned(), true, true),
        ]
    );
}

#[tokio::test]
async fn a_prowlarr_with_no_indexer_is_linked_and_nothing_more_is_written() {
    let fake = pulling(json(500, &json!({ "error": "Internal Server Error" })));
    assert!(medusa(&fake)
        .add_aggregator(&aggregator("servarr", "http://prowlarr:9696/"))
        .await
        .is_ok());
    assert!(!asked(&fake)
        .iter()
        .any(|(method, path, _)| *method == Method::Post && path == "/api/v2/providers"));
}

#[tokio::test]
async fn a_prowlarr_medusa_cannot_reach_is_refused_saying_where() {
    let fake = Fake::by_call(vec![
        (
            "/config/main",
            None,
            vec![json(
                200,
                &json!({ "providers": { "prowlarr": { "url": "http://prowlarr:9696/", "apikey": "the-aggregator-key" } } }),
            )],
        ),
        (
            "/providers/prowlarr/operation",
            Some("TEST"),
            vec![json(
                404,
                &json!({ "error": "Could not connect to prowlarr" }),
            )],
        ),
    ]);
    assert_eq!(
        why(medusa(&fake)
            .add_aggregator(&aggregator("servarr", "http://prowlarr:9696/"))
            .await),
        so(refused(
            "Medusa could not reach Prowlarr at http://prowlarr:9696/"
        ))
    );
}

#[tokio::test]
async fn nzbhydra2_is_added_as_a_newznab_provider_and_switched_on() {
    let fake = Fake::by_call(vec![
        (
            "/providers/newznab",
            None,
            vec![json(201, &json!({ "result": { "id": "prowlarr" } }))],
        ),
        (
            "/providers",
            None,
            vec![
                json(200, &json!([])),
                json(
                    200,
                    &json!([provider(
                        "prowlarr",
                        "newznab",
                        &Value::Null,
                        "http://nzbhydra2:5076",
                        "k",
                        false
                    )]),
                ),
                json(201, &json!({})),
            ],
        ),
    ]);
    assert!(medusa(&fake)
        .add_aggregator(&aggregator("nzbhydra2", "http://nzbhydra2:5076"))
        .await
        .is_ok());
    assert_eq!(
        asked(&fake).get(1),
        Some(&(
            Method::Post,
            "/api/v2/providers/newznab".to_owned(),
            Some(
                json!({ "name": "Prowlarr", "url": "http://nzbhydra2:5076", "apikey": "the-aggregator-key" })
            )
        ))
    );
    let made = Fake::always(json(201, &json!({ "unexpected": true })));
    assert!(medusa(&made)
        .add_aggregator(&aggregator("nzbhydra2", "http://nzbhydra2:5076"))
        .await
        .is_err());
}

#[tokio::test]
async fn an_aggregator_read_in_another_api_is_unsupported_and_nothing_is_asked() {
    let fake = Fake::silent();
    assert_eq!(
        why(medusa(&fake)
            .add_aggregator(&aggregator("jackett", "http://jackett"))
            .await),
        so(unsupported(
            "it pulls its indexers from Prowlarr, read in the servarr API, and from NZBHydra2"
        ))
    );
    assert!(fake.requests().is_empty());
}

#[tokio::test]
async fn a_provider_medusa_read_back_from_its_settings_is_still_its_own() {
    let fake = Fake::by_call(vec![
        ("/config/main", None, vec![json(200, &json!({}))]),
        (
            "/providers",
            None,
            vec![json(
                200,
                &json!([provider(
                    "hydra",
                    "newznab",
                    &json!(""),
                    "http://nzbhydra2:5076",
                    "k",
                    false
                )]),
            )],
        ),
    ]);
    assert_eq!(
        known_ids(medusa(&fake).aggregators().await.ok()),
        Some(vec!["hydra".to_owned()])
    );
}

#[tokio::test]
async fn a_provider_medusa_brought_back_with_no_address_is_nothing_it_pulls_from() {
    let fake = Fake::by_call(vec![
        ("/config/main", None, vec![json(200, &json!({}))]),
        (
            "/providers",
            None,
            vec![json(
                200,
                &json!([provider("gone", "newznab", &json!(""), "", "", false)]),
            )],
        ),
    ]);
    assert_eq!(
        known_ids(medusa(&fake).aggregators().await.ok()),
        Some(Vec::new())
    );
}

fn linked() -> Answer {
    json(
        200,
        &json!({ "providers": { "prowlarr": { "url": "http://prowlarr:9696", "apikey": "the-aggregator-key" } } }),
    )
}

fn holding(ids: &[(&str, u16)], key: &str) -> Value {
    Value::Array(
        ids.iter()
            .map(|(id, indexer)| {
                provider(
                    id,
                    "newznab",
                    &json!("prowlarr"),
                    &format!("http://prowlarr:9696/{indexer}/api"),
                    key,
                    false,
                )
            })
            .collect(),
    )
}

fn checking(indexers: Answer, tests: Vec<Answer>, providers: &Value) -> std::sync::Arc<Fake> {
    Fake::by_call(vec![
        ("/config/main", None, vec![linked()]),
        ("/providers/prowlarr/operation", Some("TEST"), tests),
        (
            "/providers/prowlarr/operation",
            Some("GETINDEXERS"),
            vec![indexers],
        ),
        ("/providers", None, vec![json(200, providers)]),
    ])
}

fn connected() -> Vec<Answer> {
    vec![json(200, &json!("connected"))]
}

fn enabled(ids: &[i64]) -> Answer {
    json(
        200,
        &Value::Array(
            ids.iter()
                .map(|id| json!({ "id": id, "name": "n", "protocol": "usenet", "enable": true }))
                .collect(),
        ),
    )
}

fn known_ids(found: Option<Vec<KnownAggregator>>) -> Option<Vec<String>> {
    found.map(|all| all.into_iter().map(|one| one.id).collect())
}

#[tokio::test]
async fn a_prowlarr_medusa_is_level_with_is_answered_as_read_from() {
    let fake = checking(
        enabled(&[3]),
        connected(),
        &holding(&[("three", 3)], "the-aggregator-key"),
    );
    assert_eq!(
        known_ids(medusa(&fake).aggregators().await.ok()),
        Some(vec!["prowlarr".to_owned()])
    );
}

#[tokio::test]
async fn a_prowlarr_holding_an_indexer_medusa_does_not_is_not_answered_as_read_from() {
    let fake = checking(
        enabled(&[3, 4]),
        connected(),
        &holding(&[("three", 3)], "the-aggregator-key"),
    );
    assert_eq!(
        known_ids(medusa(&fake).aggregators().await.ok()),
        Some(Vec::new())
    );
}

#[tokio::test]
async fn a_prowlarr_no_longer_holding_an_indexer_medusa_does_is_not_answered_as_read_from() {
    let fake = checking(
        enabled(&[3]),
        connected(),
        &holding(&[("three", 3), ("gone", 9)], "the-aggregator-key"),
    );
    assert_eq!(
        known_ids(medusa(&fake).aggregators().await.ok()),
        Some(Vec::new())
    );
}

#[tokio::test]
async fn a_provider_holding_a_key_prowlarr_no_longer_answers_is_not_answered_as_read_from() {
    let fake = checking(enabled(&[3]), connected(), &holding(&[("three", 3)], "old"));
    assert_eq!(
        known_ids(medusa(&fake).aggregators().await.ok()),
        Some(Vec::new())
    );
}

#[tokio::test]
async fn a_prowlarr_medusa_cannot_reach_is_not_answered_as_read_from() {
    let fake = checking(
        enabled(&[3, 4]),
        vec![json(
            404,
            &json!({ "error": "Could not connect to prowlarr" }),
        )],
        &holding(&[("three", 3)], "the-aggregator-key"),
    );
    assert_eq!(
        known_ids(medusa(&fake).aggregators().await.ok()),
        Some(Vec::new())
    );
    assert!(!asked(&fake).iter().any(|(_, _, body)| body
        .as_ref()
        .is_some_and(|one| one.get("type") == Some(&json!("GETINDEXERS")))));
}

#[tokio::test]
async fn a_prowlarr_medusa_read_as_holding_nothing_is_only_believed_while_it_still_answers() {
    let emptied = checking(
        json(500, &json!({ "error": "Internal Server Error" })),
        connected(),
        &holding(&[("three", 3)], "the-aggregator-key"),
    );
    assert_eq!(
        known_ids(medusa(&emptied).aggregators().await.ok()),
        Some(Vec::new())
    );
    let gone_between = checking(
        json(500, &json!({ "error": "Internal Server Error" })),
        vec![
            json(200, &json!("connected")),
            json(404, &json!({ "error": "Could not connect to prowlarr" })),
        ],
        &holding(&[("three", 3)], "the-aggregator-key"),
    );
    assert_eq!(
        known_ids(medusa(&gone_between).aggregators().await.ok()),
        Some(Vec::new())
    );
}

#[tokio::test]
async fn indexers_medusa_answers_in_another_shape_are_not_read() {
    let fake = checking(
        json(200, &json!({ "unexpected": true })),
        connected(),
        &holding(&[("three", 3)], "the-aggregator-key"),
    );
    assert!(medusa(&fake).aggregators().await.is_err());
}

fn levelling(indexers: Answer, tests: Vec<Answer>, held: &[Value]) -> std::sync::Arc<Fake> {
    Fake::by_call(vec![
        ("/config/main", None, vec![linked()]),
        ("/providers/prowlarr/operation", Some("TEST"), tests),
        (
            "/providers/prowlarr/operation",
            Some("GETINDEXERS"),
            vec![indexers],
        ),
        (
            "/providers/prowlarr",
            None,
            vec![json(201, &json!({ "result": { "id": "four" } }))],
        ),
        ("/providers/newznab/", None, vec![Answer::reply(204, "")]),
        (
            "/providers",
            None,
            held.iter().map(|one| json(200, one)).collect(),
        ),
    ])
}

fn at_prowlarr() -> AggregatorAt {
    aggregator("servarr", "http://prowlarr:9696")
}

#[tokio::test]
async fn an_indexer_prowlarr_gained_since_the_first_link_is_added_and_switched_on() {
    let fake = levelling(
        enabled(&[3, 4]),
        connected(),
        &[
            holding(&[("three", 3)], "the-aggregator-key"),
            holding(&[("three", 3), ("four", 4)], "the-aggregator-key"),
        ],
    );
    assert!(medusa(&fake).add_aggregator(&at_prowlarr()).await.is_ok());
    let calls = asked(&fake);
    assert_eq!(
        calls
            .iter()
            .filter(|(_, path, _)| path == "/api/v2/providers/prowlarr")
            .filter_map(|(_, _, body)| body.as_ref()?.get("id").cloned())
            .collect::<Vec<_>>(),
        [json!(4)]
    );
    let saved = calls
        .iter()
        .filter(|(method, path, _)| *method == Method::Post && path == "/api/v2/providers")
        .find_map(|(_, _, body)| body.as_ref());
    assert_eq!(
        saved.and_then(|body| body.pointer("/providers/0/config/enabled")),
        Some(&json!(false))
    );
    assert_eq!(
        saved.and_then(|body| body.pointer("/providers/1/config/enabled")),
        Some(&json!(true))
    );
}

#[tokio::test]
async fn an_indexer_prowlarr_lost_is_removed_and_nothing_else_is() {
    let fake = levelling(
        enabled(&[3]),
        connected(),
        &[holding(&[("three", 3), ("gone", 9)], "the-aggregator-key")],
    );
    assert!(medusa(&fake).add_aggregator(&at_prowlarr()).await.is_ok());
    assert_eq!(
        asked(&fake)
            .iter()
            .filter(|(method, _, _)| *method == Method::Delete)
            .map(|(_, path, _)| path.as_str())
            .collect::<Vec<_>>(),
        ["/api/v2/providers/newznab/gone"]
    );
}

#[tokio::test]
async fn a_prowlarr_that_answers_it_holds_nothing_loses_every_provider_it_gave() {
    let fake = levelling(
        json(500, &json!({ "error": "Internal Server Error" })),
        connected(),
        &[holding(&[("three", 3)], "the-aggregator-key")],
    );
    assert!(medusa(&fake).add_aggregator(&at_prowlarr()).await.is_ok());
    assert!(asked(&fake)
        .iter()
        .any(|(method, path, _)| *method == Method::Delete
            && path == "/api/v2/providers/newznab/three"));
}

#[tokio::test]
async fn a_prowlarr_gone_between_the_two_tests_loses_nothing_and_is_refused() {
    let fake = levelling(
        json(500, &json!({ "error": "Internal Server Error" })),
        vec![
            json(200, &json!("connected")),
            json(404, &json!({ "error": "Could not connect to prowlarr" })),
        ],
        &[holding(&[("three", 3)], "the-aggregator-key")],
    );
    assert_eq!(
        why(medusa(&fake).add_aggregator(&at_prowlarr()).await),
        so(refused(
            "Medusa could not reach Prowlarr at http://prowlarr:9696"
        ))
    );
    assert!(!asked(&fake)
        .iter()
        .any(|(method, _, _)| *method == Method::Delete));
}

#[tokio::test]
async fn a_provider_prowlarr_gave_under_another_key_takes_the_key_it_answers_now() {
    let fake = levelling(
        enabled(&[3]),
        connected(),
        &[holding(&[("three", 3)], "old")],
    );
    assert!(medusa(&fake).add_aggregator(&at_prowlarr()).await.is_ok());
    let saved = asked(&fake)
        .into_iter()
        .filter(|(method, path, _)| *method == Method::Post && path == "/api/v2/providers")
        .find_map(|(_, _, body)| body);
    assert_eq!(
        saved
            .as_ref()
            .and_then(|body| body.pointer("/providers/0/config/apikey")),
        Some(&json!("the-aggregator-key"))
    );
    assert_eq!(
        saved
            .as_ref()
            .and_then(|body| body.pointer("/providers/0/config/enabled")),
        Some(&json!(false))
    );
}

#[tokio::test]
async fn nzbhydra2_already_held_is_not_added_twice_and_takes_the_key_it_answers_now() {
    let fake = Fake::by_call(vec![(
        "/providers",
        None,
        vec![json(
            200,
            &json!([provider(
                "hydra",
                "newznab",
                &Value::Null,
                "http://nzbhydra2:5076",
                "old",
                false
            )]),
        )],
    )]);
    assert!(medusa(&fake)
        .add_aggregator(&aggregator("nzbhydra2", "http://nzbhydra2:5076"))
        .await
        .is_ok());
    let calls = asked(&fake);
    assert!(!calls
        .iter()
        .any(|(_, path, _)| path == "/api/v2/providers/newznab"));
    assert_eq!(
        calls
            .iter()
            .find(|(method, _, _)| *method == Method::Post)
            .and_then(|(_, _, body)| body.as_ref())
            .and_then(|body| body.pointer("/providers/0/config/apikey")),
        Some(&json!("the-aggregator-key"))
    );
    let level = Fake::by_call(vec![(
        "/providers",
        None,
        vec![json(
            200,
            &json!([provider(
                "hydra",
                "newznab",
                &Value::Null,
                "http://nzbhydra2:5076",
                "the-aggregator-key",
                false
            )]),
        )],
    )]);
    assert!(medusa(&level)
        .add_aggregator(&aggregator("nzbhydra2", "http://nzbhydra2:5076"))
        .await
        .is_ok());
    assert_eq!(level.requests().len(), 1);
}
