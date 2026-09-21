//! Reading `Interface/AddOns` to see what the client currently loads.
//!
//! This is the ground truth the UI shows: an addon dropped in by hand appears
//! here exactly like one this application installed, just without provenance.

use std::path::Path;

use crate::domain::{AddonFolder, AddonVersion};
use crate::error::{AppError, Result};
use crate::wow::toc::{InterfaceVersion, Toc};

/// One directory under `Interface/AddOns`, described by its `.toc`.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct InstalledFolder {
    pub folder: AddonFolder,
    pub title: Option<String>,
    pub version: Option<AddonVersion>,
    pub author: Option<String>,
    pub notes: Option<String>,
    pub interface: Option<InterfaceVersion>,
    /// False when the `.toc` declares only interface numbers outside
    /// Forever's band — the addon is present but the client will refuse to
    /// load it unless the user allows out-of-date addons.
    pub loads_on_client: bool,
}

/// Every addon folder in the directory, sorted by folder name.
///
/// A missing directory is not an error: a fresh client simply has no addons.
pub fn scan_addons_dir(addons_dir: &Path) -> Result<Vec<InstalledFolder>> {
    if !addons_dir.is_dir() {
        return Ok(Vec::new());
    }

    let entries = std::fs::read_dir(addons_dir)
        .map_err(|err| AppError::io("read the AddOns directory", addons_dir, &err))?;

    let mut installed = entries
        .flatten()
        .filter(|entry| entry.path().is_dir())
        .filter_map(|entry| read_folder(&entry.path()))
        .collect::<Vec<_>>();

    installed.sort_by_key(|entry| entry.folder.match_key());
    Ok(installed)
}

/// `None` for a directory that carries no usable folder name — a `.toc` is not
/// required, because a folder can hold only Lua that another addon loads.
fn read_folder(path: &Path) -> Option<InstalledFolder> {
    let name = path.file_name()?.to_str()?;
    let folder = AddonFolder::new(name).ok()?;
    let toc = read_toc(path, &folder).unwrap_or_default();
    let interfaces = toc.interfaces();

    Some(InstalledFolder {
        title: toc.title(),
        version: toc.version().cloned(),
        author: toc.author().map(str::to_owned),
        notes: toc.notes().map(str::to_owned),
        interface: interfaces.first().copied(),
        loads_on_client: interfaces.is_empty()
            || interfaces.iter().any(|version| version.loads_on_forever()),
        folder,
    })
}

/// WoW loads `<Folder>/<Folder>.toc`; some addons ship a differently-cased name,
/// so the directory is swept for any `.toc` before giving up.
fn read_toc(path: &Path, folder: &AddonFolder) -> Option<Toc> {
    let preferred = path.join(format!("{folder}.toc"));
    if let Ok(contents) = std::fs::read_to_string(&preferred) {
        return Some(Toc::parse(&contents));
    }

    let entry = std::fs::read_dir(path).ok()?.flatten().find(|entry| {
        entry
            .path()
            .extension()
            .and_then(|ext| ext.to_str())
            .is_some_and(|ext| ext.eq_ignore_ascii_case("toc"))
    })?;

    let contents = std::fs::read_to_string(entry.path()).ok()?;
    Some(Toc::parse(&contents))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write_addon(addons_dir: &Path, folder: &str, toc_name: &str, toc: &str) {
        let dir = addons_dir.join(folder);
        std::fs::create_dir_all(&dir).expect("fixture dir");
        std::fs::write(dir.join(toc_name), toc).expect("fixture toc");
    }

    #[test]
    fn returns_nothing_for_a_client_without_an_addons_directory() {
        let temp = tempfile::tempdir().expect("temp dir");
        let scanned = scan_addons_dir(&temp.path().join("Interface/AddOns")).expect("no error");
        assert!(scanned.is_empty());
    }

    #[test]
    fn describes_each_folder_from_its_toc() {
        let temp = tempfile::tempdir().expect("temp dir");
        write_addon(
            temp.path(),
            "Bagnon",
            "Bagnon.toc",
            "## Interface: 16001\n## Title: Bagnon\n## Version: 2.13.3\n",
        );

        let scanned = scan_addons_dir(temp.path()).expect("no error");

        assert_eq!(scanned.len(), 1);
        assert_eq!(scanned[0].folder.as_str(), "Bagnon");
        assert_eq!(scanned[0].title.as_deref(), Some("Bagnon"));
        assert!(scanned[0].loads_on_client);
    }

    #[test]
    fn finds_a_toc_whose_name_does_not_match_its_folder() {
        let temp = tempfile::tempdir().expect("temp dir");
        write_addon(temp.path(), "Questie", "questie.TOC", "## Title: Questie\n");

        let scanned = scan_addons_dir(temp.path()).expect("no error");
        assert_eq!(scanned[0].title.as_deref(), Some("Questie"));
    }

    #[test]
    fn lists_a_folder_that_has_no_toc_at_all() {
        let temp = tempfile::tempdir().expect("temp dir");
        std::fs::create_dir_all(temp.path().join("SharedMedia")).expect("fixture dir");

        let scanned = scan_addons_dir(temp.path()).expect("no error");

        assert_eq!(scanned.len(), 1);
        assert_eq!(scanned[0].folder.as_str(), "SharedMedia");
        assert_eq!(scanned[0].title, None);
    }

    #[test]
    fn flags_an_addon_built_for_a_different_client() {
        let temp = tempfile::tempdir().expect("temp dir");
        write_addon(
            temp.path(),
            "Retail",
            "Retail.toc",
            "## Interface: 110000\n",
        );

        let scanned = scan_addons_dir(temp.path()).expect("no error");
        assert!(!scanned[0].loads_on_client);
    }

    #[test]
    fn accepts_a_comma_delimited_list_that_includes_forever() {
        let temp = tempfile::tempdir().expect("temp dir");
        write_addon(
            temp.path(),
            "Questie",
            "Questie.toc",
            "## Interface: 110200, 16001\n## Title: Questie\n",
        );

        let scanned = scan_addons_dir(temp.path()).expect("no error");
        assert!(scanned[0].loads_on_client, "16001 appears in the list");
    }

    #[test]
    fn sorts_folders_case_insensitively() {
        let temp = tempfile::tempdir().expect("temp dir");
        for folder in ["zAddon", "Ace3", "bagnon"] {
            std::fs::create_dir_all(temp.path().join(folder)).expect("fixture dir");
        }

        let scanned = scan_addons_dir(temp.path()).expect("no error");
        let names = scanned
            .iter()
            .map(|entry| entry.folder.as_str())
            .collect::<Vec<_>>();

        assert_eq!(names, vec!["Ace3", "bagnon", "zAddon"]);
    }
}
