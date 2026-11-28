use lemonfiber_fixtures::http::{Answer, Fake};
use lemonfiber_ports::http::Method;
use lemonfiber_ports::media::{Kind, TitleKind};
use lemonfiber_ports::service::{
    CuratedSeason, CuratedTitle, DownloadState, DownloadedEpisode, Downloading, Fulfilling as _,
    TitleRequest,
};
use serde_json::{json, Value};

use crate::medusa::tests::{asked, json, medusa, refused, so, unsupported, why};

const FAWLTY: i64 = 75932;

const BAD: i64 = 81189;

fn fawlty(paused: bool, later: &str) -> Value {
    json!({ "id": { "tvdb": FAWLTY, "slug": "tvdb75932" }, "title": "Fawlty Towers", "year": { "start": 1975 }, "config": { "paused": paused, "defaultEpisodeStatus": later } })
}

fn bad() -> Value {
    json!({ "id": { "tvdb": BAD, "slug": "tvdb81189" }, "title": "Breaking Bad", "year": { "start": 2008 }, "config": { "paused": false, "defaultEpisodeStatus": "Wanted" } })
}

fn nameless() -> Value {
    json!({ "id": { "slug": "tvmaze1" }, "title": "Nowhere", "config": {} })
}

fn episode(season: u32, number: u32, status: &str, file: &str) -> Value {
    json!({ "slug": format!("s{season:02}e{number:02}"), "season": season, "episode": number, "title": format!("Episode {number}"), "status": status, "file": { "location": file } })
}

fn episodes() -> Value {
    json!([
        episode(0, 1, "Unaired", ""),
        episode(1, 1, "Downloaded", "/data/tv/Fawlty Towers/Season 01/a.mkv"),
        episode(1, 2, "Skipped", ""),
        episode(2, 1, "Ignored", ""),
        episode(2, 2, "Unaired", ""),
    ])
}

fn seasons(specials: bool) -> Vec<CuratedSeason> {
    vec![
        CuratedSeason {
            number: 0,
            monitored: specials,
            files: 0,
            episodes: 1,
        },
        CuratedSeason {
            number: 1,
            monitored: true,
            files: 1,
            episodes: 2,
        },
        CuratedSeason {
            number: 2,
            monitored: specials,
            files: 0,
            episodes: 2,
        },
    ]
}

fn titled(reference: i64, title: &str, year: u32, monitored: bool, specials: bool) -> CuratedTitle {
    CuratedTitle {
        reference,
        id: Some(reference),
        title: title.to_owned(),
        year: Some(year),
        monitored,
        files: 1,
        seasons: seasons(specials),
    }
}

fn holding(series: &[Value]) -> std::sync::Arc<Fake> {
    Fake::by_route_in_turn(vec![
        (Method::Get, "/episodes", vec![json(200, &episodes())]),
        (
            Method::Get,
            "/series?",
            vec![json(200, &Value::Array(series.to_vec()))],
        ),
    ])
}

fn asking(reference: i64, seasons: Vec<u32>, search: bool) -> TitleRequest {
    TitleRequest {
        reference,
        root_folder: "/data/tv/".to_owned(),
        quality_profile: 328,
        seasons,
        anime: true,
        later_seasons: false,
        search,
    }
}

fn settled() -> Vec<(Method, &'static str, Vec<Answer>)> {
    vec![
        (
            Method::Get,
            "/config/main",
            vec![json(200, &json!({ "rootDirs": ["0", "/data/tv"] }))],
        ),
        (
            Method::Get,
            "/config/consts",
            vec![json(
                200,
                &json!({ "qualities": { "presets": [{ "value": 328, "name": "HD720p" }] } }),
            )],
        ),
    ]
}

fn writes(fake: &Fake) -> Vec<(Method, String, Option<Value>)> {
    asked(fake)
        .into_iter()
        .filter(|(method, _, _)| *method != Method::Get)
        .collect()
}

#[tokio::test]
async fn every_series_it_holds_by_thetvdb_id_is_a_title_with_its_seasons() {
    let fake = holding(&[fawlty(false, "Wanted"), fawlty(true, "Skipped"), nameless()]);
    assert_eq!(
        medusa(&fake).titles(TitleKind::Tv).await.ok(),
        Some(vec![
            titled(FAWLTY, "Fawlty Towers", 1975, true, true),
            titled(FAWLTY, "Fawlty Towers", 1975, false, false),
        ])
    );
}

#[tokio::test]
async fn a_series_it_holds_is_catalogued_and_held_by_its_thetvdb_id() {
    let fake = holding(&[fawlty(false, "Skipped")]);
    let held = Some(titled(FAWLTY, "Fawlty Towers", 1975, true, false));
    assert_eq!(
        medusa(&fake).catalogued(Kind::Tv, FAWLTY).await.ok(),
        Some(held.clone())
    );
    assert_eq!(
        medusa(&fake).held_title(Kind::Tv, FAWLTY).await.ok(),
        Some(held)
    );
    assert_eq!(
        medusa(&fake).catalogued(Kind::Tv, BAD).await.ok(),
        Some(None)
    );
}

#[tokio::test]
async fn no_reference_is_catalogued_and_nothing_is_asked() {
    let fake = Fake::silent();
    assert_eq!(medusa(&fake).catalogued(Kind::Tv, 0).await.ok(), Some(None));
    assert!(asked(&fake).is_empty());
}

#[tokio::test]
async fn films_are_neither_held_nor_requested() {
    let fake = Fake::silent();
    let medusa = medusa(&fake);
    let none = so(unsupported("it files no film"));
    assert_eq!(why(medusa.catalogued(Kind::Movies, 1).await), none);
    assert_eq!(why(medusa.titles(TitleKind::Movies).await), none);
    assert_eq!(
        why(medusa
            .request(Kind::Movies, &asking(1, vec![], false))
            .await),
        none
    );
    assert_eq!(why(medusa.search(TitleKind::Movies, 1).await), none);
    assert_eq!(why(medusa.remove(Kind::Movies, 1).await), none);
    assert_eq!(why(medusa.downloading(TitleKind::Movies).await), none);
    assert!(asked(&fake).is_empty());
}

#[tokio::test]
async fn albums_and_books_are_neither_listed_searched_nor_downloading() {
    let fake = Fake::silent();
    let medusa = medusa(&fake);
    for (kind, noun) in [(TitleKind::Music, "album"), (TitleKind::Books, "book")] {
        let none = || so(unsupported(&format!("it files no {noun}")));
        assert_eq!(why(medusa.titles(kind).await), none());
        assert_eq!(why(medusa.search(kind, 1).await), none());
        assert_eq!(why(medusa.downloading(kind).await), none());
    }
    assert!(asked(&fake).is_empty());
}

#[tokio::test]
async fn a_request_naming_no_reference_a_folder_or_a_profile_it_lacks_is_refused() {
    let fake = Fake::by_route_in_turn(settled());
    let medusa = medusa(&fake);
    assert_eq!(
        why(medusa.request(Kind::Tv, &asking(0, vec![1], false)).await),
        so(refused(
            "a series is requested by its TheTVDB id, and this request names none"
        ))
    );
    let mut elsewhere = asking(BAD, vec![1], false);
    elsewhere.root_folder = "/data/films".to_owned();
    assert_eq!(
        why(medusa.request(Kind::Tv, &elsewhere).await),
        so(refused("Medusa holds no root folder /data/films"))
    );
    let mut unprofiled = asking(BAD, vec![1], false);
    unprofiled.quality_profile = 1;
    assert_eq!(
        why(medusa.request(Kind::Tv, &unprofiled).await),
        so(refused("Medusa holds no quality profile 1"))
    );
    assert!(writes(&fake).is_empty());
}

fn adding(steps: Vec<Answer>) -> std::sync::Arc<Fake> {
    let mut routes = settled();
    routes.extend([
        (Method::Get, "/series/tvdb81189?detailed=true", steps),
        (Method::Get, "/series/tvdb81189/episodes", vec![json(200, &episodes())]),
        (
            Method::Patch,
            "/series/tvdb81189/episodes",
            vec![json(207, &json!({ "s01e02": { "status": 200 }, "s02e01": { "status": 200 }, "s02e02": { "status": 200 } }))],
        ),
        (Method::Patch, "/series/tvdb81189", vec![json(200, &json!({}))]),
        (Method::Put, "/search/backlog", vec![json(200, &json!({}))]),
        (Method::Post, "/series", vec![json(201, &json!({}))]),
        (
            Method::Get,
            "/series?",
            vec![
                json(200, &json!([])),
                json(200, &json!([])),
                json(200, &json!([bad()])),
            ],
        ),
    ]);
    Fake::by_route_in_turn(routes)
}

fn busy(active: bool) -> Answer {
    json(
        200,
        &json!({ "showQueueStatus": [{ "action": "isBeingUpdated", "active": true }, { "action": "isBeingAdded", "active": active }] }),
    )
}

#[tokio::test]
async fn a_series_it_does_not_hold_is_added_wanting_the_seasons_asked_alone() {
    let fake = adding(vec![busy(true), busy(false)]);
    let title = medusa(&fake)
        .request(Kind::Tv, &asking(BAD, vec![1, 2], true))
        .await;
    assert_eq!(
        title.ok().map(|title| (title.reference, title.title)),
        Some((BAD, "Breaking Bad".to_owned()))
    );
    assert_eq!(
        writes(&fake),
        [
            (
                Method::Post,
                "/api/v2/series".to_owned(),
                Some(json!({ "id": { "tvdb": BAD }, "options": {
                    "rootDir": "/data/tv/",
                    "quality": { "allowed": [8, 64, 256], "preferred": [] },
                    "status": 5,
                    "seasonFolders": true,
                    "anime": true,
                } }))
            ),
            (
                Method::Patch,
                "/api/v2/series/tvdb81189".to_owned(),
                Some(json!({ "config": { "defaultEpisodeStatus": "Skipped" } }))
            ),
            (
                Method::Patch,
                "/api/v2/series/tvdb81189/episodes".to_owned(),
                Some(
                    json!({ "s01e02": { "status": 3 }, "s02e01": { "status": 3 }, "s02e02": { "status": 3 } })
                )
            ),
            (
                Method::Put,
                "/api/v2/search/backlog".to_owned(),
                Some(json!({ "showSlug": "tvdb81189" }))
            ),
        ]
    );
}

#[tokio::test]
async fn a_series_asked_to_air_on_wants_what_airs_later() {
    let fake = adding(vec![busy(false)]);
    let mut later = asking(BAD, vec![1], false);
    later.later_seasons = true;
    assert!(medusa(&fake).request(Kind::Tv, &later).await.is_ok());
    assert!(writes(&fake).contains(&(
        Method::Patch,
        "/api/v2/series/tvdb81189".to_owned(),
        Some(json!({ "config": { "defaultEpisodeStatus": "Wanted" } }))
    )));
}

#[tokio::test]
async fn a_series_medusa_is_still_adding_is_refused_and_nothing_more_is_written() {
    let fake = adding(vec![busy(true)]);
    assert_eq!(
        why(medusa(&fake)
            .request(Kind::Tv, &asking(BAD, vec![1], false))
            .await),
        so(refused(
            "Medusa had not finished adding the series with TheTVDB id 81189 in time"
        ))
    );
    assert_eq!(writes(&fake).len(), 1);
}

const DAILY: i64 = 71256;

const ONE_PIECE: i64 = 81797;

const HEIST: i64 = 327_417;

fn show(tvdb: i64, title: &str, year: u32) -> Value {
    json!({ "id": { "tvdb": tvdb, "slug": format!("tvdb{tvdb}") }, "title": title, "year": { "start": year }, "config": { "paused": false, "defaultEpisodeStatus": "Skipped" } })
}

fn skipped_in(seasons: &[u32]) -> Value {
    Value::Array(
        seasons
            .iter()
            .map(|season| episode(*season, 1, "Skipped", ""))
            .collect(),
    )
}

struct Show {
    series: Value,
    detailed: &'static str,
    episodes: &'static str,
    one: &'static str,
}

fn daily() -> Show {
    Show {
        series: show(DAILY, "The Daily Show", 1996),
        detailed: "/series/tvdb71256?detailed=true",
        episodes: "/series/tvdb71256/episodes",
        one: "/series/tvdb71256",
    }
}

fn one_piece() -> Show {
    Show {
        series: show(ONE_PIECE, "One Piece", 1999),
        detailed: "/series/tvdb81797?detailed=true",
        episodes: "/series/tvdb81797/episodes",
        one: "/series/tvdb81797",
    }
}

fn heist() -> Show {
    Show {
        series: show(HEIST, "Money Heist", 2017),
        detailed: "/series/tvdb327417?detailed=true",
        episodes: "/series/tvdb327417/episodes",
        one: "/series/tvdb327417",
    }
}

fn added_holding(
    added: &Show,
    seasons: &[u32],
    taken_out: Answer,
    after: &[Value],
) -> std::sync::Arc<Fake> {
    let mut routes = settled();
    let mut listed = vec![json(200, &json!([])), json(200, &json!([&added.series]))];
    listed.extend(after.iter().map(|one| json(200, one)));
    routes.extend([
        (Method::Get, added.detailed, vec![busy(false)]),
        (
            Method::Get,
            added.episodes,
            vec![json(200, &skipped_in(seasons))],
        ),
        (
            Method::Patch,
            added.episodes,
            vec![json(207, &json!({ "s03e01": { "status": 200 } }))],
        ),
        (Method::Patch, added.one, vec![json(200, &json!({}))]),
        (Method::Delete, added.one, vec![taken_out]),
        (Method::Post, "/series", vec![json(201, &json!({}))]),
        (Method::Get, "/series?", listed),
    ]);
    Fake::by_route_in_turn(routes)
}

fn taken_out() -> Answer {
    Answer::reply(204, "")
}

#[tokio::test]
async fn a_series_numbered_by_year_elsewhere_is_refused_and_taken_out_again() {
    let fake = added_holding(&daily(), &[1, 2, 3], taken_out(), &[json!([])]);
    assert_eq!(
        why(medusa(&fake)
            .request(Kind::Tv, &asking(DAILY, vec![2015, 2016], false))
            .await),
        so(refused(
            "Medusa holds none of the seasons asked for (2015, 2016) of the series with TheTVDB \
             id 71256, holding seasons 1, 2, 3, so the series was taken out again"
        ))
    );
    let written: Vec<(Method, String)> = writes(&fake)
        .into_iter()
        .map(|(method, path, _)| (method, path))
        .collect();
    assert_eq!(
        written,
        [
            (Method::Post, "/api/v2/series".to_owned()),
            (Method::Patch, "/api/v2/series/tvdb71256".to_owned()),
            (Method::Delete, "/api/v2/series/tvdb71256".to_owned()),
        ]
    );
}

#[tokio::test]
async fn an_anime_numbered_by_year_elsewhere_is_refused_and_its_files_are_never_asked_to_go() {
    let fake = added_holding(&one_piece(), &[0, 1, 23], taken_out(), &[json!([])]);
    let refusal = why(medusa(&fake)
        .request(Kind::Tv, &asking(ONE_PIECE, vec![1999], true))
        .await);
    assert_eq!(
        refusal,
        so(refused(
            "Medusa holds none of the seasons asked for (1999) of the series with TheTVDB id \
             81797, holding seasons 0, 1, 23, so the series was taken out again"
        ))
    );
    assert!(writes(&fake)
        .iter()
        .all(|(method, path, _)| *method != Method::Put && !path.contains("remove-files")));
}

#[tokio::test]
async fn a_series_holding_some_of_the_seasons_asked_wants_those_it_holds() {
    let fake = added_holding(&heist(), &[1, 2, 3], taken_out(), &[]);
    assert!(medusa(&fake)
        .request(Kind::Tv, &asking(HEIST, vec![3, 4, 5], false))
        .await
        .is_ok());
    let written = writes(&fake);
    assert!(written.contains(&(
        Method::Patch,
        "/api/v2/series/tvdb327417/episodes".to_owned(),
        Some(json!({ "s03e01": { "status": 3 } }))
    )));
    assert!(written
        .iter()
        .all(|(method, _, _)| *method != Method::Delete));
}

#[tokio::test]
async fn a_series_holding_no_season_is_refused_saying_so() {
    let fake = added_holding(&daily(), &[], taken_out(), &[json!([])]);
    assert_eq!(
        why(medusa(&fake)
            .request(Kind::Tv, &asking(DAILY, vec![1], false))
            .await),
        so(refused(
            "Medusa holds none of the seasons asked for (1) of the series with TheTVDB id 71256, \
             holding no season, so the series was taken out again"
        ))
    );
}

#[tokio::test]
async fn a_series_asked_for_no_season_is_added_and_kept() {
    let fake = added_holding(&daily(), &[1], taken_out(), &[]);
    assert!(medusa(&fake)
        .request(Kind::Tv, &asking(DAILY, Vec::new(), false))
        .await
        .is_ok());
    assert!(writes(&fake)
        .iter()
        .all(|(method, _, _)| *method != Method::Delete));
}

#[tokio::test]
async fn a_series_medusa_will_not_take_out_again_is_a_failure() {
    let fake = added_holding(&daily(), &[1], json(409, &json!({})), &[]);
    assert!(medusa(&fake)
        .request(Kind::Tv, &asking(DAILY, vec![2015], false))
        .await
        .is_err());
}

#[tokio::test]
async fn a_series_medusa_is_still_taking_out_is_refused_saying_so() {
    let daily_held = daily().series;
    let fake = added_holding(&daily(), &[1], taken_out(), &[json!([daily_held])]);
    assert_eq!(
        why(medusa(&fake)
            .request(Kind::Tv, &asking(DAILY, vec![2015], false))
            .await),
        so(refused(
            "Medusa had not finished taking out the series with TheTVDB id 71256 in time"
        ))
    );
}

fn held(paused: bool, saved: &Value) -> std::sync::Arc<Fake> {
    let mut routes = settled();
    routes.extend([
        (
            Method::Get,
            "/series/tvdb75932/episodes",
            vec![json(200, &episodes())],
        ),
        (
            Method::Patch,
            "/series/tvdb75932/episodes",
            vec![json(207, saved)],
        ),
        (
            Method::Patch,
            "/series/tvdb75932",
            vec![json(200, &json!({}))],
        ),
        (Method::Put, "/search/backlog", vec![json(200, &json!({}))]),
        (
            Method::Get,
            "/series?",
            vec![
                json(200, &json!([fawlty(paused, "Skipped")])),
                json(200, &json!([fawlty(false, "Skipped")])),
            ],
        ),
    ]);
    Fake::by_route_in_turn(routes)
}

#[tokio::test]
async fn a_series_it_holds_is_watched_again_wanting_what_it_skipped_of_the_seasons_asked() {
    let fake = held(
        true,
        &json!({ "s02e01": { "status": 200 }, "s02e02": { "status": 200 } }),
    );
    let title = medusa(&fake)
        .request(Kind::Tv, &asking(FAWLTY, vec![2], false))
        .await;
    assert_eq!(title.ok().map(|title| title.monitored), Some(true));
    assert_eq!(
        writes(&fake),
        [
            (
                Method::Patch,
                "/api/v2/series/tvdb75932".to_owned(),
                Some(json!({ "config": { "paused": false } }))
            ),
            (
                Method::Patch,
                "/api/v2/series/tvdb75932/episodes".to_owned(),
                Some(json!({ "s02e01": { "status": 3 }, "s02e02": { "status": 3 } }))
            ),
        ]
    );
}

#[tokio::test]
async fn a_series_it_watches_wanting_every_season_asked_has_nothing_written() {
    let fake = held(false, &json!({}));
    assert!(medusa(&fake)
        .request(Kind::Tv, &asking(FAWLTY, vec![3], false))
        .await
        .is_ok());
    assert!(writes(&fake).is_empty());
}

#[tokio::test]
async fn an_episode_medusa_would_not_want_is_refused_by_name() {
    let fake = held(
        false,
        &json!({ "s02e01": { "status": 404 }, "s02e02": { "status": 200 } }),
    );
    assert_eq!(
        why(medusa(&fake)
            .request(Kind::Tv, &asking(FAWLTY, vec![2], false))
            .await),
        so(refused("Medusa would not want s02e01"))
    );
}

#[tokio::test]
async fn a_series_it_holds_is_searched_and_removed_with_its_files() {
    let fake = Fake::by_route_in_turn(vec![
        (Method::Put, "/search/backlog", vec![json(200, &json!({}))]),
        (
            Method::Delete,
            "/series/tvdb75932",
            vec![Answer::reply(204, "")],
        ),
        (
            Method::Get,
            "/series?",
            vec![json(200, &json!([fawlty(false, "Wanted")]))],
        ),
    ]);
    let medusa = medusa(&fake);
    assert!(medusa.search(TitleKind::Tv, FAWLTY).await.is_ok());
    assert!(medusa.remove(Kind::Tv, FAWLTY).await.is_ok());
    assert_eq!(
        writes(&fake),
        [
            (
                Method::Put,
                "/api/v2/search/backlog".to_owned(),
                Some(json!({ "showSlug": "tvdb75932" }))
            ),
            (
                Method::Delete,
                "/api/v2/series/tvdb75932?remove-files=true".to_owned(),
                None
            ),
        ]
    );
}

#[tokio::test]
async fn a_series_it_does_not_hold_is_neither_searched_nor_removed() {
    let fake = Fake::by_route_in_turn(vec![(Method::Get, "/series?", vec![json(200, &json!([]))])]);
    let medusa = medusa(&fake);
    let none = so(refused("Medusa holds no series with TheTVDB id 81189"));
    assert_eq!(why(medusa.search(TitleKind::Tv, BAD).await), none);
    assert_eq!(why(medusa.remove(Kind::Tv, BAD).await), none);
    assert!(writes(&fake).is_empty());
}

#[tokio::test]
async fn a_removal_medusa_refuses_is_a_failure() {
    let fake = Fake::by_route_in_turn(vec![
        (
            Method::Delete,
            "/series/tvdb75932",
            vec![json(400, &json!({ "error": "no" }))],
        ),
        (
            Method::Get,
            "/series?",
            vec![json(200, &json!([fawlty(false, "Wanted")]))],
        ),
    ]);
    assert!(medusa(&fake).remove(Kind::Tv, FAWLTY).await.is_err());
}

fn snatch(slug: &str, episode: u32, bits: &[i64]) -> Value {
    json!({
        "status": 2,
        "resource": format!("Release.S01E0{episode}"),
        "size": 2_000_000,
        "season": 1,
        "episode": episode,
        "infoHash": format!("hash-{episode}"),
        "showSlug": slug,
        "showTitle": "Fawlty Towers",
        "clientStatus": { "status": bits },
    })
}

#[tokio::test]
async fn what_is_downloading_names_its_series_its_episode_and_where_it_stands() {
    let fake = Fake::by_route_in_turn(vec![
        (
            Method::Get,
            "/history?",
            vec![json(
                200,
                &json!([
                    snatch("tvdb75932", 1, &[128]),
                    snatch("tvdb75932", 2, &[2]),
                    snatch("tvdb75932", 3, &[1]),
                    snatch("tvdb75932", 4, &[]),
                    snatch("tvdb75932", 5, &[16]),
                    snatch("tvmaze1", 1, &[2]),
                ]),
            )],
        ),
        (Method::Get, "/episodes", vec![json(200, &episodes())]),
        (
            Method::Get,
            "/series?",
            vec![json(200, &json!([fawlty(false, "Wanted"), nameless()]))],
        ),
    ]);
    let downloading = medusa(&fake)
        .downloading(TitleKind::Tv)
        .await
        .unwrap_or_default();
    let one = |episode: u32, left: u64, state: DownloadState, title: &str| Downloading {
        title_id: FAWLTY,
        release: format!("Release.S01E0{episode}"),
        bytes: 2_000_000,
        left,
        expected: None,
        state,
        download_id: Some(format!("hash-{episode}")),
        queued: None,
        episode: Some(DownloadedEpisode {
            season: 1,
            number: episode,
            title: title.to_owned(),
        }),
    };
    assert_eq!(
        downloading,
        [
            one(1, 0, DownloadState::Completed, "Episode 1"),
            one(2, 2_000_000, DownloadState::Downloading, "Episode 2"),
            one(3, 2_000_000, DownloadState::Paused, ""),
            one(4, 2_000_000, DownloadState::Queued, ""),
            one(5, 2_000_000, DownloadState::Failed, ""),
        ]
    );
}

#[tokio::test]
async fn nothing_downloading_asks_for_no_series() {
    let fake = Fake::always(json(200, &json!([])));
    assert_eq!(
        medusa(&fake).downloading(TitleKind::Tv).await.ok(),
        Some(Vec::new())
    );
    assert_eq!(asked(&fake).len(), 1);
}

fn looking_up(fake: &std::sync::Arc<Fake>) -> crate::medusa::Medusa {
    medusa(fake).looking_up(Some(std::sync::Arc::new(crate::thetvdb::TheTvdb::new(
        fake.clone(),
        "https://api4.thetvdb.com/v4",
        crate::thetvdb::SignIn {
            key: "operator-key".to_owned(),
            pin: None,
        },
    ))))
}

fn thetvdb_knowing(series: &[Value]) -> std::sync::Arc<Fake> {
    Fake::by_route_in_turn(vec![
        (
            Method::Post,
            "/login",
            vec![json(200, &json!({ "data": { "token": "t" } }))],
        ),
        (
            Method::Get,
            "/series/81189/episodes/default",
            vec![json(
                200,
                &json!({ "data": { "series": { "name": "Breaking Bad", "year": "2008" }, "episodes": [{ "seasonNumber": 1 }] } }),
            )],
        ),
        (Method::Get, "/episodes", vec![json(200, &episodes())]),
        (
            Method::Get,
            "/series?",
            vec![json(200, &Value::Array(series.to_vec()))],
        ),
    ])
}

#[tokio::test]
async fn with_the_lookup_on_a_series_it_does_not_hold_is_catalogued_from_thetvdb() {
    let fake = thetvdb_knowing(&[]);
    assert_eq!(
        looking_up(&fake).catalogued(Kind::Tv, BAD).await.ok(),
        Some(Some(CuratedTitle {
            reference: BAD,
            id: None,
            title: "Breaking Bad".to_owned(),
            year: Some(2008),
            monitored: false,
            files: 0,
            seasons: vec![CuratedSeason {
                number: 1,
                monitored: false,
                files: 0,
                episodes: 1
            }],
        }))
    );
}

#[tokio::test]
async fn with_the_lookup_on_a_series_it_holds_is_answered_from_medusa_alone() {
    let fake = thetvdb_knowing(&[fawlty(false, "Wanted")]);
    let medusa = looking_up(&fake);
    assert_eq!(
        medusa
            .catalogued(Kind::Tv, FAWLTY)
            .await
            .ok()
            .flatten()
            .map(|title| title.id),
        Some(Some(FAWLTY))
    );
    assert_eq!(medusa.held_title(Kind::Tv, BAD).await.ok(), Some(None));
    assert_eq!(medusa.catalogued(Kind::Tv, 0).await.ok(), Some(None));
    assert!(!asked(&fake)
        .iter()
        .any(|(_, url, _)| url.contains("thetvdb")));
}

#[tokio::test]
async fn with_tvmaze_looked_up_in_a_series_it_does_not_hold_is_catalogued_from_tvmaze_alone() {
    let fake = Fake::by_route_in_turn(vec![
        (
            Method::Get,
            "/lookup/shows?thetvdb=81189",
            vec![Answer::Headed {
                status: 301,
                headers: vec![(
                    "Location".to_owned(),
                    "https://api.tvmaze.com/shows/169".to_owned(),
                )],
                body: String::new(),
            }],
        ),
        (
            Method::Get,
            "/shows/169?embed=episodes",
            vec![json(
                200,
                &json!({ "name": "Breaking Bad", "premiered": "2008-01-20", "externals": { "thetvdb": BAD }, "_embedded": { "episodes": [{ "season": 1, "number": 1 }] } }),
            )],
        ),
        (Method::Get, "/series?", vec![json(200, &json!([]))]),
    ]);
    let medusa = medusa(&fake).looking_up(Some(std::sync::Arc::new(crate::tvmaze::TvMaze::new(
        fake.clone(),
        crate::tvmaze::ADDRESS,
    ))));
    assert_eq!(
        medusa
            .catalogued(Kind::Tv, BAD)
            .await
            .ok()
            .flatten()
            .map(|title| (title.title, title.year, title.seasons.len())),
        Some(("Breaking Bad".to_owned(), Some(2008), 1))
    );
    assert!(!asked(&fake)
        .iter()
        .any(|(_, url, _)| url.contains("thetvdb.com")));
}
