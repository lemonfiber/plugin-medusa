use lemonfiber_fixtures::http::Fake;
use lemonfiber_ports::http::Method;
use lemonfiber_ports::service::stage::Stage;
use lemonfiber_ports::service::{Queued, Queues as _};
use serde_json::{json, Value};

use super::in_flight;
use crate::medusa::tests::{asked, json, medusa};
use crate::medusa::wire::HistoryRow;

fn row(status: i64, episode: u32, client: Option<&[i64]>) -> Value {
    json!({
        "status": status,
        "actionDate": 20_261_106_193_809_i64,
        "resource": format!("Show.S01E0{episode}.{status}"),
        "season": 1,
        "episode": episode,
        "infoHash": format!("hash-{episode}-{status}"),
        "showSlug": "tvdb1",
        "showTitle": "Show",
        "clientStatus": client.map(|bits| json!({ "status": bits, "string": "" })),
    })
}

fn rows(all: &[Value]) -> Vec<HistoryRow> {
    serde_json::from_value(Value::Array(all.to_vec())).unwrap_or_default()
}

fn read(all: &[Value]) -> Vec<(u32, &'static str, &'static str, Stage, u32)> {
    in_flight(rows(all))
        .into_iter()
        .map(|one| (one.episode, one.status, one.state, one.stage, one.grabs))
        .collect()
}

#[test]
fn a_snatch_is_in_flight_at_the_stage_its_client_says() {
    assert_eq!(
        read(&[
            row(2, 1, None),
            row(2, 2, Some(&[0])),
            row(9, 3, Some(&[2])),
            row(12, 4, Some(&[64])),
            row(2, 5, Some(&[1, 2])),
            row(2, 6, Some(&[128])),
            row(2, 7, Some(&[4, 8])),
            row(2, 8, Some(&[16])),
        ]),
        [
            (1, "ok", "queued", Stage::Grabbed, 1),
            (2, "ok", "queued", Stage::Grabbed, 1),
            (3, "ok", "downloading", Stage::Downloading, 1),
            (4, "ok", "downloading", Stage::Downloading, 1),
            (5, "warning", "downloading", Stage::Downloading, 1),
            (6, "ok", "importPending", Stage::Downloaded, 1),
            (7, "ok", "importPending", Stage::Downloaded, 1),
            (8, "error", "failedPending", Stage::Downloading, 1),
        ]
    );
}

#[test]
fn a_snatch_its_client_is_done_with_or_one_older_than_a_download_is_not_in_flight() {
    assert_eq!(
        read(&[
            row(2, 1, Some(&[128, 256])),
            row(2, 1, None),
            row(2, 2, Some(&[32])),
            row(2, 3, Some(&[1024])),
            row(2, 4, Some(&[512])),
            row(4, 5, None),
            row(2, 5, None),
            row(11, 6, None),
            row(2, 6, Some(&[2])),
            row(6, 7, None),
            row(2, 7, None),
            row(10, 8, None),
        ]),
        []
    );
}

#[test]
fn every_grab_since_the_last_download_is_counted_on_the_newest() {
    assert_eq!(
        read(&[
            row(2, 1, Some(&[2])),
            row(2, 1, Some(&[16])),
            row(12, 1, None),
            row(4, 1, None),
            row(2, 1, None),
        ]),
        [(1, "ok", "downloading", Stage::Downloading, 3)]
    );
}

#[tokio::test]
async fn the_queue_is_every_snatch_in_flight_newest_first() {
    let fake = Fake::always(json(
        200,
        &json!([row(2, 1, Some(&[2])), row(2, 2, Some(&[16]))]),
    ));
    let queue = medusa(&fake).queue().await.ok();
    assert_eq!(queue.as_ref().map(|queue| queue.total), Some(2));
    assert_eq!(
        queue.and_then(|queue| queue.items.into_iter().nth(1)),
        Some(Queued {
            title: "Show.S01E02.2".to_owned(),
            status: "error".to_owned(),
            state: "failedPending".to_owned(),
            message: None,
            download_id: Some("hash-2-2".to_owned()),
            grabs: 1,
        })
    );
    assert_eq!(
        asked(&fake),
        [(
            Method::Get,
            "/api/v2/history?limit=1000&sort=%5B%7B%22field%22%3A%22date%22%2C%22type%22%3A%22desc%22%7D%5D".to_owned(),
            None
        )]
    );
}

#[tokio::test]
async fn a_snatch_naming_no_hash_names_no_download() {
    let mut nameless = row(2, 1, None);
    if let Some(at) = nameless.as_object_mut() {
        at.insert("infoHash".to_owned(), json!(""));
    }
    let fake = Fake::always(json(200, &json!([nameless])));
    assert_eq!(
        medusa(&fake)
            .queue()
            .await
            .ok()
            .and_then(|queue| queue.items.into_iter().next())
            .map(|one| one.download_id),
        Some(None)
    );
}
