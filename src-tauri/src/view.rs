//! Projections the UI renders: what is installed, where it came from, and
//! whether the source has something newer.

use crate::catalog::Catalog;
use crate::domain::AddonVersion;
use crate::install::manifest::{InstalledRecord, Manifest};
use crate::wow::InstalledFolder;

/// Whether a source offers a newer build than the one on disk.
///
/// Versions are the clearest answer but not always available. Every source
/// does publish when it last changed an addon, so a source change after the
/// install date is the fallback signal.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(tag = "status", rename_all = "kebab-case")]
pub enum UpdateStatus {
    UpToDate,
    Available {
        latest: AddonVersion,
    },
    /// No versions to compare, but the source published a change after this
    /// copy was installed.
    SourceChanged {
        updated_at: String,
    },
    /// Neither signal is available: the catalog has not been refreshed, or the
    /// source publishes neither a version nor a modification date. Saying so is
    /// more useful than claiming an addon is current.
    Unknown,
}

/// One addon this application installed.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct ManagedAddon {
    #[serde(flatten)]
    pub record: InstalledRecord,
    /// The recorded folders as they are on disk right now.
    pub folders: Vec<InstalledFolder>,
    /// False once a user has deleted the folders by hand.
    pub present: bool,
    pub update: UpdateStatus,
}

/// The installed tab: managed addons, plus whatever else is in the directory.
#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize)]
pub struct InstalledView {
    pub managed: Vec<ManagedAddon>,
    /// Folders no record claims — addons the user unpacked themselves. They are
    /// listed so the view matches the directory, and never touched.
    pub unmanaged: Vec<InstalledFolder>,
}

/// Joins the AddOns directory, the manifest, and the catalogs into one view.
pub fn build<'a>(
    scanned: Vec<InstalledFolder>,
    manifest: &Manifest,
    catalogs: impl IntoIterator<Item = &'a Catalog> + Clone,
) -> InstalledView {
    let managed = manifest
        .records()
        .map(|record| managed_addon(record, &scanned, catalogs.clone()))
        .collect::<Vec<_>>();

    let unmanaged = scanned
        .into_iter()
        .filter(|found| manifest.owner_of(&found.folder).is_none())
        .collect();

    InstalledView { managed, unmanaged }
}

fn managed_addon<'a>(
    record: &InstalledRecord,
    scanned: &[InstalledFolder],
    catalogs: impl IntoIterator<Item = &'a Catalog>,
) -> ManagedAddon {
    let folders = scanned
        .iter()
        .filter(|found| {
            record
                .folders
                .iter()
                .any(|owned| owned.match_key() == found.folder.match_key())
        })
        .cloned()
        .collect::<Vec<_>>();

    let published = catalogs
        .into_iter()
        .find_map(|catalog| catalog.find(&record.id));

    ManagedAddon {
        update: update_status(
            record,
            published.and_then(|addon| addon.version.as_ref()),
            published.and_then(|addon| addon.updated_at.as_deref()),
        ),
        present: !folders.is_empty(),
        folders,
        record: record.clone(),
    }
}

fn update_status(
    record: &InstalledRecord,
    latest: Option<&AddonVersion>,
    source_updated: Option<&str>,
) -> UpdateStatus {
    if let (Some(installed), Some(latest)) = (record.version.as_ref(), latest) {
        return match installed.matches(latest) {
            true => UpdateStatus::UpToDate,
            false => UpdateStatus::Available {
                latest: latest.clone(),
            },
        };
    }

    let Some(source_updated) = source_updated else {
        return UpdateStatus::Unknown;
    };

    // Both timestamps are RFC 3339 in UTC, so they order lexicographically.
    if source_updated > record.installed_at.as_str() {
        return UpdateStatus::SourceChanged {
            updated_at: source_updated.to_owned(),
        };
    }

    UpdateStatus::UpToDate
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{AddonFolder, AddonId, AddonKey, AddonSummary, Download, SourceId};

    fn addon_id(key: &str) -> AddonId {
        AddonId::new(SourceId::CurseForge, AddonKey::new(key).expect("non-empty"))
    }

    fn record(key: &str, version: Option<&str>, folders: &[&str]) -> InstalledRecord {
        InstalledRecord {
            id: addon_id(key),
            name: "Bagnon".to_owned(),
            version: version.and_then(AddonVersion::new),
            folders: folders
                .iter()
                .map(|name| AddonFolder::new(*name).expect("valid folder"))
                .collect(),
            page_url: String::new(),
            installed_at: "2026-09-04T10:00:00Z".to_owned(),
        }
    }

    fn scanned(names: &[&str]) -> Vec<InstalledFolder> {
        names
            .iter()
            .map(|name| InstalledFolder {
                folder: AddonFolder::new(*name).expect("valid folder"),
                title: None,
                version: None,
                author: None,
                notes: None,
                interface: None,
                loads_on_client: true,
            })
            .collect()
    }

    fn catalog(key: &str, version: Option<&str>) -> Catalog {
        catalog_with(key, version, None)
    }

    fn catalog_with(key: &str, version: Option<&str>, updated_at: Option<&str>) -> Catalog {
        Catalog::new(
            SourceId::CurseForge,
            vec![AddonSummary {
                id: addon_id(key),
                name: "Bagnon".to_owned(),
                summary: String::new(),
                author: None,
                version: version.and_then(AddonVersion::new),
                updated_at: updated_at.map(str::to_owned),
                icon_url: None,
                page_url: String::new(),
                categories: Vec::new(),
                downloads: None,
                expansions: Vec::new(),
                download: Download::Brokered,
            }],
        )
    }

    #[test]
    fn separates_managed_addons_from_folders_the_user_added() {
        let mut manifest = Manifest::default();
        manifest.insert(record("1", Some("2.13.3"), &["Bagnon"]));

        let view = build(scanned(&["Bagnon", "MyOwnAddon"]), &manifest, &[]);

        assert_eq!(view.managed.len(), 1);
        assert_eq!(view.managed[0].record.id, addon_id("1"));
        assert_eq!(view.unmanaged.len(), 1);
        assert_eq!(view.unmanaged[0].folder.as_str(), "MyOwnAddon");
    }

    #[test]
    fn reports_an_update_when_the_catalog_moved_ahead() {
        let mut manifest = Manifest::default();
        manifest.insert(record("1", Some("2.13.3"), &["Bagnon"]));

        let catalogs = [catalog("1", Some("2.14.0"))];
        let view = build(scanned(&["Bagnon"]), &manifest, catalogs.iter());

        assert_eq!(
            view.managed[0].update,
            UpdateStatus::Available {
                latest: AddonVersion::new("2.14.0").expect("non-empty")
            }
        );
    }

    #[test]
    fn reports_up_to_date_across_a_v_prefix() {
        let mut manifest = Manifest::default();
        manifest.insert(record("1", Some("v2.13.3"), &["Bagnon"]));

        let catalogs = [catalog("1", Some("2.13.3"))];
        let view = build(scanned(&["Bagnon"]), &manifest, catalogs.iter());

        assert_eq!(view.managed[0].update, UpdateStatus::UpToDate);
    }

    #[test]
    fn reports_unknown_when_a_source_publishes_neither_signal() {
        let mut manifest = Manifest::default();
        manifest.insert(record("1", None, &["Bagnon"]));

        let catalogs = [catalog("1", Some("2.14.0"))];
        let view = build(scanned(&["Bagnon"]), &manifest, catalogs.iter());

        assert_eq!(view.managed[0].update, UpdateStatus::Unknown);
    }

    #[test]
    fn falls_back_to_the_source_changing_after_the_install() {
        let mut manifest = Manifest::default();
        manifest.insert(record("1", None, &["Bagnon"]));

        let catalogs = [catalog_with("1", None, Some("2026-09-12T00:00:00Z"))];
        let view = build(scanned(&["Bagnon"]), &manifest, catalogs.iter());

        assert_eq!(
            view.managed[0].update,
            UpdateStatus::SourceChanged {
                updated_at: "2026-09-12T00:00:00Z".to_owned()
            }
        );
    }

    #[test]
    fn treats_a_source_untouched_since_the_install_as_current() {
        let mut manifest = Manifest::default();
        manifest.insert(record("1", None, &["Bagnon"]));

        let catalogs = [catalog_with("1", None, Some("2026-08-01T00:00:00Z"))];
        let view = build(scanned(&["Bagnon"]), &manifest, catalogs.iter());

        assert_eq!(view.managed[0].update, UpdateStatus::UpToDate);
    }

    #[test]
    fn prefers_a_version_comparison_over_the_modification_date() {
        let mut manifest = Manifest::default();
        manifest.insert(record("1", Some("2.13.3"), &["Bagnon"]));

        let catalogs = [catalog_with(
            "1",
            Some("2.13.3"),
            Some("2026-09-01T00:00:00Z"),
        )];
        let view = build(scanned(&["Bagnon"]), &manifest, catalogs.iter());

        assert_eq!(view.managed[0].update, UpdateStatus::UpToDate);
    }

    #[test]
    fn flags_a_managed_addon_whose_folders_were_deleted_by_hand() {
        let mut manifest = Manifest::default();
        manifest.insert(record("1", Some("2.13.3"), &["Bagnon"]));

        let view = build(Vec::new(), &manifest, &[]);

        assert!(!view.managed[0].present);
        assert!(view.managed[0].folders.is_empty());
    }

    #[test]
    fn matches_recorded_folders_case_insensitively() {
        let mut manifest = Manifest::default();
        manifest.insert(record("1", None, &["Bagnon"]));

        let view = build(scanned(&["bagnon"]), &manifest, &[]);

        assert!(view.managed[0].present);
        assert!(view.unmanaged.is_empty());
    }
}
