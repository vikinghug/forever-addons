//! Persisted user settings.

use std::path::{Path, PathBuf};

use crate::domain::SourceId;
use crate::error::{AppError, Result};

/// What the application remembers between launches.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Settings {
    /// The client's product directory, as the user chose it. Stored
    /// unvalidated: a drive that is not mounted yet should not erase the
    /// setting.
    #[serde(default)]
    pub client_path: Option<PathBuf>,
    /// Sources the user wants in the catalog, in display order.
    #[serde(default = "default_sources")]
    pub enabled_sources: Vec<SourceId>,
    /// The user's key for the official CurseForge API; the CurseForge source
    /// cannot be queried without one.
    #[serde(default)]
    pub curseforge_api_key: Option<String>,
    /// The user's Wago Addons access token (self-serve, from account
    /// settings); the Wago source cannot be queried without one.
    #[serde(default)]
    pub wago_token: Option<String>,
    /// GitHub repositories tracked by hand as `owner/name`, in the order the
    /// user added them.
    #[serde(default)]
    pub github_repos: Vec<String>,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            client_path: None,
            enabled_sources: default_sources(),
            curseforge_api_key: None,
            wago_token: None,
            github_repos: Vec::new(),
        }
    }
}

fn default_sources() -> Vec<SourceId> {
    SourceId::ALL.to_vec()
}

impl Settings {
    /// A missing or unreadable settings file yields defaults rather than
    /// stopping the application from starting.
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
            action: "serialize the settings",
            path: path.to_owned(),
            reason: err.to_string(),
        })?;

        std::fs::write(path, body)
            .map_err(|err| AppError::io("write the settings file", path, &err))
    }

    pub fn is_enabled(&self, source: SourceId) -> bool {
        self.enabled_sources.contains(&source)
    }
}

pub fn settings_path(data_dir: &Path) -> PathBuf {
    data_dir.join("settings.json")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn starts_with_every_source_enabled() {
        let settings = Settings::default();

        assert!(settings.client_path.is_none());
        for source in SourceId::ALL {
            assert!(settings.is_enabled(source), "{source} should be enabled");
        }
    }

    #[test]
    fn round_trips_through_a_file() {
        let temp = tempfile::tempdir().expect("temp dir");
        let path = settings_path(temp.path());

        let settings = Settings {
            client_path: Some(PathBuf::from(
                "/games/bnet/World of Warcraft/_classic_beta_",
            )),
            enabled_sources: vec![SourceId::Wago, SourceId::CurseForge],
            curseforge_api_key: Some("$2a$10$example".to_owned()),
            wago_token: Some("wago-token".to_owned()),
            github_repos: vec!["RevoltLive85/ForeverGuide".to_owned()],
        };
        settings.save(&path).expect("settings are written");

        assert_eq!(Settings::load(&path), settings);
    }

    #[test]
    fn falls_back_to_defaults_for_an_unreadable_settings_file() {
        let temp = tempfile::tempdir().expect("temp dir");
        let path = settings_path(temp.path());
        std::fs::write(&path, b"not json at all").expect("fixture write");

        assert_eq!(Settings::load(&path), Settings::default());
    }

    #[test]
    fn keeps_a_configured_path_that_is_not_currently_mounted() {
        let temp = tempfile::tempdir().expect("temp dir");
        let path = settings_path(temp.path());
        std::fs::write(&path, br#"{"client_path":"/mnt/usb/wow"}"#).expect("fixture write");

        let loaded = Settings::load(&path);
        assert_eq!(loaded.client_path, Some(PathBuf::from("/mnt/usb/wow")));
        assert_eq!(loaded.enabled_sources, default_sources());
    }
}
