use std::sync::Arc;
use std::time::Duration;

use lemonfiber_fixtures::http::{Answer, Fake};
use lemonfiber_ports::service::{CuratedSeason, CuratedTitle, Failure};
use serde_json::{json, Value};

use super::{TvMaze, ADDRESS};
use crate::lookup::Lookup as _;

const THRONES: i64 = 121_361;

fn tvmaze(fake: &Arc<Fake>) -> TvMaze {
    TvMaze::new(fake.clone(), ADDRESS)
}

fn moved(to: &str) -> Answer {
    Answer::Headed {
        status: 301,
        headers: vec![("location".to_owned(), to.to_owned())],
        body: String::new(),
    }
}

fn show(thetvdb: &Value, premiered: &Value, episodes: &Value) -> Answer {
    Answer::reply(
        200,
        json!({
            "id": 82,
            "name": "Game of Thrones",
            "premiered": premiered,
            "externals": { "tvrage": 24493, "thetvdb": thetvdb, "imdb": "tt0944947" },
            "_embedded": { "episodes": episodes },
        })
        .to_string(),
    )
}

fn aired(seasons: &[(u32, u32)]) -> Value {
    Value::Array(
        seasons
            .iter()
            .flat_map(|(season, count)| {
                (1..=*count).map(
                    move |number| json!({ "season": season, "number": number, "type": "regular" }),
                )
            })
            .collect(),
    )
}

fn knowing(answers: Vec<Answer>) -> Arc<Fake> {
    Fake::by_path_in_turn(vec![
        ("/lookup/shows?thetvdb=121361", answers),
        (
            "/shows/82?embed=episodes",
            vec![show(
                &json!(THRONES),
                &json!("2011-04-17"),
                &aired(&[(1, 10), (2, 10), (8, 6)]),
            )],
        ),
    ])
}

fn season(number: u32, episodes: u32) -> CuratedSeason {
    CuratedSeason {
        number,
        monitored: false,
        files: 0,
        episodes,
    }
}

fn asked(fake: &Fake) -> Vec<String> {
    fake.requests().into_iter().map(|one| one.url).collect()
}

fn failed<T>(result: Result<T, Failure>) -> String {
    result
        .err()
        .map(|failure| format!("{failure:?}"))
        .unwrap_or_default()
}

fn refused(detail: &str) -> String {
    format!(
        "{:?}",
        Failure::Refused {
            service: "tvmaze".to_owned(),
            detail: detail.to_owned(),
        }
    )
}

fn unavailable() -> String {
    format!(
        "{:?}",
        Failure::Unavailable {
            service: "tvmaze".to_owned()
        }
    )
}

#[tokio::test]
async fn a_series_is_looked_up_by_its_thetvdb_id_and_read_with_its_episodes_from_the_show_named() {
    let fake = knowing(vec![moved("https://api.tvmaze.com/shows/82")]);
    assert_eq!(
        tvmaze(&fake).series(THRONES).await.ok(),
        Some(Some(CuratedTitle {
            reference: THRONES,
            id: None,
            title: "Game of Thrones".to_owned(),
            year: Some(2011),
            monitored: false,
            files: 0,
            seasons: vec![season(1, 10), season(2, 10), season(8, 6)],
        }))
    );
    assert_eq!(
        asked(&fake),
        vec![
            "https://api.tvmaze.com/lookup/shows?thetvdb=121361".to_owned(),
            "https://api.tvmaze.com/shows/82?embed=episodes".to_owned(),
        ]
    );
    let sent = fake.requests();
    assert!(sent.iter().all(|one| one.headers.is_empty()));
}

#[tokio::test]
async fn a_special_counts_in_no_season_and_a_year_it_cannot_read_is_none() {
    let fake = Fake::by_path_in_turn(vec![
        ("/lookup/", vec![moved("https://api.tvmaze.com/shows/82")]),
        (
            "/shows/82",
            vec![show(
                &json!(THRONES),
                &Value::Null,
                &json!([
                    { "season": 1, "number": 1, "type": "regular" },
                    { "season": 1, "number": null, "type": "significant_special" },
                ]),
            )],
        ),
    ]);
    let title = tvmaze(&fake).series(THRONES).await.ok().flatten();
    assert_eq!(
        title
            .as_ref()
            .map(|title| (title.year, title.seasons.clone())),
        Some((None, vec![season(1, 1)]))
    );
}

#[tokio::test]
async fn a_series_tvmaze_does_not_know_is_none() {
    let fake = knowing(vec![Answer::reply(404, "null")]);
    assert_eq!(tvmaze(&fake).series(THRONES).await.ok(), Some(None));
    assert_eq!(asked(&fake).len(), 1);
}

#[tokio::test]
async fn an_id_that_is_not_a_positive_number_is_never_asked_for() {
    let fake = knowing(Vec::new());
    assert_eq!(tvmaze(&fake).series(0).await.ok(), Some(None));
    assert_eq!(tvmaze(&fake).series(-121_361).await.ok(), Some(None));
    assert!(asked(&fake).is_empty());
}

#[tokio::test]
async fn a_lookup_sent_anywhere_but_one_of_its_own_shows_is_never_followed() {
    for location in [
        "https://elsewhere.example/shows/82",
        "https://api.tvmaze.com.elsewhere.example/shows/82",
        "https://api.tvmaze.com/shows/82/../../elsewhere",
        "https://api.tvmaze.com/shows/",
        "https://api.tvmaze.com/shows/99999999999999999999999",
        "/shows/82",
    ] {
        let fake = knowing(vec![moved(location)]);
        assert_eq!(
            failed(tvmaze(&fake).series(THRONES).await),
            refused("TVmaze did not name one of its own shows for the series"),
            "{location}"
        );
        assert_eq!(asked(&fake).len(), 1, "{location}");
    }
    let fake = knowing(vec![Answer::reply(301, "")]);
    assert_eq!(
        failed(tvmaze(&fake).series(THRONES).await),
        refused("TVmaze did not name one of its own shows for the series")
    );
}

#[tokio::test]
async fn a_show_naming_another_series_or_nothing_cannot_tell_which_series_it_is() {
    for (thetvdb, detail) in [
        (
            json!(71256),
            "TVmaze's show for the series names another TheTVDB id than 121361",
        ),
        (
            Value::Null,
            "TVmaze's show for the series names another TheTVDB id than 121361",
        ),
    ] {
        let fake = Fake::by_path_in_turn(vec![
            ("/lookup/", vec![moved("https://api.tvmaze.com/shows/82")]),
            (
                "/shows/82",
                vec![show(&thetvdb, &json!("2011-04-17"), &aired(&[(1, 1)]))],
            ),
        ]);
        assert_eq!(failed(tvmaze(&fake).series(THRONES).await), refused(detail));
    }
    let fake = Fake::by_path_in_turn(vec![
        ("/lookup/", vec![moved("https://api.tvmaze.com/shows/82")]),
        (
            "/shows/82",
            vec![Answer::reply(
                200,
                json!({ "name": " ", "externals": { "thetvdb": THRONES }, "_embedded": { "episodes": [] } })
                    .to_string(),
            )],
        ),
    ]);
    assert_eq!(
        failed(tvmaze(&fake).series(THRONES).await),
        refused("TVmaze names the series nothing")
    );
}

#[tokio::test]
async fn a_show_in_another_shape_is_refused_not_read() {
    for body in [
        json!({ "name": "Game of Thrones", "externals": { "thetvdb": THRONES } }),
        json!({ "name": "Game of Thrones", "externals": { "thetvdb": THRONES }, "_embedded": { "episodes": [{ "season": -1, "number": 1 }] } }),
    ] {
        let fake = Fake::by_path_in_turn(vec![
            ("/lookup/", vec![moved("https://api.tvmaze.com/shows/82")]),
            ("/shows/82", vec![Answer::reply(200, body.to_string())]),
        ]);
        assert!(
            matches!(
                tvmaze(&fake).series(THRONES).await,
                Err(Failure::Refused { ref detail, .. })
                    if detail.starts_with("TVmaze's show and its episodes could not be read")
            ),
            "{body}"
        );
    }
}

#[tokio::test(start_paused = true)]
async fn asked_too_often_it_is_asked_again_after_a_pause_and_at_last_unavailable() {
    let fake = knowing(vec![
        Answer::reply(429, ""),
        moved("https://api.tvmaze.com/shows/82"),
    ]);
    let started = tokio::time::Instant::now();
    assert!(tvmaze(&fake).series(THRONES).await.ok().flatten().is_some());
    assert_eq!(started.elapsed(), Duration::from_secs(2));
    assert_eq!(asked(&fake).len(), 3);

    let fake = knowing(vec![Answer::reply(429, "")]);
    let started = tokio::time::Instant::now();
    assert_eq!(
        failed(
            tvmaze(&fake)
                .backing_off(Duration::from_secs(5))
                .series(THRONES)
                .await
        ),
        unavailable()
    );
    assert_eq!(started.elapsed(), Duration::from_secs(10));
    assert_eq!(asked(&fake).len(), 3);
}

#[tokio::test]
async fn tvmaze_failing_or_unreached_is_a_failure_not_a_series() {
    let fake = knowing(vec![Answer::reply(500, "")]);
    assert_eq!(
        failed(tvmaze(&fake).series(THRONES).await),
        refused("HTTP 500")
    );
    assert_eq!(
        failed(tvmaze(&Fake::silent()).series(THRONES).await),
        unavailable()
    );
}
