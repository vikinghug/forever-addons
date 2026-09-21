//! The local World of Warcraft: Forever client: locating it, validating it,
//! and reading what is currently installed under `Interface/AddOns`.
//!
//! The configured root is a Battle.net *product* directory (currently
//! `_classic_beta_` while Forever is in beta): the executable and a
//! `.flavor.info` marker live in it, while the shared `Data` directory sits in
//! the parent. Addons live under `Interface/AddOns` inside the product
//! directory.

pub mod scan;
pub mod toc;

use std::path::{Path, PathBuf};

use crate::error::{AppError, Result};

pub use scan::{InstalledFolder, scan_addons_dir};
pub use toc::{InterfaceVersion, Toc, TocDirective};

/// A directory that has been checked to look like a Forever client.
///
/// Everything that writes to disk takes one of these, so "the user pointed us
/// at their Downloads folder and we deleted it" is not a reachable state.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(transparent)]
pub struct WowInstall {
    root: PathBuf,
}

impl WowInstall {
    /// Accepts a Battle.net product directory: a `Wow*.exe` (the beta ships
    /// `WowB.exe`) plus the `.flavor.info` marker Battle.net writes into every
    /// product it installs. `Data` lives in the shared parent, so it is not
    /// required here.
    pub fn open(root: impl Into<PathBuf>) -> Result<Self> {
        let root = root.into();

        if !root.is_dir() {
            return Err(invalid(&root, "the path is not a directory"));
        }
        if !contains_wow_executable(&root) {
            return Err(invalid(&root, "no WoW executable (Wow*.exe) in it"));
        }
        if !root.join(".flavor.info").is_file() {
            return Err(invalid(
                &root,
                "no .flavor.info marker, so this is not a Battle.net product directory \
                 (expected something like World of Warcraft/_classic_beta_)",
            ));
        }

        Ok(Self { root })
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn addons_dir(&self) -> PathBuf {
        self.root.join("Interface").join("AddOns")
    }

    /// A fresh client ships without `Interface/AddOns`; the first install
    /// creates it rather than failing.
    pub fn ensure_addons_dir(&self) -> Result<PathBuf> {
        let dir = self.addons_dir();
        std::fs::create_dir_all(&dir)
            .map_err(|err| AppError::io("create the AddOns directory", &dir, &err))?;
        Ok(dir)
    }
}

fn contains_wow_executable(root: &Path) -> bool {
    let Ok(entries) = std::fs::read_dir(root) else {
        return false;
    };

    entries.flatten().any(|entry| {
        entry
            .file_name()
            .to_str()
            .is_some_and(is_wow_executable_name)
    })
}

fn is_wow_executable_name(name: &str) -> bool {
    let lowered = name.to_ascii_lowercase();
    lowered.starts_with("wow") && lowered.ends_with(".exe") && !lowered.contains("error")
}

fn invalid(root: &Path, reason: &str) -> AppError {
    AppError::InvalidInstall {
        path: root.to_owned(),
        reason: reason.to_owned(),
    }
}

/// The places the Forever client is commonly installed, checked in order so
/// the first launch can offer a guess instead of an empty file picker.
///
/// The list names `_classic_beta_`, the product directory the beta installs
/// under; the release build may add a differently named sibling, in which case
/// this list grows rather than changes.
pub fn likely_install_roots() -> Vec<PathBuf> {
    let mut roots = Vec::new();

    if let Some(home) = dirs::home_dir() {
        for relative in [
            "Games/Battle.net/World of Warcraft/_classic_beta_",
            "Games/World of Warcraft/_classic_beta_",
            ".wine/drive_c/Program Files (x86)/World of Warcraft/_classic_beta_",
        ] {
            roots.push(home.join(relative));
        }
    }

    for absolute in [
        "C:\\Program Files (x86)\\World of Warcraft\\_classic_beta_",
        "C:\\Program Files\\World of Warcraft\\_classic_beta_",
    ] {
        roots.push(PathBuf::from(absolute));
    }

    roots
}

/// The first likely root that actually validates, if any.
pub fn detect_install() -> Option<WowInstall> {
    likely_install_roots()
        .into_iter()
        .find_map(|root| WowInstall::open(root).ok())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn client_at(root: &Path) {
        std::fs::create_dir_all(root).expect("fixture dirs");
        std::fs::write(root.join("WowB.exe"), b"MZ").expect("fixture executable");
        std::fs::write(root.join(".flavor.info"), "wow_classic_beta\n").expect("fixture marker");
    }

    #[test]
    fn opens_a_product_directory_holding_a_client() {
        let temp = tempfile::tempdir().expect("temp dir");
        client_at(temp.path());

        let install = WowInstall::open(temp.path()).expect("fixture is a valid client");
        assert_eq!(
            install.addons_dir(),
            temp.path().join("Interface").join("AddOns")
        );
    }

    #[test]
    fn accepts_any_wow_prefixed_executable_name() {
        let temp = tempfile::tempdir().expect("temp dir");
        std::fs::write(temp.path().join("wow.exe"), b"MZ").expect("fixture executable");
        std::fs::write(temp.path().join(".flavor.info"), "wow\n").expect("fixture marker");

        assert!(WowInstall::open(temp.path()).is_ok());
    }

    #[test]
    fn rejects_a_directory_without_a_client_executable() {
        let temp = tempfile::tempdir().expect("temp dir");
        std::fs::write(temp.path().join(".flavor.info"), "wow\n").expect("fixture marker");

        let error = WowInstall::open(temp.path()).expect_err("no executable");
        assert!(error.to_string().contains("Wow*.exe"), "{error}");
    }

    #[test]
    fn rejects_a_directory_without_the_flavor_marker() {
        let temp = tempfile::tempdir().expect("temp dir");
        std::fs::write(temp.path().join("WowB.exe"), b"MZ").expect("fixture executable");

        let error = WowInstall::open(temp.path()).expect_err("no .flavor.info");
        assert!(error.to_string().contains(".flavor.info"), "{error}");
    }

    #[test]
    fn does_not_mistake_blizzard_error_for_the_client() {
        let temp = tempfile::tempdir().expect("temp dir");
        std::fs::write(temp.path().join("BlizzardError.exe"), b"MZ").expect("fixture executable");
        std::fs::write(temp.path().join(".flavor.info"), "wow\n").expect("fixture marker");

        assert!(WowInstall::open(temp.path()).is_err());
    }

    #[test]
    fn creates_the_addons_directory_on_a_fresh_client() {
        let temp = tempfile::tempdir().expect("temp dir");
        client_at(temp.path());
        let install = WowInstall::open(temp.path()).expect("valid client");

        let dir = install.ensure_addons_dir().expect("directory is created");
        assert!(dir.is_dir());
    }
}
