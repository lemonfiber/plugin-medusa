use std::sync::Arc;

use lemonfiber_adapter_kit::recounting::{Entry, History};
use lemonfiber_fixtures::http::{Answer, Fake};
use lemonfiber_ports::service::{Event, Failure, Happened, Recounting as _};
use serde_json::{json, Value};

use super::super::tests::{json, medusa, refused, so, why};
use super::{Happenings, Medusa};

fn series() -> Answer {
    json(
        200,
        &json!([{ "id": { "tvdb": 371_980, "slug": "tvdb371980" }, "title": "Severance" }]),
    )
}

fn row(id: u64, status: i64, date: u64) -> Value {
    json!({
        "id": id,
        "series": "tvdb371980",
        "status": status,
        "actionDate": date,
        "season": 1,
        "episode": id,
        "showSlug": "tvdb371980",
        "showTitle": "Severance",
        "episodeTitle": format!("Severance - s01e{id:02}"),
    })
}

fn rows(rows: &[Value]) -> Answer {
    json(200, &Value::Array(rows.to_vec()))
}

fn held(fake: &Arc<Fake>) -> Arc<Medusa> {
    Arc::new(medusa(fake))
}

fn heard(medusa: &Arc<Medusa>) -> Happenings {
    Happenings(Arc::downgrade(medusa))
}

fn told(event: Happened, id: u64) -> Event {
    Event {
        event,
        reference: Some("371980".to_owned()),
        name: format!("Severance - s01e{id:02}"),
        reason: None,
        cleared: false,
    }
}

#[tokio::test]
async fn the_newest_is_the_highest_row_id_among_the_newest_rows() {
    let fake = Fake::by_path(vec![(
        "/history?",
        rows(&[
            row(7, 2, 20_261_111_120_000),
            row(9, 2, 20_261_111_120_000),
            row(8, 2, 20_261_111_120_000),
            row(6, 4, 20_261_111_110_000),
            row(5, 4, 20_261_111_100_000),
        ]),
    )]);
    let medusa = held(&fake);
    assert_eq!(heard(&medusa).newest().await.ok().as_deref(), Some("9"));
    let asked: Vec<String> = fake.requests().into_iter().map(|one| one.url).collect();
    assert_eq!(
        asked,
        ["http://medusa:8081/api/v2/history?limit=1000&sort=%5B%7B%22field%22%3A%22date%22%2C%22type%22%3A%22desc%22%7D%5D&page=1"]
    );
}

#[tokio::test]
async fn an_empty_history_s_newest_is_where_it_begins() {
    let fake = Fake::by_path(vec![("/history?", rows(&[]))]);
    let medusa = held(&fake);
    let heard = heard(&medusa);
    assert_eq!(heard.newest().await.ok().as_deref(), Some("0"));
    assert_eq!(heard.after("0").await.ok(), Some(Some(Vec::new())));
}

#[tokio::test]
async fn what_followed_the_cursor_comes_oldest_first_imports_and_failures_told() {
    let fake = Fake::by_path(vec![
        (
            "/history?",
            rows(&[
                row(14, 11, 20_261_111_130_000),
                row(13, 4, 20_261_111_120_000),
                row(12, 2, 20_261_111_120_000),
                row(10, 4, 20_261_111_110_000),
                row(11, 10, 20_261_111_110_000),
                row(9, 4, 20_261_111_100_000),
                row(8, 4, 20_261_111_090_000),
            ]),
        ),
        ("/series", series()),
    ]);
    let medusa = held(&fake);
    let after = heard(&medusa).after("10").await.ok().flatten();
    assert_eq!(
        after,
        Some(vec![
            Entry {
                position: "11".to_owned(),
                event: None,
            },
            Entry {
                position: "12".to_owned(),
                event: None,
            },
            Entry {
                position: "13".to_owned(),
                event: Some(told(Happened::Imported, 13)),
            },
            Entry {
                position: "14".to_owned(),
                event: Some(told(Happened::Failed, 14)),
            },
        ])
    );
}

#[tokio::test]
async fn rows_that_tell_nothing_move_the_cursor_without_reading_the_series() {
    let fake = Fake::by_path(vec![(
        "/history?",
        rows(&[row(3, 2, 20_261_111_120_000), row(2, 9, 20_261_111_110_000)]),
    )]);
    let medusa = held(&fake);
    assert_eq!(
        heard(&medusa).after("1").await.ok().flatten(),
        Some(vec![
            Entry {
                position: "2".to_owned(),
                event: None,
            },
            Entry {
                position: "3".to_owned(),
                event: None,
            },
        ])
    );
    assert!(!fake.asked_for("/series"));
}

#[tokio::test]
async fn a_series_medusa_no_longer_follows_is_told_by_name_alone() {
    let fake = Fake::by_path(vec![
        ("/history?", rows(&[row(2, 4, 20_261_111_120_000)])),
        ("/series", json(200, &json!([]))),
    ]);
    let medusa = held(&fake);
    let after = heard(&medusa).after("1").await.ok().flatten();
    assert_eq!(
        after,
        Some(vec![Entry {
            position: "2".to_owned(),
            event: Some(Event {
                reference: None,
                ..told(Happened::Imported, 2)
            }),
        }])
    );
}

#[tokio::test]
async fn a_full_page_reads_on_until_a_row_older_than_the_cursor_s() {
    let newer: Vec<Value> = (101..=1100)
        .rev()
        .map(|id| row(id, 2, 20_261_112_000_000 + id))
        .collect();
    let older = [
        row(1102, 4, 20_261_111_000_100),
        row(100, 2, 20_261_111_000_100),
        row(1101, 4, 20_261_111_000_100),
        row(99, 2, 20_261_111_000_099),
        row(1103, 4, 20_261_111_000_098),
    ];
    let fake = Fake::by_path(vec![
        ("page=1", rows(&newer)),
        ("page=2", rows(&older)),
        ("/series", series()),
    ]);
    let medusa = held(&fake);
    let after = heard(&medusa)
        .after("100")
        .await
        .ok()
        .flatten()
        .unwrap_or_default();
    let positions: Vec<String> = after.iter().map(|entry| entry.position.clone()).collect();
    let expected: Vec<String> = (101..=1102).map(|id| id.to_string()).collect();
    assert_eq!(positions, expected);
    assert_eq!(
        after.iter().filter(|entry| entry.event.is_some()).count(),
        2
    );
    assert!(!fake.asked_for("page=3"));
}

#[tokio::test]
async fn a_cursor_medusa_no_longer_holds_or_cannot_be_read_is_not_held() {
    let fake = Fake::by_path(vec![(
        "/history?",
        rows(&[row(3, 4, 20_261_111_120_000), row(2, 4, 20_261_111_110_000)]),
    )]);
    let medusa = held(&fake);
    let heard = heard(&medusa);
    assert_eq!(heard.after("50").await.ok(), Some(None));
    assert_eq!(heard.after("not a cursor").await.ok(), Some(None));
    assert_eq!(heard.after("-1").await.ok(), Some(None));
}

#[tokio::test]
async fn a_history_medusa_will_not_give_is_a_failure() {
    let fake = Fake::always(Answer::reply(500, ""));
    let medusa = held(&fake);
    assert!(heard(&medusa).after("1").await.is_err());
    assert!(heard(&medusa).newest().await.is_err());
}

#[tokio::test]
async fn a_medusa_the_adapter_no_longer_holds_is_not_answering() {
    let fake = Fake::silent();
    let heard = heard(&held(&fake));
    assert!(matches!(
        heard.newest().await,
        Err(Failure::Unavailable { .. })
    ));
    assert!(matches!(
        heard.after("1").await,
        Err(Failure::Unavailable { .. })
    ));
}

#[tokio::test]
async fn asked_with_nowhere_to_keep_a_cursor_it_refuses_and_asks_medusa_nothing() {
    let fake = Fake::silent();
    let said = medusa(&fake).events(None, None, None, None).await;
    assert_eq!(
        why(said),
        so(refused(
            "this adapter was started with nowhere to keep its cursor"
        ))
    );
    assert!(fake.requests().is_empty());
}

#[test]
fn a_row_s_instant_is_read_as_medusa_writes_it() {
    assert_eq!(
        super::instant(&json!(20_261_111_120_000_u64)),
        20_261_111_120_000
    );
    assert_eq!(
        super::instant(&json!(" 20261111120000 ")),
        20_261_111_120_000
    );
    assert_eq!(super::instant(&json!("soon")), 0);
    assert_eq!(super::instant(&Value::Null), 0);
}
