//! Every conformance recording, read by the client the core asks the adapter with.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use lemonfiber_contract::client::Witness;
use lemonfiber_contract::conformance::{filed, Expect};
use lemonfiber_contract::{Contracted, Failure};
use lemonfiber_ports::http::{Http, Request, Response, Unreachable};

const ACCEPT: &str = "application/json";

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct Recording {
    recorded_from: String,
    note: String,
    request: Asked,
    response: Answer,
}

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct Asked {
    method: String,
    path: String,
    accept: Option<String>,
}

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct Answer {
    status: u16,
    #[serde(default)]
    headers: BTreeMap<String, String>,
    #[serde(default)]
    json: Option<serde_json::Value>,
    #[serde(default)]
    body_starts_with: Option<String>,
}

struct Replayed(Response);

#[async_trait]
impl Http for Replayed {
    async fn send(&self, _request: &Request) -> Result<Response, Unreachable> {
        Ok(self.0.clone())
    }
}

#[derive(Default)]
struct Told(Mutex<Vec<String>>);

impl Witness for Told {
    fn nonconforming(&self, _operation: &str, why: &str) {
        if let Ok(mut told) = self.0.lock() {
            told.push(why.to_owned());
        }
    }
}

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn manifest() -> toml::Table {
    std::fs::read_to_string(root().join("plugin.toml"))
        .ok()
        .and_then(|text| text.parse().ok())
        .unwrap_or_default()
}

fn services() -> Vec<toml::Table> {
    manifest()
        .get("service")
        .and_then(toml::Value::as_array)
        .map(|services| {
            services
                .iter()
                .filter_map(|one| one.as_table().cloned())
                .collect()
        })
        .unwrap_or_default()
}

fn text(table: &toml::Table, field: &str) -> String {
    table
        .get(field)
        .and_then(toml::Value::as_str)
        .unwrap_or_default()
        .to_owned()
}

fn plugin_id() -> String {
    manifest()
        .get("plugin")
        .and_then(toml::Value::as_table)
        .map(|plugin| text(plugin, "id"))
        .unwrap_or_default()
}

fn upstream() -> toml::Table {
    let id = plugin_id();
    services()
        .into_iter()
        .find(|one| text(one, "id") == id)
        .unwrap_or_default()
}

fn pinned() -> String {
    let upstream = upstream();
    format!("{}@{}", text(&upstream, "image"), text(&upstream, "digest"))
}

fn files() -> Vec<String> {
    upstream()
        .get("media_types")
        .and_then(toml::Value::as_array)
        .map(|types| {
            types
                .iter()
                .filter_map(|one| one.as_str().map(str::to_owned))
                .collect()
        })
        .unwrap_or_default()
}

fn spoken() -> Vec<String> {
    medusa_adapter::adapter::about().speaks
}

fn read(path: &Path) -> Result<Recording, String> {
    let text = std::fs::read_to_string(path).map_err(|why| why.to_string())?;
    serde_json::from_str(&text).map_err(|why| why.to_string())
}

fn replayed(answer: &Answer) -> Response {
    Response {
        status: answer.status,
        headers: answer
            .headers
            .iter()
            .map(|(name, value)| (name.clone(), value.clone()))
            .collect(),
        body: answer
            .json
            .as_ref()
            .map(ToString::to_string)
            .or_else(|| answer.body_starts_with.clone())
            .unwrap_or_default(),
    }
}

#[tokio::test]
async fn every_case_its_upstream_is_asked_is_recorded_and_read_as_the_core_reads_it() {
    let pinned = pinned();
    let spoken = spoken();
    let files = files();
    assert_eq!(spoken, ["indexer.pull@1", "library.curate@1"]);
    let mut faults = Vec::new();
    for capability in lemonfiber_contract::capabilities::all() {
        let named = lemonfiber_contract::spoken(capability.name, capability.major);
        if !spoken.contains(&named) {
            continue;
        }
        let asked = filed(&capability, &files);
        let held = std::fs::read_dir(root().join("conformance").join(&named))
            .map(|entries| {
                entries
                    .filter_map(|entry| entry.ok()?.file_name().into_string().ok())
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        faults.extend(
            held.iter()
                .filter(|name| {
                    !asked
                        .iter()
                        .any(|case| **name == format!("{}.json", case.case))
                })
                .map(|name| format!("{named}/{name}: answers no case asked")),
        );
        for case in asked {
            let path = root()
                .join("conformance")
                .join(&named)
                .join(format!("{}.json", case.case));
            let recording = match read(&path) {
                Ok(recording) => recording,
                Err(why) => {
                    faults.push(format!("{named}/{}: {why}", case.case));
                    continue;
                }
            };
            let asked = &recording.request;
            if recording.recorded_from != pinned
                || recording.note.is_empty()
                || asked.method != "POST"
                || asked.path != case.operation.path()
                || asked.accept.as_deref() != Some(ACCEPT)
            {
                faults.push(format!(
                    "{named}/{}: not a recording of this case",
                    case.case
                ));
                continue;
            }
            match case.expect {
                Expect::Status(statuses) => {
                    if !statuses.contains(&recording.response.status) {
                        faults.push(format!(
                            "{named}/{}: answered {}",
                            case.case, recording.response.status
                        ));
                    }
                }
                Expect::Conforms => {
                    let told = Arc::new(Told::default());
                    let client = Contracted::new(
                        Arc::new(Replayed(replayed(&recording.response))),
                        "http://recorded",
                        "medusa-adapter",
                        "recorded",
                    )
                    .witnessed_by(told.clone());
                    let read = case.operation.judged(&client).await;
                    let said = told.0.lock().map(|said| said.clone()).unwrap_or_default();
                    if matches!(read, Err(Failure::Unauthorised { .. })) || !said.is_empty() {
                        faults.push(format!("{named}/{}: {said:?}", case.case));
                    }
                }
            }
        }
    }
    assert!(faults.is_empty(), "{faults:#?}");
}

#[test]
fn the_adapter_names_the_release_and_digest_its_recordings_were_taken_from() {
    let about = medusa_adapter::adapter::about();
    let upstream = upstream();
    assert_eq!(
        about
            .releases
            .iter()
            .map(|release| (release.version.clone(), release.digest.clone()))
            .collect::<Vec<_>>(),
        [(text(&upstream, "tag"), text(&upstream, "digest"))]
    );
}
