use lemonfiber_fixtures::http::{Answer, Fake};
use lemonfiber_ports::http::Method;
use lemonfiber_ports::media::{Format, Kind};
use lemonfiber_ports::service::{
    AddPlan, Catalogue as _, CatalogueEntry, Maintenance as _, MusicQuality as _,
    QualityReleases as _, ReleaseProbe,
};
use serde_json::{json, Value};

use crate::medusa::tests::{asked, json, medusa, refused, so, unsupported, why};

fn found(results: &Value) -> Answer {
    json(200, &json!({ "results": results, "languageId": 7 }))
}

#[tokio::test]
async fn a_term_is_looked_up_on_thetvdb_and_a_series_it_follows_is_named_as_held() {
    let fake = Fake::always(found(&json!([
        [
            "TVDBv2",
            1,
            "https://thetvdb.com/",
            75932,
            "Fawlty Towers",
            "1975-09-19",
            "BBC Two",
            "Fawlty Towers",
            false
        ],
        [
            "TVDBv2",
            1,
            "https://thetvdb.com/",
            81189,
            "Breaking Bad",
            "N/A",
            "AMC",
            "Breaking Bad",
            ["tvdb", 81189]
        ],
        [
            "TVDBv2",
            1,
            "https://thetvdb.com/",
            "unnumbered",
            "Odd",
            "2001",
            "",
            "Odd",
            false
        ],
    ])));
    assert_eq!(
        medusa(&fake).lookup(Kind::Tv, "a term & more").await.ok(),
        Some(vec![
            CatalogueEntry {
                title: "Fawlty Towers".to_owned(),
                year: Some(1975),
                reference: 75932,
                held_as: None,
            },
            CatalogueEntry {
                title: "Breaking Bad".to_owned(),
                year: None,
                reference: 81189,
                held_as: Some(81189),
            },
        ])
    );
    assert_eq!(
        asked(&fake),
        [(
            Method::Get,
            "/api/v2/internal/searchIndexersForShowName?query=a+term+%26+more&indexerId=1"
                .to_owned(),
            None
        )]
    );
}

#[tokio::test]
async fn films_are_neither_looked_up_planned_added_nor_searched() {
    let fake = Fake::always(Answer::reply(200, "[]"));
    let medusa = medusa(&fake);
    let film = unsupported("it files no film");
    assert_eq!(why(medusa.lookup(Kind::Movies, "x").await), so(film));
    assert_eq!(
        why(medusa.add_plan(Kind::Movies).await),
        so(unsupported("it files no film"))
    );
    assert_eq!(
        why(medusa
            .add(
                Kind::Movies,
                &CatalogueEntry {
                    title: String::new(),
                    year: None,
                    reference: 1,
                    held_as: None
                },
                &AddPlan {
                    root_folder: String::new(),
                    quality_profile: 0
                }
            )
            .await),
        so(unsupported("it files no film"))
    );
    assert_eq!(
        why(medusa.search_upgrades(Kind::Movies).await),
        so(unsupported("it files no film"))
    );
    assert_eq!(
        why(medusa.probe_releases(Kind::Movies).await),
        so(unsupported("it files no film"))
    );
    assert!(fake.requests().is_empty());
}

#[tokio::test]
async fn a_series_is_planned_into_its_default_folder_at_its_default_quality() {
    let fake = Fake::always(json(
        200,
        &json!({ "rootDirs": ["1", "/a", "/data/tv"], "showDefaults": { "quality": 6 } }),
    ));
    assert_eq!(
        medusa(&fake).add_plan(Kind::Tv).await.ok(),
        Some(AddPlan {
            root_folder: "/data/tv".to_owned(),
            quality_profile: 6,
        })
    );
    let unset = Fake::always(json(200, &json!({ "rootDirs": ["4", "/a"] })));
    assert_eq!(
        medusa(&unset)
            .add_plan(Kind::Tv)
            .await
            .ok()
            .map(|plan| plan.root_folder),
        Some("/a".to_owned())
    );
    let none = Fake::always(json(200, &json!({ "rootDirs": [] })));
    assert_eq!(
        why(medusa(&none).add_plan(Kind::Tv).await),
        so(refused("series has no root folder configured yet"))
    );
}

#[tokio::test]
async fn a_series_is_added_by_its_thetvdb_id_wanted_into_the_planned_folder_and_quality() {
    let fake = Fake::always(Answer::reply(201, "{}"));
    let added = medusa(&fake)
        .add(
            Kind::Tv,
            &CatalogueEntry {
                title: "Fawlty Towers".to_owned(),
                year: Some(1975),
                reference: 75932,
                held_as: None,
            },
            &AddPlan {
                root_folder: "/data/tv".to_owned(),
                quality_profile: 6 | (32 << 16),
            },
        )
        .await
        .ok();
    assert_eq!(
        added.map(|one| (one.id, one.title)),
        Some((75932, "Fawlty Towers".to_owned()))
    );
    assert_eq!(
        asked(&fake),
        [(
            Method::Post,
            "/api/v2/series".to_owned(),
            Some(json!({
                "id": { "tvdb": 75932 },
                "options": {
                    "rootDir": "/data/tv",
                    "quality": { "allowed": [2, 4], "preferred": [32] },
                    "status": 3,
                    "seasonFolders": true,
                },
            }))
        )]
    );
}

#[tokio::test]
async fn an_entry_naming_no_thetvdb_id_is_refused_and_nothing_is_added() {
    let fake = Fake::always(Answer::reply(201, "{}"));
    assert_eq!(
        why(medusa(&fake)
            .add(
                Kind::Tv,
                &CatalogueEntry {
                    title: "conformance".to_owned(),
                    year: None,
                    reference: 0,
                    held_as: None,
                },
                &AddPlan {
                    root_folder: "/data/tv".to_owned(),
                    quality_profile: 6,
                },
            )
            .await),
        so(refused(
            "a series is added by its TheTVDB id, and this entry names none"
        ))
    );
    assert!(fake.requests().is_empty());
}

#[tokio::test]
async fn it_counts_the_providers_it_searches() {
    let fake = Fake::always(json(
        200,
        &json!([
            { "id": "a", "config": { "enabled": true } },
            { "id": "b", "config": { "enabled": false } },
            { "id": "c" },
            { "id": "d", "config": { "enabled": true } },
        ]),
    ));
    assert_eq!(medusa(&fake).indexer_count().await.ok(), Some(2));
}

#[tokio::test]
async fn upgrades_are_searched_by_a_backlog_search_of_each_series_it_is_not_pausing() {
    let fake = Fake::by_route(vec![
        (
            Method::Get,
            "/series",
            json(
                200,
                &json!([
                    { "id": { "tvdb": 1, "slug": "tvdb1" }, "config": { "paused": false } },
                    { "id": { "tvdb": 2, "slug": "tvdb2" }, "config": { "paused": true } },
                    { "id": { "tvdb": 3, "slug": "tvdb3" }, "config": { "paused": false } },
                ]),
            ),
        ),
        (
            Method::Put,
            "/search/backlog",
            Answer::reply(202, "\"Backlog search started\""),
        ),
    ]);
    assert!(medusa(&fake).search_upgrades(Kind::Tv).await.is_ok());
    assert_eq!(
        asked(&fake)
            .into_iter()
            .filter(|(method, _, _)| *method == Method::Put)
            .map(|(_, path, body)| (path, body))
            .collect::<Vec<_>>(),
        [
            (
                "/api/v2/search/backlog".to_owned(),
                Some(json!({ "showSlug": "tvdb1" }))
            ),
            (
                "/api/v2/search/backlog".to_owned(),
                Some(json!({ "showSlug": "tvdb3" }))
            ),
        ]
    );
    let refusing = Fake::by_route(vec![
        (
            Method::Get,
            "/series",
            json(200, &json!([{ "id": { "tvdb": 1, "slug": "tvdb1" } }])),
        ),
        (
            Method::Put,
            "/search/backlog",
            json(404, &json!({ "error": "Series not found" })),
        ),
    ]);
    assert_eq!(
        why(medusa(&refusing).search_upgrades(Kind::Tv).await),
        so(refused("HTTP 404: Series not found"))
    );
    let nothing = Fake::always(json(200, &json!([])));
    assert!(medusa(&nothing).search_upgrades(Kind::Tv).await.is_ok());
    assert_eq!(nothing.requests().len(), 1);
}

#[tokio::test]
async fn it_wants_something_while_its_backlog_lists_a_wanted_episode() {
    for (listed, wanting) in [(json!([{ "slug": "tvdb1" }]), true), (json!([]), false)] {
        let fake = Fake::always(json(200, &listed));
        assert_eq!(medusa(&fake).wanting().await.ok(), Some(wanting));
        assert_eq!(
            asked(&fake),
            [(
                Method::Get,
                "/api/v2/internal/getEpisodeBacklog?status=wanted&period=all".to_owned(),
                None
            )]
        );
    }
}

#[tokio::test]
async fn it_files_no_music() {
    let fake = Fake::silent();
    assert_eq!(
        why(medusa(&fake).apply_music_format(Format::Lossless).await),
        so(unsupported("it files no music"))
    );
}

fn wanted() -> Value {
    json!([
        { "slug": "tvdb2", "quality": 6, "episodes": [] },
        { "slug": "tvdb1", "quality": 6 | (32 << 16), "episodes": [
            { "season": 1, "episode": 2, "slug": "S01E02" },
            { "season": 1, "episode": 1, "slug": "S01E01" },
        ] },
    ])
}

fn providers() -> Answer {
    json(
        200,
        &json!([
            { "id": "one", "config": { "enabled": true, "search": { "manual": { "enabled": true } } } },
            { "id": "two", "config": { "enabled": true, "search": { "manual": { "enabled": true } } } },
            { "id": "off", "config": { "enabled": false, "search": { "manual": { "enabled": true } } } },
            { "id": "auto", "config": { "enabled": true, "search": { "manual": { "enabled": false } } } },
        ]),
    )
}

fn searched(status: &str) -> Answer {
    json(
        200,
        &json!({ "results": [
        { "episode": { "slug": "s01e09" }, "search": { "status": "queued" } },
        { "episode": { "slug": "s01e02" }, "search": { "status": status } },
    ] }),
    )
}

fn probing(one: Answer, two: Answer) -> std::sync::Arc<Fake> {
    Fake::by_rules(vec![
        (None, "getEpisodeBacklog", json(200, &wanted())),
        (
            Some(Method::Put),
            "/search/manual",
            Answer::reply(202, "\"started\""),
        ),
        (None, "/search/tvdb1", searched("finished")),
        (None, "/providers/one/results", one),
        (None, "/providers/two/results", two),
        (None, "/providers", providers()),
    ])
}

fn none_found() -> Answer {
    json(404, &json!({ "error": "Provider cache results not found" }))
}

#[tokio::test]
async fn the_first_wanted_episode_is_searched_and_what_was_found_judged_by_its_quality() {
    for (one, two, probe) in [
        (none_found(), none_found(), ReleaseProbe::NoneFound),
        (
            json(200, &json!([{ "quality": 8 }])),
            none_found(),
            ReleaseProbe::NoneMatch,
        ),
        (
            json(200, &json!([{ "quality": 8 }])),
            json(200, &json!([{ "quality": 32 }])),
            ReleaseProbe::Matching,
        ),
        (
            json(200, &json!([{ "quality": 4 }])),
            none_found(),
            ReleaseProbe::Matching,
        ),
    ] {
        let fake = probing(one, two);
        assert_eq!(
            medusa(&fake).probe_releases(Kind::Tv).await.ok(),
            Some(probe)
        );
        let paths: Vec<(Method, String, Option<Value>)> = asked(&fake);
        assert_eq!(
            paths.get(1),
            Some(&(
                Method::Put,
                "/api/v2/search/manual".to_owned(),
                Some(json!({ "showSlug": "tvdb1", "episodes": ["S01E02"] }))
            ))
        );
        assert!(fake.asked_for("/providers/one/results?showslug=tvdb1&season=1&episode=2"));
        assert!(!fake.asked_for("/providers/off/results"));
        assert!(!fake.asked_for("/providers/auto/results"));
    }
}

#[tokio::test]
async fn nothing_wanted_is_nothing_searched() {
    let fake = Fake::always(json(200, &json!([{ "slug": "tvdb2", "episodes": [] }])));
    assert_eq!(
        medusa(&fake).probe_releases(Kind::Tv).await.ok(),
        Some(ReleaseProbe::NothingWanted)
    );
    assert_eq!(fake.requests().len(), 1);
}

#[tokio::test]
async fn a_search_that_does_not_finish_in_time_is_refused() {
    let fake = Fake::by_rules(vec![
        (None, "getEpisodeBacklog", json(200, &wanted())),
        (
            Some(Method::Put),
            "/search/manual",
            Answer::reply(202, "\"started\""),
        ),
        (None, "/search/tvdb1", searched("searching")),
    ]);
    assert_eq!(
        why(medusa(&fake).probe_releases(Kind::Tv).await),
        so(refused(
            "Medusa's search for releases did not finish in time"
        ))
    );
    assert_eq!(
        fake.requests()
            .iter()
            .filter(|one| one.url.contains("/search/tvdb1"))
            .count(),
        10
    );
    let elsewhere = Fake::by_rules(vec![
        (None, "getEpisodeBacklog", json(200, &wanted())),
        (
            Some(Method::Put),
            "/search/manual",
            Answer::reply(202, "\"started\""),
        ),
        (None, "/search/tvdb1", json(200, &json!({ "results": [] }))),
    ]);
    assert!(medusa(&elsewhere).probe_releases(Kind::Tv).await.is_err());
}
