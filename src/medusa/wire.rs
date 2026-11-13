use serde::Deserialize;
use serde_json::Value;

pub(crate) const PREFERRED_SHIFT: u32 = 16;

pub(crate) const ALLOWED_MASK: i64 = 0xFFFF;

pub(crate) const UNASKED: [&str; 2] = ["Skipped", "Ignored"];

pub(crate) const WANTED: i64 = 3;

#[derive(Deserialize, Default)]
pub(crate) struct Ids {
    #[serde(default)]
    pub(crate) tvdb: Option<i64>,
    #[serde(default)]
    pub(crate) slug: String,
}

#[derive(Deserialize, Default)]
pub(crate) struct Externals {
    #[serde(default)]
    pub(crate) tvdb: Option<i64>,
}

#[derive(Deserialize, Default)]
pub(crate) struct Started {
    #[serde(default)]
    pub(crate) start: Option<u16>,
}

#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Qualities {
    #[serde(default)]
    pub(crate) allowed: Vec<i64>,
    #[serde(default)]
    pub(crate) preferred: Vec<i64>,
}

impl Qualities {
    pub(crate) fn combined(&self) -> i64 {
        let allowed = self.allowed.iter().fold(0, |all, one| all | one);
        let preferred = self.preferred.iter().fold(0, |all, one| all | one);
        allowed | (preferred << PREFERRED_SHIFT)
    }

    pub(crate) fn of(combined: i64) -> Self {
        Self {
            allowed: bits(combined & ALLOWED_MASK),
            preferred: bits((combined >> PREFERRED_SHIFT) & ALLOWED_MASK),
        }
    }

    pub(crate) fn wanted(&self) -> i64 {
        self.allowed
            .iter()
            .chain(&self.preferred)
            .fold(0, |all, one| all | one)
    }
}

fn bits(of: i64) -> Vec<i64> {
    (0..PREFERRED_SHIFT)
        .map(|bit| 1_i64 << bit)
        .filter(|bit| of & bit != 0)
        .collect()
}

#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub(crate) struct SeriesConfig {
    #[serde(default)]
    pub(crate) paused: bool,
    #[serde(default)]
    pub(crate) root_dir: String,
    #[serde(default)]
    pub(crate) qualities: Qualities,
    #[serde(default)]
    pub(crate) default_episode_status: String,
}

#[derive(Deserialize)]
pub(crate) struct Series {
    #[serde(default)]
    pub(crate) id: Ids,
    #[serde(default)]
    pub(crate) externals: Externals,
    #[serde(default)]
    pub(crate) title: String,
    #[serde(default)]
    pub(crate) year: Started,
    #[serde(default)]
    pub(crate) config: SeriesConfig,
}

impl Series {
    pub(crate) fn tvdb(&self) -> Option<i64> {
        self.id
            .tvdb
            .or(self.externals.tvdb)
            .filter(|tvdb| *tvdb > 0)
    }
}

#[derive(Deserialize, Default)]
pub(crate) struct Location {
    #[serde(default)]
    pub(crate) location: String,
}

#[derive(Deserialize)]
pub(crate) struct Part {
    #[serde(default)]
    pub(crate) id: Ids,
    #[serde(default)]
    pub(crate) slug: String,
    #[serde(default)]
    pub(crate) season: u32,
    #[serde(default, rename = "episode")]
    pub(crate) number: u32,
    #[serde(default)]
    pub(crate) title: String,
    #[serde(default)]
    pub(crate) status: String,
    #[serde(default)]
    pub(crate) file: Location,
}

#[derive(Deserialize, Default)]
pub(crate) struct ClientStatus {
    #[serde(default)]
    pub(crate) status: Vec<i64>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct HistoryRow {
    #[serde(default)]
    pub(crate) id: u64,
    #[serde(default)]
    pub(crate) status: i64,
    #[serde(default)]
    pub(crate) action_date: Value,
    #[serde(default)]
    pub(crate) resource: String,
    #[serde(default)]
    pub(crate) size: u64,
    #[serde(default)]
    pub(crate) season: u32,
    #[serde(default)]
    pub(crate) episode: u32,
    #[serde(default)]
    pub(crate) info_hash: Option<String>,
    #[serde(default)]
    pub(crate) show_slug: Option<String>,
    #[serde(default)]
    pub(crate) show_title: String,
    #[serde(default)]
    pub(crate) episode_title: String,
    #[serde(default)]
    pub(crate) client_status: Option<ClientStatus>,
}

#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ShowDefaults {
    #[serde(default)]
    pub(crate) quality: i64,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Main {
    #[serde(default)]
    pub(crate) root_dirs: Vec<String>,
    #[serde(default)]
    pub(crate) show_defaults: ShowDefaults,
}

impl Main {
    pub(crate) fn folders(&self) -> Vec<String> {
        self.root_dirs.iter().skip(1).cloned().collect()
    }

    pub(crate) fn default_folder(&self) -> Option<String> {
        let chosen = self
            .root_dirs
            .first()
            .and_then(|index| index.parse::<usize>().ok())
            .unwrap_or_default();
        let folders = self.folders();
        folders.get(chosen).or_else(|| folders.first()).cloned()
    }
}

#[derive(Deserialize)]
pub(crate) struct Preset {
    pub(crate) value: i64,
    #[serde(default)]
    pub(crate) name: String,
}

#[derive(Deserialize, Default)]
pub(crate) struct QualityConsts {
    #[serde(default)]
    pub(crate) presets: Vec<Preset>,
}

#[derive(Deserialize)]
pub(crate) struct Consts {
    #[serde(default)]
    pub(crate) qualities: QualityConsts,
}
