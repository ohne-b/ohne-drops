use std::{
    collections::{BTreeMap, HashSet},
    fs::{self, File, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result, bail};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize, de::DeserializeOwned};

use crate::{
    config::{Settings, fold},
    dto::{CampaignView, HistoryEntry},
};

pub struct DataDirectory {
    pub path: PathBuf,
    _lock: File,
}

impl DataDirectory {
    pub fn open(path: impl Into<PathBuf>) -> Result<Self> {
        let path = path.into();
        fs::create_dir_all(&path).context("cannot create data directory")?;
        let lock = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(path.join(".miner.lock"))
            .context("cannot open data directory lock")?;
        lock.try_lock()
            .context("data directory is already in use or cannot be locked")?;
        Ok(Self { path, _lock: lock })
    }

    pub fn settings(&self) -> Result<Settings> {
        read_json(&self.path.join("settings.json"))?
            .map(Settings::from_saved)
            .transpose()
            .context("invalid settings file; original file preserved")
            .map(Option::unwrap_or_default)
    }

    pub fn save_settings(&self, settings: &Settings) -> Result<()> {
        atomic_json(&self.path.join("settings.json"), settings)
    }
}

pub fn read_json<T: DeserializeOwned>(path: &Path) -> Result<Option<T>> {
    match fs::read(path) {
        Ok(bytes) => serde_json::from_slice(&bytes)
            .map(Some)
            .context("invalid saved data"),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(_) => bail!("cannot read saved data"),
    }
}

pub fn atomic_json<T: Serialize>(path: &Path, value: &T) -> Result<()> {
    let parent = path
        .parent()
        .context("saved data requires a parent directory")?;
    let mut temporary =
        tempfile::NamedTempFile::new_in(parent).context("cannot prepare saved data")?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        temporary
            .as_file()
            .set_permissions(fs::Permissions::from_mode(0o600))
            .context("cannot protect saved data")?;
    }
    serde_json::to_writer_pretty(&mut temporary, value).context("cannot encode saved data")?;
    temporary
        .write_all(b"\n")
        .context("cannot write saved data")?;
    temporary
        .as_file()
        .sync_all()
        .context("cannot sync saved data")?;
    temporary
        .persist(path)
        .map_err(|_| anyhow::anyhow!("cannot replace saved data"))?;
    #[cfg(unix)]
    if File::open(parent)
        .and_then(|directory| directory.sync_all())
        .is_err()
    {
        // Replacement already committed. Keep memory consistent with disk even on
        // filesystems that cannot sync directories; report weaker crash durability.
        tracing::warn!("Saved data was replaced, but the directory could not be synced");
    }
    Ok(())
}

#[derive(Serialize, Deserialize)]
struct HistoryFile {
    version: u32,
    entries: Vec<HistoryEntry>,
}

pub struct History {
    path: PathBuf,
    entries: Vec<HistoryEntry>,
    pub writable: bool,
}

#[derive(Default)]
pub struct HistoryFilter {
    pub game: Option<String>,
    pub campaign_id: Option<String>,
    pub since: Option<DateTime<Utc>>,
    pub limit: Option<usize>,
}

impl History {
    pub fn load(directory: &Path) -> Self {
        let path = directory.join("drop_history.json");
        let loaded = read_json::<HistoryFile>(&path).and_then(|value| {
            let Some(value) = value else {
                return Ok(vec![]);
            };
            let mut seen = HashSet::new();
            if value.version != 1
                || value
                    .entries
                    .iter()
                    .any(|e| e.id.is_empty() || !seen.insert(&e.id))
            {
                bail!("invalid history");
            }
            Ok(value.entries)
        });
        match loaded {
            Ok(entries) => Self {
                path,
                entries,
                writable: true,
            },
            Err(_) => {
                tracing::error!("Cannot read claim history; preserving the original file");
                Self {
                    path,
                    entries: vec![],
                    writable: false,
                }
            }
        }
    }

    pub fn record(&mut self, entry: HistoryEntry) -> Result<bool> {
        if self.entries.iter().any(|e| e.id == entry.id) {
            return Ok(false);
        }
        let mut next = self.entries.clone();
        next.push(entry);
        self.replace(next)?;
        Ok(true)
    }

    fn replace(&mut self, entries: Vec<HistoryEntry>) -> Result<()> {
        if !self.writable {
            bail!("claim history is unreadable; original file preserved");
        }
        let next = HistoryFile {
            version: 1,
            entries,
        };
        atomic_json(&self.path, &next)?;
        self.entries = next.entries;
        Ok(())
    }

    pub fn clear(&mut self) -> Result<()> {
        self.replace(vec![])
    }
    pub fn total(&self) -> usize {
        self.entries.len()
    }

    pub fn entries(&self, filter: &HistoryFilter) -> Vec<HistoryEntry> {
        self.entries
            .iter()
            .rev()
            .filter(|e| {
                filter
                    .game
                    .as_ref()
                    .is_none_or(|game| fold(game) == fold(&e.game))
                    && filter
                        .campaign_id
                        .as_ref()
                        .is_none_or(|id| id == &e.campaign_id)
                    && filter.since.is_none_or(|since| since <= e.claimed_at)
            })
            .take(filter.limit.unwrap_or(usize::MAX))
            .cloned()
            .collect()
    }

    pub fn csv(&self, filter: &HistoryFilter) -> Result<Vec<u8>> {
        let mut writer = csv::WriterBuilder::new()
            .terminator(csv::Terminator::CRLF)
            .from_writer(vec![0xef, 0xbb, 0xbf]);
        writer.write_record([
            "claimed_at",
            "game",
            "campaign",
            "drop_name",
            "benefits",
            "required_minutes",
            "drop_id",
            "campaign_id",
        ])?;
        for entry in self.entries(filter) {
            writer.write_record([
                entry.claimed_at.to_rfc3339(),
                entry.game,
                entry.campaign,
                entry.drop_name,
                entry.benefits.join("; "),
                entry.required_minutes.to_string(),
                entry.id,
                entry.campaign_id,
            ])?;
        }
        Ok(writer.into_inner()?)
    }

    pub fn stats(&self) -> serde_json::Value {
        let mut games = BTreeMap::<String, usize>::new();
        let mut months = BTreeMap::<String, usize>::new();
        for entry in &self.entries {
            *games.entry(entry.game.clone()).or_default() += 1;
            *months
                .entry(entry.claimed_at.format("%Y-%m").to_string())
                .or_default() += 1;
        }
        serde_json::json!({"total": self.total(), "by_game": games, "by_month": months})
    }
}

#[derive(Serialize, Deserialize)]
struct ArchiveFile {
    version: u32,
    campaigns: Vec<CampaignView>,
}

pub struct CampaignArchive {
    path: PathBuf,
    campaigns: BTreeMap<String, CampaignView>,
    pub writable: bool,
}

impl CampaignArchive {
    pub fn load(directory: &Path) -> Self {
        let path = directory.join("completed_campaigns.json");
        let loaded = read_json::<ArchiveFile>(&path).and_then(|value| {
            let Some(value) = value else {
                return Ok(BTreeMap::new());
            };
            let mut campaigns = BTreeMap::new();
            if value.version != 1 {
                bail!("unknown campaign archive version");
            }
            for campaign in value.campaigns {
                if !Self::valid(&campaign)
                    || campaigns.insert(campaign.id.clone(), campaign).is_some()
                {
                    bail!("invalid campaign archive");
                }
            }
            Ok(campaigns)
        });
        match loaded {
            Ok(campaigns) => Self {
                path,
                campaigns,
                writable: true,
            },
            Err(_) => {
                tracing::error!("Cannot read completed campaigns; preserving the original file");
                Self {
                    path,
                    campaigns: BTreeMap::new(),
                    writable: false,
                }
            }
        }
    }

    fn valid(c: &CampaignView) -> bool {
        let mut seen = HashSet::new();
        c.finished
            && !c.id.is_empty()
            && !c.drops.is_empty()
            && c.total_drops == c.drops.len()
            && c.claimed_drops == c.drops.len()
            && c.drops.iter().all(|d| {
                d.is_claimed && d.required_minutes > 0 && !d.id.is_empty() && seen.insert(&d.id)
            })
    }

    pub fn update(&mut self, live: &[CampaignView]) -> Result<()> {
        let mut next = self.campaigns.clone();
        for campaign in live {
            if campaign.finished && Self::valid(campaign) {
                next.insert(campaign.id.clone(), campaign.clone());
            } else if let Some(previous) = next.get(&campaign.id) {
                let ids =
                    |c: &CampaignView| c.drops.iter().map(|d| d.id.clone()).collect::<HashSet<_>>();
                if campaign
                    .drops
                    .iter()
                    .any(|d| !d.is_claimed && d.confirmed_at.is_some())
                    || ids(campaign) != ids(previous)
                {
                    next.remove(&campaign.id);
                }
            }
        }
        if next != self.campaigns {
            if !self.writable {
                bail!("campaign archive is unreadable; original file preserved");
            }
            atomic_json(
                &self.path,
                &ArchiveFile {
                    version: 1,
                    campaigns: next.values().cloned().collect(),
                },
            )?;
            self.campaigns = next;
        }
        Ok(())
    }

    pub fn merge(&self, live: Vec<CampaignView>, now: DateTime<Utc>) -> Vec<CampaignView> {
        let mut combined: BTreeMap<_, _> = live.into_iter().map(|c| (c.id.clone(), c)).collect();
        for (id, archived) in &self.campaigns {
            let mut archived = archived.clone();
            archived.active = archived.starts_at <= now && now < archived.ends_at;
            archived.upcoming = now < archived.starts_at;
            archived.expired = archived.ends_at <= now;
            combined.insert(id.clone(), archived);
        }
        combined.into_values().collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn entry(id: &str) -> HistoryEntry {
        serde_json::from_value(
            json!({"id":id, "claimed_at":"2026-01-01T23:45:00-01:00", "game":"Rust",
            "campaign":"Winter", "drop_name":"Coat, warm", "benefits":["Coat", "Boots"],
            "required_minutes": 30, "campaign_id":"c"}),
        )
        .unwrap()
    }

    #[test]
    fn settings_and_exclusive_lock_round_trip() {
        let dir = tempfile::tempdir().unwrap();
        let data = DataDirectory::open(dir.path()).unwrap();
        assert!(DataDirectory::open(dir.path()).is_err());
        let settings = Settings::default()
            .patched(&json!({"games_to_watch":["Rust"],"connection_quality": 3}))
            .unwrap();
        data.save_settings(&settings).unwrap();
        assert_eq!(data.settings().unwrap().games_to_watch, ["Rust"]);
        drop(data);
        assert_eq!(
            DataDirectory::open(dir.path())
                .unwrap()
                .settings()
                .unwrap()
                .connection_quality,
            3
        );
    }

    #[test]
    fn history_preserves_old_artless_entries_and_exports_filtered_data() {
        let dir = tempfile::tempdir().unwrap();
        let mut history = History::load(dir.path());
        assert!(history.record(entry("a")).unwrap());
        assert!(!history.record(entry("a")).unwrap());
        history.record(entry("b")).unwrap();
        let restored = History::load(dir.path());
        let filter = HistoryFilter {
            game: Some("RUST".into()),
            since: Some("2026-01-02T00:00:00Z".parse().unwrap()),
            ..Default::default()
        };
        assert_eq!(
            restored
                .entries(&filter)
                .iter()
                .map(|e| e.id.as_str())
                .collect::<Vec<_>>(),
            ["b", "a"]
        );
        assert!(restored.entries(&filter)[0].image_url.is_empty());
        let csv = String::from_utf8(restored.csv(&filter).unwrap()).unwrap();
        assert!(csv.starts_with('\u{feff}'));
        assert!(csv.contains("\"Coat, warm\",Coat; Boots,30,b,c\r\n"));
        assert_eq!(restored.stats()["by_month"]["2026-01"], 2);
        history.clear().unwrap();
        assert_eq!(History::load(dir.path()).total(), 0);
    }

    #[test]
    fn unreadable_files_survive_mutations() {
        let dir = tempfile::tempdir().unwrap();
        for name in [
            "drop_history.json",
            "completed_campaigns.json",
            "settings.json",
        ] {
            fs::write(dir.path().join(name), b"{broken").unwrap();
        }
        let mut history = History::load(dir.path());
        assert!(!history.writable);
        assert!(history.record(entry("a")).is_err());
        assert!(history.clear().is_err());
        assert!(!CampaignArchive::load(dir.path()).writable);
        assert!(DataDirectory::open(dir.path()).unwrap().settings().is_err());
        for name in [
            "drop_history.json",
            "completed_campaigns.json",
            "settings.json",
        ] {
            assert_eq!(fs::read(dir.path().join(name)).unwrap(), b"{broken");
        }
    }

    #[test]
    fn failed_replace_does_not_change_in_memory_history() {
        let dir = tempfile::tempdir().unwrap();
        let mut history = History::load(dir.path());
        history.record(entry("a")).unwrap();
        history.path = dir.path().join("missing").join("history.json");
        assert!(history.record(entry("b")).is_err());
        assert!(history.clear().is_err());
        assert_eq!(history.total(), 1);
        assert_eq!(History::load(dir.path()).total(), 1);
    }
}
