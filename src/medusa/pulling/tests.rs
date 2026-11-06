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
    let fake = Fake::by_path(vec![
        (
            "/config/main",
            json(
                200,
                &json!({ "providers": { "prowlarr": { "url": "http://prowlarr:9696", "apikey": "k" } } }),
            ),
        ),
        (
            "/providers",
            json(
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
            ),
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
async fn a_prowlarr_medusa_cannot_reach_is_refused_in_its_words() {
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
        so(refused("HTTP 404: Could not connect to prowlarr"))
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
        asked(&fake).first(),
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
