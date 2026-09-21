use std::path::PathBuf;

use crate::domain::{AddonId, SourceId};

/// Every failure the application boundary can report to the UI.
///
/// Variants carry the identifying context a user or a bug report needs: which
/// source, which addon, which path. `Display` output is what reaches the
/// frontend, so it must read as a sentence.
#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("no World of Warcraft: Forever installation is configured yet")]
    NoInstallConfigured,

    #[error("{path} is not a World of Warcraft: Forever installation: {reason}")]
    InvalidInstall { path: PathBuf, reason: String },

    #[error("{source_id} request to {url} failed: {reason}")]
    SourceRequest {
        source_id: SourceId,
        url: String,
        reason: String,
    },

    #[error("{source_id} returned HTTP {status} for {url}")]
    SourceStatus {
        source_id: SourceId,
        url: String,
        status: u16,
    },

    #[error("could not read {source_id} response from {url}: expected {expected}, {reason}")]
    SourceShape {
        source_id: SourceId,
        url: String,
        expected: &'static str,
        reason: String,
    },

    #[error("{source_id} needs an API key before it can be queried; add one on the Sources screen")]
    MissingApiKey { source_id: SourceId },

    #[error("{addon_id} offers no download for World of Warcraft: Forever")]
    NoForeverDownload { addon_id: AddonId },

    #[error(
        "{addon_id}'s author distributes downloads only through the source's website; open the addon's page to download it"
    )]
    ExternalDownloadOnly { addon_id: AddonId },

    #[error(
        "{addon_id} is published as a .{format} archive, which this application cannot open; download it from the source's page instead"
    )]
    UnsupportedArchive { addon_id: AddonId, format: String },

    #[error("{addon_id} is not in the catalog; refresh the catalog and try again")]
    UnknownAddon { addon_id: AddonId },

    #[error("the archive for {addon_id} contains no addon folder with a .toc file")]
    ArchiveHasNoAddon { addon_id: AddonId },

    #[error("the archive for {addon_id} contains an unsafe path: {entry}")]
    ArchiveUnsafePath { addon_id: AddonId, entry: String },

    #[error("the archive for {addon_id} is not a readable zip: {reason}")]
    ArchiveUnreadable { addon_id: AddonId, reason: String },

    #[error("{addon_id} is not installed")]
    NotInstalled { addon_id: AddonId },

    #[error("could not {action} {path}: {reason}")]
    Io {
        action: &'static str,
        path: PathBuf,
        reason: String,
    },

    #[error("could not {action} {path}: {reason}")]
    Persist {
        action: &'static str,
        path: PathBuf,
        reason: String,
    },
}

impl AppError {
    pub(crate) fn io(action: &'static str, path: impl Into<PathBuf>, err: &std::io::Error) -> Self {
        Self::Io {
            action,
            path: path.into(),
            reason: err.to_string(),
        }
    }
}

/// Tauri commands must hand the frontend something serializable; the rendered
/// message is the whole contract, so the payload is a plain string.
impl serde::Serialize for AppError {
    fn serialize<S: serde::Serializer>(
        &self,
        serializer: S,
    ) -> std::result::Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.to_string())
    }
}

pub type Result<T> = std::result::Result<T, AppError>;
