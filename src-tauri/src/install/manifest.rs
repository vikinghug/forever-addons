//! The record of what this application installed.
//!
//! WoW itself keeps no provenance: a folder under `Interface/AddOns` says
//! nothing about where it came from. The manifest is what lets an upgrade
//! replace exactly the folders an addon owns, and an uninstall delete exactly
//! those and nothing a user dropped in by hand.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use crate::domain::{AddonFolder, AddonId, AddonVersion};
use crate::error::{AppError, Result};

/// One installed addon, as this application installed it.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct InstalledRecord {
    pub id: AddonId,
    pub name: String,
    pub version: Option<AddonVersion>,
    /// Every folder the archive placed under `Interface/AddOns`.
    pub folders: Vec<AddonFolder>,
    pub page_url: String,
    /// RFC 3339, in UTC.
    pub installed_at: String,
}

/// The manifest file's contents, keyed by rendered [`AddonId`].
///
/// A `BTreeMap` so the file has a stable order and diffs cleanly.
#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Manifest {
    #[serde(default)]
    addons: BTreeMap<String, InstalledRecord>,
}

impl Manifest {
    /// A missing or unreadable manifest is an empty one: a corrupted file must
    /// not make the application refuse to start.
    pub fn load(path: &Path) -> Self {
        std::fs::read_to_string(path)
            .ok()
            .and_then(|raw| serde_json::from_str(&raw).ok())
            .unwrap_or_default()
    }

    pub fn save(&self, path: &Path) -> Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|err| AppError::io("create the data directory", parent, &err))?;
        }

        let body = serde_json::to_string_pretty(self).map_err(|err| AppError::Persist {
            action: "serialize the installed-addon manifest",
            path: path.to_owned(),
            reason: err.to_string(),
        })?;

        write_atomically(path, body.as_bytes())
    }

    pub fn get(&self, id: &AddonId) -> Option<&InstalledRecord> {
        self.addons.get(&id.to_string())
    }

    pub fn insert(&mut self, record: InstalledRecord) {
        self.addons.insert(record.id.to_string(), record);
    }

    pub fn remove(&mut self, id: &AddonId) -> Option<InstalledRecord> {
        self.addons.remove(&id.to_string())
    }

    pub fn records(&self) -> impl Iterator<Item = &InstalledRecord> {
        self.addons.values()
    }

    /// Which addon owns a given folder, so the installed list can attribute a
    /// folder on disk to the catalog entry it came from.
    pub fn owner_of(&self, folder: &AddonFolder) -> Option<&InstalledRecord> {
        let key = folder.match_key();
        self.records()
            .find(|record| record.folders.iter().any(|owned| owned.match_key() == key))
    }
}

/// Writes through a temporary file in the same directory, so an interrupted
/// write cannot leave a half-written manifest behind.
fn write_atomically(path: &Path, body: &[u8]) -> Result<()> {
    let directory = path.parent().unwrap_or(Path::new("."));
    let temporary = path.with_extension("json.tmp");

    std::fs::write(&temporary, body)
        .map_err(|err| AppError::io("write the installed-addon manifest", &temporary, &err))?;
    std::fs::rename(&temporary, path).map_err(|err| {
        let _ = std::fs::remove_file(&temporary);
        AppError::io("replace the installed-addon manifest", path, &err)
    })?;

    let _ = directory;
    Ok(())
}

/// The manifest's location inside the application's data directory.
/// The current instant, in the one format this application persists times in.
///
/// A clock that cannot be formatted is not worth failing an install over, so
/// the epoch stands in — a visibly wrong timestamp rather than a lost record.
pub(crate) fn now_rfc3339() -> String {
    time::OffsetDateTime::now_utc()
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap_or_else(|_| String::from("1970-01-01T00:00:00Z"))
}

/// The manifest's location inside the application's data directory.
pub fn manifest_path(data_dir: &Path) -> PathBuf {
    data_dir.join("installed.json")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{AddonKey, SourceId};

    fn record(key: &str, folders: &[&str]) -> InstalledRecord {
        InstalledRecord {
            id: AddonId::new(SourceId::CurseForge, AddonKey::new(key).expect("non-empty")),
            name: "Bagnon".to_owned(),
            version: AddonVersion::new("2.13.3"),
            folders: folders
                .iter()
                .map(|name| AddonFolder::new(*name).expect("valid folder"))
                .collect(),
            page_url: "https://www.curseforge.com/wow/addons/bagnon".to_owned(),
            installed_at: "2026-09-04T10:00:00Z".to_owned(),
        }
    }

    #[test]
    fn round_trips_through_a_file() {
        let temp = tempfile::tempdir().expect("temp dir");
        let path = manifest_path(temp.path());

        let mut manifest = Manifest::default();
        manifest.insert(record("4857", &["Bagnon", "Bagnon_Config"]));
        manifest.save(&path).expect("manifest is written");

        let loaded = Manifest::load(&path);
        assert_eq!(loaded, manifest);
    }

    #[test]
    fn treats_a_missing_manifest_as_empty() {
        let temp = tempfile::tempdir().expect("temp dir");
        assert_eq!(
            Manifest::load(&manifest_path(temp.path()))
                .records()
                .count(),
            0
        );
    }

    #[test]
    fn treats_a_corrupted_manifest_as_empty_instead_of_failing_to_start() {
        let temp = tempfile::tempdir().expect("temp dir");
        let path = manifest_path(temp.path());
        std::fs::write(&path, b"{ this is not json").expect("fixture write");

        assert_eq!(Manifest::load(&path).records().count(), 0);
    }

    #[test]
    fn attributes_an_installed_folder_to_its_addon_case_insensitively() {
        let mut manifest = Manifest::default();
        manifest.insert(record("4857", &["Bagnon", "Bagnon_Config"]));

        let folder = AddonFolder::new("bagnon_config").expect("valid folder");
        let owner = manifest.owner_of(&folder).expect("folder is owned");

        assert_eq!(owner.id.to_string(), "curseforge:4857");
    }

    #[test]
    fn reports_no_owner_for_a_hand_installed_folder() {
        let manifest = Manifest::default();
        let folder = AddonFolder::new("SomeoneElsesAddon").expect("valid folder");

        assert!(manifest.owner_of(&folder).is_none());
    }

    #[test]
    fn replaces_a_record_when_an_addon_is_upgraded() {
        let mut manifest = Manifest::default();
        manifest.insert(record("4857", &["Bagnon"]));
        manifest.insert(record("4857", &["Bagnon", "Bagnon_Config"]));

        assert_eq!(manifest.records().count(), 1);
        assert_eq!(
            manifest
                .get(&AddonId::new(
                    SourceId::CurseForge,
                    AddonKey::new("4857").expect("non-empty")
                ))
                .map(|found| found.folders.len()),
            Some(2)
        );
    }
}
