use lemonfiber_fixtures::http::{Answer, Fake};
use lemonfiber_ports::http::Method;
use lemonfiber_ports::service::{
    Category, Credential, DownloadClient, Protocol, RegisteredClient, RootFolder,
};
use lemonfiber_ports::Client as _;
use serde_json::{json, Value};

use super::{reached, Slot};
use crate::medusa::tests::{asked, json, medusa, refused, so, unsupported, why};

fn client(protocol: &str, credential: Credential) -> DownloadClient {
    DownloadClient {
        name: "conformance".to_owned(),
        host: "conformance-download".to_owned(),
        port: 8080,
        protocol: Protocol(protocol.to_owned()),
        credential,
        category: Category {
            field: "tvCategory".to_owned(),
            value: "tv".to_owned(),
        },
    }
}

fn key() -> Credential {
    Credential::ApiKey("the-client-key".to_owned())
}

fn account() -> Credential {
    Credential::UserPass {
        username: "admin".to_owned(),
        password: "the-client-password".to_owned(),
    }
}

fn echoing() -> std::sync::Arc<Fake> {
    Fake::always(Answer::reply(200, "{}"))
}

fn categories(value: &str) -> Value {
    json!({
        "category": value,
        "categoryBacklog": value,
        "categoryAnime": value,
        "categoryAnimeBacklog": value,
    })
}

const HANDLED: &str = r#"{ "downloadHandler": { "enabled": true, "frequency": 5 } }"#;

fn handled() -> Value {
    serde_json::from_str(HANDLED).unwrap_or_default()
}

async fn written(protocol: &str, credential: Credential) -> Option<Value> {
    let fake = echoing();
    let _ = medusa(&fake)
        .register_download_client(&client(protocol, credential))
        .await;
    asked(&fake)
        .into_iter()
        .next()
        .and_then(|(_, _, body)| body)
}

#[tokio::test]
async fn it_names_itself_and_the_release_it_runs() {
    let fake = Fake::always(Answer::reply(200, "\"1.0.27\""));
    let identity = medusa(&fake).identity().await.ok();
    assert_eq!(
        identity.map(|one| (one.name, one.version)),
        Some(("Medusa".to_owned(), "1.0.27".to_owned()))
    );
    assert_eq!(
        asked(&fake),
        [(
            Method::Get,
            "/api/v2/config/system/release".to_owned(),
            None
        )]
    );
    let silent = Fake::always(Answer::reply(200, "\"\""));
    assert_eq!(
        why(medusa(&silent).identity().await),
        so(refused("Medusa named no release"))
    );
}

#[tokio::test]
async fn sabnzbd_is_written_into_the_usenet_slot_with_its_key_and_the_category_everywhere() {
    let mut sabnzbd = json!({ "host": "http://conformance-download:8080/", "apiKey": "the-client-key", "username": "", "password": "" });
    if let (Some(at), Value::Object(more)) = (sabnzbd.as_object_mut(), categories("tv")) {
        at.extend(more);
    }
    assert_eq!(
        written("sabnzbd", key()).await,
        Some(json!({
            "clients": { "nzb": { "enabled": true, "method": "sabnzbd", "sabnzbd": sabnzbd } },
            "postProcessing": handled(),
        }))
    );
}

#[tokio::test]
async fn sabnzbd_behind_an_account_is_written_with_it_and_no_key() {
    let written = written("sabnzbd", account()).await;
    assert_eq!(
        written
            .as_ref()
            .and_then(|body| body.pointer("/clients/nzb/sabnzbd/username")),
        Some(&json!("admin"))
    );
    assert_eq!(
        written
            .as_ref()
            .and_then(|body| body.pointer("/clients/nzb/sabnzbd/apiKey")),
        Some(&json!(""))
    );
}

#[tokio::test]
async fn nzbget_is_written_by_host_and_port_with_its_account() {
    let written = written("nzbget", account()).await;
    let nzbget = written
        .as_ref()
        .and_then(|body| body.pointer("/clients/nzb/nzbget"));
    assert_eq!(
        nzbget.and_then(|one| one.get("host")),
        Some(&json!("conformance-download:8080"))
    );
    assert_eq!(
        nzbget.and_then(|one| one.get("useHttps")),
        Some(&json!(false))
    );
    assert_eq!(
        nzbget.and_then(|one| one.get("password")),
        Some(&json!("the-client-password"))
    );
    assert_eq!(
        written
            .as_ref()
            .and_then(|body| body.pointer("/clients/nzb/method")),
        Some(&json!("nzbget"))
    );
}

#[tokio::test]
async fn a_torrent_client_is_written_into_the_torrent_slot_labelled_with_the_category() {
    assert_eq!(
        written("qbittorrent", account()).await,
        Some(json!({
            "clients": { "torrents": {
                "enabled": true,
                "method": "qbittorrent",
                "host": "http://conformance-download:8080/",
                "username": "admin",
                "password": "the-client-password",
                "label": "tv",
                "labelAnime": "tv",
            } },
            "postProcessing": handled(),
        }))
    );
}

#[tokio::test]
async fn a_client_it_cannot_sign_in_to_or_does_not_speak_is_unsupported_and_nothing_is_written() {
    for (protocol, credential, detail) in [
        (
            "qbittorrent",
            key(),
            "it signs in to qbittorrent with a name and password, not a key",
        ),
        (
            "nzbget",
            key(),
            "it signs in to nzbget with a name and password, not a key",
        ),
        (
            "rtorrent",
            account(),
            "it files no download client spoken to as rtorrent",
        ),
    ] {
        let fake = echoing();
        assert_eq!(
            why(medusa(&fake)
                .register_download_client(&client(protocol, credential))
                .await),
            so(unsupported(detail))
        );
        assert!(fake.requests().is_empty());
    }
}

fn clients(nzb: &Value, torrents: &Value) -> std::sync::Arc<Fake> {
    Fake::always(json(200, &json!({ "nzb": nzb, "torrents": torrents })))
}

fn off() -> Value {
    json!({ "enabled": false, "method": "blackhole", "host": "" })
}

#[tokio::test]
async fn the_clients_it_holds_are_read_by_where_they_are_reached_and_their_category() {
    let fake = clients(
        &json!({ "enabled": true, "method": "sabnzbd", "sabnzbd": { "host": "http://sabnzbd:8080/", "category": "tv", "apiKey": "x" } }),
        &json!({ "enabled": true, "method": "qbittorrent", "host": "https://qbittorrent", "label": "series" }),
    );
    assert_eq!(
        medusa(&fake).download_clients().await.ok(),
        Some(vec![
            RegisteredClient {
                id: "nzb".to_owned(),
                host: "sabnzbd".to_owned(),
                port: 8080,
                category: Some(Category {
                    field: "category".to_owned(),
                    value: "tv".to_owned(),
                }),
            },
            RegisteredClient {
                id: "torrent".to_owned(),
                host: "qbittorrent".to_owned(),
                port: 443,
                category: Some(Category {
                    field: "label".to_owned(),
                    value: "series".to_owned(),
                }),
            },
        ])
    );
    assert_eq!(
        asked(&fake),
        [(Method::Get, "/api/v2/config/clients".to_owned(), None)]
    );
}

#[tokio::test]
async fn nzbget_is_read_by_its_host_and_port() {
    let fake = clients(
        &json!({ "enabled": true, "method": "nzbget", "nzbget": { "host": "nzbget:6789", "category": "tv" } }),
        &off(),
    );
    assert_eq!(
        medusa(&fake).download_clients().await.ok().map(|held| held
            .into_iter()
            .map(|one| (one.id, one.host, one.port))
            .collect::<Vec<_>>()),
        Some(vec![("nzb".to_owned(), "nzbget".to_owned(), 6789)])
    );
}

#[tokio::test]
async fn a_blackhole_a_switched_off_slot_or_an_address_that_reaches_nothing_is_no_client() {
    for (nzb, torrents) in [
        (
            json!({ "enabled": false, "method": "sabnzbd", "sabnzbd": { "host": "http://sabnzbd:8080/" } }),
            off(),
        ),
        (
            json!({ "enabled": true, "method": "blackhole" }),
            json!({ "enabled": true, "method": "blackhole", "host": "http://x/" }),
        ),
        (
            json!({ "enabled": true, "method": "sabnzbd", "sabnzbd": { "host": "" } }),
            json!({ "enabled": true, "method": "transmission", "host": "" }),
        ),
        (
            json!({ "enabled": true, "method": "nzbget", "nzbget": { "host": "nzbget" } }),
            json!({ "enabled": true, "method": "deluge", "host": "http://deluge:port/" }),
        ),
    ] {
        let fake = clients(&nzb, &torrents);
        assert_eq!(
            medusa(&fake).download_clients().await.ok(),
            Some(Vec::new())
        );
    }
}

#[test]
fn an_address_is_read_with_the_port_its_scheme_implies() {
    assert_eq!(
        reached("http://sabnzbd/sabnzbd/", false),
        Some(("sabnzbd".to_owned(), 80))
    );
    assert_eq!(
        reached("HTTPS://sabnzbd", false),
        Some(("sabnzbd".to_owned(), 443))
    );
    assert_eq!(
        reached("sabnzbd:9090", false),
        Some(("sabnzbd".to_owned(), 9090))
    );
    assert_eq!(reached("nzbget", true), None);
    assert_eq!(reached("http://:8080/", false), None);
}

#[tokio::test]
async fn a_client_is_updated_in_its_own_slot_and_refused_in_another() {
    let fake = echoing();
    let _ = medusa(&fake)
        .update_download_client("torrent", &client("qbittorrent", account()))
        .await;
    assert_eq!(
        asked(&fake)
            .into_iter()
            .map(|(method, path, _)| (method, path))
            .collect::<Vec<_>>(),
        [(Method::Patch, "/api/v2/config/main".to_owned())]
    );
    let refusing = echoing();
    assert_eq!(
        why(medusa(&refusing)
            .update_download_client("nzb", &client("qbittorrent", account()))
            .await),
        so(refused(
            "Medusa holds its qbittorrent client as torrent, not as nzb"
        ))
    );
    assert_eq!(
        why(medusa(&refusing)
            .update_download_client("7", &client("qbittorrent", account()))
            .await),
        so(refused("Medusa holds no download client called 7"))
    );
    assert!(refusing.requests().is_empty());
}

#[tokio::test]
async fn the_category_of_a_torrent_client_is_set_as_its_label() {
    let fake = echoing();
    let _ = medusa(&fake)
        .set_client_field("torrent", "tvCategory", None)
        .await;
    assert_eq!(
        asked(&fake)
            .into_iter()
            .next()
            .and_then(|(_, _, body)| body),
        Some(json!({ "clients": { "torrents": { "label": "", "labelAnime": "" } } }))
    );
}

#[tokio::test]
async fn the_category_of_the_usenet_client_is_set_on_the_client_medusa_uses() {
    let fake = Fake::by_route(vec![
        (
            Method::Get,
            "/config/clients",
            json(
                200,
                &json!({ "nzb": { "enabled": true, "method": "sabnzbd", "sabnzbd": { "host": "http://s:1/" } } }),
            ),
        ),
        (
            Method::Patch,
            "/config/main",
            json(
                200,
                &json!({ "clients": { "nzb": { "sabnzbd": categories("series") } } }),
            ),
        ),
    ]);
    assert!(medusa(&fake)
        .set_client_field("nzb", "category", Some("series"))
        .await
        .is_ok());
    let none = clients(&json!({ "enabled": false }), &off());
    assert_eq!(
        why(medusa(&none)
            .set_client_field("nzb", "category", Some("series"))
            .await),
        so(refused(
            "Medusa holds no Usenet client to set the category of"
        ))
    );
}

#[tokio::test]
async fn a_field_other_than_the_category_is_not_set_and_nothing_is_asked() {
    let fake = echoing();
    assert_eq!(
        why(medusa(&fake)
            .set_client_field("nzb", "host", Some("elsewhere"))
            .await),
        so(unsupported(
            "it sets only the category of a download client, not its host"
        ))
    );
    assert!(Slot::Torrent.takes("label"));
    assert!(!Slot::Nzb.takes("label"));
    assert!(fake.requests().is_empty());
}

#[tokio::test]
async fn with_no_client_held_nothing_is_tested_and_nobody_signs_in() {
    let fake = clients(&json!({ "enabled": false }), &off());
    assert_eq!(
        medusa(&fake)
            .test_download_clients()
            .await
            .ok()
            .map(|probes| probes.len()),
        Some(0)
    );
    assert_eq!(fake.requests().len(), 1);
}

#[tokio::test]
async fn every_client_held_is_tested_by_medusa_itself_and_answered_in_its_words() {
    let fake = Fake::by_rules(vec![
        (
            None,
            "/config/clients",
            json(
                200,
                &json!({
                    "nzb": { "enabled": true, "method": "sabnzbd", "sabnzbd": { "host": "http://sabnzbd:8080/", "apiKey": "k", "username": "", "password": "" } },
                    "torrents": { "enabled": true, "method": "qbittorrent", "host": "http://qbittorrent:8080/", "username": "u", "password": "p" },
                }),
            ),
        ),
        (
            Some(Method::Post),
            "/login/",
            Answer::Headed {
                status: 302,
                headers: vec![("Set-Cookie".to_owned(), "medusa_user=s; Path=/".to_owned())],
                body: String::new(),
            },
        ),
        (
            None,
            "/home/testSABnzbd",
            Answer::reply(200, "Success. Connected and authenticated"),
        ),
        (
            None,
            "/home/testTorrent",
            Answer::reply(200, " Unable to connect to host \n"),
        ),
    ]);
    let probes = medusa(&fake).test_download_clients().await.ok();
    assert_eq!(
        probes.map(|probes| probes
            .into_iter()
            .map(|one| (one.id, one.reachable, one.detail))
            .collect::<Vec<_>>()),
        Some(vec![
            ("nzb".to_owned(), true, None),
            (
                "torrent".to_owned(),
                false,
                Some("Unable to connect to host".to_owned())
            ),
        ])
    );
    let paths: Vec<String> = asked(&fake).into_iter().map(|(_, path, _)| path).collect();
    assert_eq!(
        paths.get(2).map(String::as_str),
        Some("/home/testSABnzbd?host=http%3A%2F%2Fsabnzbd%3A8080%2F&username=&password=&apikey=k")
    );
    assert_eq!(
        paths.get(3).map(String::as_str),
        Some("/home/testTorrent?torrent_method=qbittorrent&host=http%3A%2F%2Fqbittorrent%3A8080%2F&username=u&password=p")
    );
}

#[tokio::test]
async fn nzbget_is_tested_through_its_own_page() {
    let fake = Fake::by_rules(vec![
        (
            None,
            "/config/clients",
            json(
                200,
                &json!({
                    "nzb": { "enabled": true, "method": "nzbget", "nzbget": { "host": "nzbget:6789", "username": "u", "password": "p", "useHttps": true } },
                    "torrents": off(),
                }),
            ),
        ),
        (
            Some(Method::Post),
            "/login/",
            Answer::Headed {
                status: 302,
                headers: vec![("set-cookie".to_owned(), "medusa_user=s".to_owned())],
                body: String::new(),
            },
        ),
        (None, "/home/testNZBget", Answer::reply(200, "")),
    ]);
    let probes = medusa(&fake).test_download_clients().await.ok();
    assert_eq!(
        probes.map(|probes| probes
            .into_iter()
            .map(|one| (one.id, one.reachable, one.detail))
            .collect::<Vec<_>>()),
        Some(vec![("nzb".to_owned(), false, None)])
    );
    assert!(fake.asked_for("/home/testNZBget?host=nzbget%3A6789&username=u&password=p&use_https=1"));
    let other = clients(
        &json!({ "enabled": true, "method": "nzbget", "nzbget": { "host": "" } }),
        &off(),
    );
    assert_eq!(
        medusa(&other)
            .test_download_clients()
            .await
            .ok()
            .map(|probes| probes.len()),
        Some(0)
    );
}

fn main(root_dirs: &Value) -> Answer {
    json(
        200,
        &json!({ "rootDirs": root_dirs, "showDefaults": { "quality": 6 } }),
    )
}

#[tokio::test]
async fn a_folder_is_added_to_the_folders_it_files_into_once() {
    for (held, written) in [
        (json!([]), json!(["0", "/data/tv"])),
        (
            json!(["0", "/elsewhere"]),
            json!(["0", "/elsewhere", "/data/tv"]),
        ),
    ] {
        let fake = Fake::by_route(vec![
            (Method::Get, "/config/main", main(&held)),
            (
                Method::Patch,
                "/config/main",
                json(200, &json!({ "rootDirs": written })),
            ),
        ]);
        assert!(medusa(&fake)
            .register_root_folder(&RootFolder {
                path: "/data/tv".to_owned(),
                media_type: "tv".to_owned(),
            })
            .await
            .is_ok());
        assert_eq!(
            asked(&fake)
                .into_iter()
                .nth(1)
                .and_then(|(_, _, body)| body),
            Some(json!({ "rootDirs": written }))
        );
    }
    let held = Fake::always(main(&json!(["0", "/data/tv"])));
    assert!(medusa(&held)
        .register_root_folder(&RootFolder {
            path: "/data/tv".to_owned(),
            media_type: "tv".to_owned(),
        })
        .await
        .is_ok());
    assert_eq!(held.requests().len(), 1);
}

#[tokio::test]
async fn a_folder_for_anything_but_series_is_unsupported() {
    let fake = echoing();
    assert_eq!(
        why(medusa(&fake)
            .register_root_folder(&RootFolder {
                path: "/data/movies".to_owned(),
                media_type: "movies".to_owned(),
            })
            .await),
        so(unsupported("it files no movies"))
    );
    assert!(fake.requests().is_empty());
}

#[tokio::test]
async fn the_folders_it_files_into_are_named_by_their_paths() {
    let fake = Fake::always(main(&json!(["1", "/a", "/b"])));
    assert_eq!(
        medusa(&fake).root_folders().await.ok().map(|held| held
            .into_iter()
            .map(|one| (one.id, one.path))
            .collect::<Vec<_>>()),
        Some(vec![
            ("/a".to_owned(), "/a".to_owned()),
            ("/b".to_owned(), "/b".to_owned())
        ])
    );
}

#[tokio::test]
async fn its_quality_presets_are_its_quality_profiles() {
    let fake = Fake::always(json(
        200,
        &json!({ "qualities": { "presets": [
        { "value": 6, "key": "sd", "name": "SD" },
        { "value": -1, "key": "odd", "name": "Odd" },
        { "value": 7, "key": "nameless", "name": "" },
    ] } }),
    ));
    assert_eq!(
        medusa(&fake).quality_profiles().await.ok().map(|held| held
            .into_iter()
            .map(|one| (one.id, one.name))
            .collect::<Vec<_>>()),
        Some(vec![(6, "SD".to_owned())])
    );
}
