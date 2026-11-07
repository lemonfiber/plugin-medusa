use async_trait::async_trait;
use lemonfiber_adapter_kit::endpoint::form_encoded;
use lemonfiber_ports::service::{
    Category, ClientProbe, Credential, DownloadClient, Failure, Identity, QualityProfile,
    RegisteredClient, RegisteredFolder, RootFolder,
};
use lemonfiber_ports::Client;
use serde::Deserialize;
use serde_json::{json, Value};

use super::{unsupported, Medusa};

const CLIENTS: &str = "/config/clients";

const NAME: &str = "Medusa";

const BLACKHOLE: &str = "blackhole";

const SUCCEEDED: &str = "Success";

const NZB_CATEGORIES: [&str; 4] = [
    "category",
    "categoryBacklog",
    "categoryAnime",
    "categoryAnimeBacklog",
];

const TORRENT_LABELS: [&str; 2] = ["label", "labelAnime"];

const CATEGORY_SUFFIX: &str = "Category";

const HANDLER_MINUTES: u32 = 5;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Slot {
    Nzb,
    Torrent,
}

impl Slot {
    const fn id(self) -> &'static str {
        match self {
            Self::Nzb => "nzb",
            Self::Torrent => "torrent",
        }
    }

    fn named(id: &str) -> Option<Self> {
        [Self::Nzb, Self::Torrent]
            .into_iter()
            .find(|slot| slot.id() == id)
    }

    const fn field(self) -> &'static str {
        match self {
            Self::Nzb => "category",
            Self::Torrent => "label",
        }
    }

    fn takes(self, field: &str) -> bool {
        field == self.field() || field.ends_with(CATEGORY_SUFFIX)
    }
}

const SPOKEN: [(&str, Slot); 5] = [
    (SABNZBD, Slot::Nzb),
    (NZBGET, Slot::Nzb),
    ("qbittorrent", Slot::Torrent),
    ("transmission", Slot::Torrent),
    ("deluge", Slot::Torrent),
];

const NZBGET: &str = "nzbget";

const SABNZBD: &str = "sabnzbd";

#[derive(Deserialize, Default)]
struct NzbClient {
    #[serde(default)]
    host: String,
    #[serde(default)]
    category: String,
}

#[derive(Deserialize, Default)]
struct Nzb {
    #[serde(default)]
    enabled: bool,
    #[serde(default)]
    method: String,
    #[serde(default)]
    sabnzbd: NzbClient,
    #[serde(default)]
    nzbget: NzbClient,
}

#[derive(Deserialize, Default)]
struct Torrents {
    #[serde(default)]
    enabled: bool,
    #[serde(default)]
    method: String,
    #[serde(default)]
    host: String,
    #[serde(default)]
    label: String,
}

#[derive(Deserialize)]
struct Clients {
    #[serde(default)]
    nzb: Nzb,
    #[serde(default)]
    torrents: Torrents,
}

impl Clients {
    fn usenet(&self) -> Option<&NzbClient> {
        if !self.nzb.enabled {
            return None;
        }
        match self.nzb.method.as_str() {
            SABNZBD => Some(&self.nzb.sabnzbd),
            NZBGET => Some(&self.nzb.nzbget),
            _ => None,
        }
    }

    fn torrent(&self) -> Option<&Torrents> {
        Some(&self.torrents).filter(|torrents| {
            torrents.enabled && torrents.method != BLACKHOLE && !torrents.host.is_empty()
        })
    }
}

fn reached(address: &str, nzbget: bool) -> Option<(String, u16)> {
    let (secure, rest) = match address.split_once("://") {
        Some((scheme, rest)) => (scheme.eq_ignore_ascii_case("https"), rest),
        None => (false, address),
    };
    let authority = rest.split('/').next()?;
    let (host, port) = match authority.rsplit_once(':') {
        Some((host, port)) => (host, port.parse().ok()?),
        None if nzbget => return None,
        None => (authority, if secure { 443 } else { 80 }),
    };
    Some((host.to_owned(), port)).filter(|(host, _)| !host.is_empty())
}

fn same(value: &str, fields: &[&str]) -> Value {
    Value::Object(
        fields
            .iter()
            .map(|field| ((*field).to_owned(), json!(value)))
            .collect(),
    )
}

fn merged(mut into: Value, more: Value) -> Value {
    if let (Some(into), Value::Object(more)) = (into.as_object_mut(), more) {
        into.extend(more);
    }
    into
}

fn signed_with(client: &DownloadClient, with_key: bool) -> Option<Value> {
    match &client.credential {
        Credential::ApiKey(key) if with_key => {
            Some(json!({ "apiKey": key, "username": "", "password": "" }))
        }
        Credential::ApiKey(_) => None,
        Credential::UserPass { username, password } => {
            let mut signed = json!({ "username": username, "password": password });
            if with_key {
                signed = merged(signed, json!({ "apiKey": "" }));
            }
            Some(signed)
        }
    }
}

fn written(client: &DownloadClient) -> Result<(Slot, Value), Failure> {
    let Some((method, slot)) = SPOKEN
        .into_iter()
        .find(|(spoken, _)| *spoken == client.protocol.0)
    else {
        return Err(unsupported(&format!(
            "it files no download client spoken to as {}",
            client.protocol.0
        )));
    };
    let Some(signed) = signed_with(client, method == SABNZBD) else {
        return Err(unsupported(&format!(
            "it signs in to {} with a name and password, not a key",
            client.protocol.0
        )));
    };
    let category = &client.category.value;
    let url = format!("http://{}:{}/", client.host, client.port);
    let clients = match slot {
        Slot::Nzb => {
            let mut settings = if method == NZBGET {
                json!({ "host": format!("{}:{}", client.host, client.port), "useHttps": false })
            } else {
                json!({ "host": url })
            };
            settings = merged(merged(settings, signed), same(category, &NZB_CATEGORIES));
            json!({ "nzb": { "enabled": true, "method": method, (method): settings } })
        }
        Slot::Torrent => json!({
            "torrents": merged(
                merged(json!({ "enabled": true, "method": method, "host": url }), signed),
                same(category, &TORRENT_LABELS),
            )
        }),
    };
    Ok((
        slot,
        json!({
            "clients": clients,
            "postProcessing": { "downloadHandler": { "enabled": true, "frequency": HANDLER_MINUTES } },
        }),
    ))
}

fn tested(clients: &Value) -> Vec<(Slot, String)> {
    let text = |at: &str| {
        clients
            .pointer(at)
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned()
    };
    let on = |at: &str| clients.pointer(at).and_then(Value::as_bool) == Some(true);
    let mut asked = Vec::new();
    if on("/nzb/enabled") {
        match text("/nzb/method").as_str() {
            SABNZBD if !text("/nzb/sabnzbd/host").is_empty() => asked.push((
                Slot::Nzb,
                format!(
                    "/home/testSABnzbd?{}",
                    form_encoded(&[
                        ("host", &text("/nzb/sabnzbd/host")),
                        ("username", &text("/nzb/sabnzbd/username")),
                        ("password", &text("/nzb/sabnzbd/password")),
                        ("apikey", &text("/nzb/sabnzbd/apiKey")),
                    ])
                ),
            )),
            NZBGET if !text("/nzb/nzbget/host").is_empty() => asked.push((
                Slot::Nzb,
                format!(
                    "/home/testNZBget?{}",
                    form_encoded(&[
                        ("host", &text("/nzb/nzbget/host")),
                        ("username", &text("/nzb/nzbget/username")),
                        ("password", &text("/nzb/nzbget/password")),
                        (
                            "use_https",
                            if on("/nzb/nzbget/useHttps") { "1" } else { "0" }
                        ),
                    ])
                ),
            )),
            _ => {}
        }
    }
    let method = text("/torrents/method");
    if on("/torrents/enabled") && method != BLACKHOLE && !text("/torrents/host").is_empty() {
        asked.push((
            Slot::Torrent,
            format!(
                "/home/testTorrent?{}",
                form_encoded(&[
                    ("torrent_method", &method),
                    ("host", &text("/torrents/host")),
                    ("username", &text("/torrents/username")),
                    ("password", &text("/torrents/password")),
                ])
            ),
        ));
    }
    asked
}

fn probe(slot: Slot, said: &str) -> ClientProbe {
    let said = said.trim();
    let reachable = said.starts_with(SUCCEEDED);
    ClientProbe {
        id: slot.id().to_owned(),
        reachable,
        detail: (!reachable && !said.is_empty())
            .then(|| lemonfiber_error::withheld::withheld_text(said)),
    }
}

impl Medusa {
    async fn clients(&self) -> Result<Clients, Failure> {
        self.read(CLIENTS, "Medusa's download clients could not be read")
            .await
    }

    fn slot(&self, id: &str) -> Result<Slot, Failure> {
        Slot::named(id).ok_or_else(|| {
            self.endpoint
                .refused(&format!("Medusa holds no download client called {id}"))
        })
    }
}

#[async_trait]
impl Client for Medusa {
    async fn identity(&self) -> Result<Identity, Failure> {
        let version = self.release().await?;
        if version.is_empty() {
            return Err(self.endpoint.refused("Medusa named no release"));
        }
        Ok(Identity {
            name: NAME.to_owned(),
            version,
        })
    }

    async fn register_download_client(&self, client: &DownloadClient) -> Result<(), Failure> {
        let (_, patch) = written(client)?;
        self.configure(&patch).await
    }

    async fn download_clients(&self) -> Result<Vec<RegisteredClient>, Failure> {
        let clients = self.clients().await?;
        let usenet = clients.usenet().and_then(|usenet| {
            reached(&usenet.host, clients.nzb.method == NZBGET).map(|(host, port)| {
                RegisteredClient {
                    id: Slot::Nzb.id().to_owned(),
                    host,
                    port,
                    category: Some(Category {
                        field: Slot::Nzb.field().to_owned(),
                        value: usenet.category.clone(),
                    }),
                }
            })
        });
        let torrent = clients.torrent().and_then(|torrent| {
            reached(&torrent.host, false).map(|(host, port)| RegisteredClient {
                id: Slot::Torrent.id().to_owned(),
                host,
                port,
                category: Some(Category {
                    field: Slot::Torrent.field().to_owned(),
                    value: torrent.label.clone(),
                }),
            })
        });
        Ok(usenet.into_iter().chain(torrent).collect())
    }

    async fn update_download_client(
        &self,
        id: &str,
        client: &DownloadClient,
    ) -> Result<(), Failure> {
        let held = self.slot(id)?;
        let (slot, patch) = written(client)?;
        if slot != held {
            return Err(self.endpoint.refused(&format!(
                "Medusa holds its {} client as {}, not as {id}",
                client.protocol.0,
                slot.id()
            )));
        }
        self.configure(&patch).await
    }

    async fn set_client_field(
        &self,
        id: &str,
        field: &str,
        value: Option<&str>,
    ) -> Result<(), Failure> {
        let slot = self.slot(id)?;
        if !slot.takes(field) {
            return Err(unsupported(&format!(
                "it sets only the category of a download client, not its {field}"
            )));
        }
        let value = value.unwrap_or_default();
        let patch = match slot {
            Slot::Torrent => json!({ "clients": { "torrents": same(value, &TORRENT_LABELS) } }),
            Slot::Nzb => {
                let clients = self.clients().await?;
                if clients.usenet().is_none() {
                    return Err(self
                        .endpoint
                        .refused("Medusa holds no Usenet client to set the category of"));
                }
                json!({ "clients": { "nzb": { (clients.nzb.method): same(value, &NZB_CATEGORIES) } } })
            }
        };
        self.configure(&patch).await
    }

    async fn test_download_clients(&self) -> Result<Vec<ClientProbe>, Failure> {
        let clients: Value = self
            .read(CLIENTS, "Medusa's download clients could not be read")
            .await?;
        let asked = tested(&clients);
        if asked.is_empty() {
            return Ok(Vec::new());
        }
        let session = self.signed_in().await?;
        let mut probes = Vec::new();
        for (slot, path) in asked {
            let said = self.page(&session, &path).await?;
            probes.push(probe(slot, &said));
        }
        Ok(probes)
    }

    async fn register_root_folder(&self, folder: &RootFolder) -> Result<(), Failure> {
        if folder.media_type != lemonfiber_ports::media::Kind::Tv.media_type() {
            return Err(unsupported(&format!("it files no {}", folder.media_type)));
        }
        let held = self.main().await?;
        if held.folders().contains(&folder.path) {
            return Ok(());
        }
        let mut root_dirs = held.root_dirs;
        if root_dirs.is_empty() {
            root_dirs.push("0".to_owned());
        }
        root_dirs.push(folder.path.clone());
        self.configure(&json!({ "rootDirs": root_dirs })).await
    }

    async fn root_folders(&self) -> Result<Vec<RegisteredFolder>, Failure> {
        Ok(self
            .main()
            .await?
            .folders()
            .into_iter()
            .map(|path| RegisteredFolder {
                id: path.clone(),
                path,
            })
            .collect())
    }

    async fn quality_profiles(&self) -> Result<Vec<QualityProfile>, Failure> {
        Ok(self
            .presets()
            .await?
            .into_iter()
            .filter_map(|preset| {
                u32::try_from(preset.value).ok().map(|id| QualityProfile {
                    id,
                    name: preset.name,
                })
            })
            .collect())
    }
}

#[cfg(test)]
mod tests;
