//! An addon's published files: every build a source lists for Forever, how
//! each relates to other addons, and which one is installed.

use std::fmt;

use crate::domain::{AddonFolder, AddonId, AddonSummary};

/// A source-local file identifier — a file id on CurseForge.
#[derive(Debug, Clone, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[serde(transparent)]
pub struct FileKey(String);

impl FileKey {
    pub fn new(raw: impl Into<String>) -> Option<Self> {
        let raw = raw.into();
        let trimmed = raw.trim();
        (!trimmed.is_empty()).then(|| Self(trimmed.to_owned()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for FileKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// How stable the author says a file is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum Channel {
    Release,
    Beta,
    Alpha,
    /// A channel code this build does not know.
    Unknown {
        code: u8,
    },
}

/// What a file says about another addon.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum Relation {
    /// Must be installed alongside for this file to load.
    Required,
    /// Used when present.
    Optional,
    /// A companion tool the author recommends.
    Tool,
    /// Breaks, or is broken by, this file.
    Incompatible,
    /// Shipped inside this file's own folders.
    Embedded,
    /// Packaged into this file's archive as folders of its own.
    Included,
    /// A relation code this build does not know.
    Unknown { code: u8 },
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Dependency {
    pub addon: AddonId,
    pub relation: Relation,
}

/// One downloadable build of an addon.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct PublishedFile {
    pub id: FileKey,
    /// The author's name for the build; usually, not always, its version.
    pub name: String,
    pub file_name: String,
    pub channel: Channel,
    /// RFC 3339, in UTC.
    pub published_at: String,
    pub size: Option<u64>,
    pub downloads: Option<u64>,
    /// The folders the archive places under `Interface/AddOns`.
    pub folders: Vec<AddonFolder>,
    pub dependencies: Vec<Dependency>,
}

/// An addon some file depends on, as far as its source could say.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(tag = "availability", rename_all = "kebab-case")]
pub enum RelatedAddon {
    /// Listed with a Forever file, so it can be installed from here.
    Listed(Box<AddonSummary>),
    /// Listed, but with no Forever file of its own — typically a library
    /// that only ever ships embedded.
    NoForeverFile {
        id: AddonId,
        name: String,
        page_url: String,
    },
    /// Not in the source's answer at all: deleted, private, or moderated.
    Unlisted { id: AddonId },
}

impl RelatedAddon {
    pub fn id(&self) -> &AddonId {
        match self {
            Self::Listed(summary) => &summary.id,
            Self::NoForeverFile { id, .. } | Self::Unlisted { id } => id,
        }
    }
}

/// Every Forever file of one addon, newest first.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct FileHistory {
    pub files: Vec<PublishedFile>,
    /// The file an install would fetch now.
    pub target: Option<FileKey>,
    /// The file an install at the recorded time would have fetched. The
    /// manifest keeps no file id, but an install always takes the preferred
    /// file of its moment, so the time identifies it.
    pub installed: Option<FileKey>,
    /// Every addon any file depends on, in first-mentioned order.
    pub related: Vec<RelatedAddon>,
}

impl FileHistory {
    /// `installed_at` is the manifest's RFC 3339 install time, or `None` when
    /// the addon is not installed.
    pub fn new(
        mut files: Vec<PublishedFile>,
        installed_at: Option<&str>,
        related: Vec<RelatedAddon>,
    ) -> Self {
        // RFC 3339 in UTC orders lexicographically.
        files.sort_by(|a, b| b.published_at.cmp(&a.published_at));

        let target = preferred(files.iter()).map(|file| file.id.clone());
        let installed = installed_at.and_then(|at| {
            preferred(files.iter().filter(|file| file.published_at.as_str() <= at))
                .map(|file| file.id.clone())
        });

        Self {
            files,
            target,
            installed,
            related,
        }
    }
}

/// Every addon `files` mention, each once, in first-mentioned order.
pub fn mentioned_addons(files: &[PublishedFile]) -> Vec<AddonId> {
    let mut seen = Vec::new();
    for dependency in files.iter().flat_map(|file| &file.dependencies) {
        if !seen.contains(&dependency.addon) {
            seen.push(dependency.addon.clone());
        }
    }
    seen
}

/// The newest stable release, or the newest file of any channel when there
/// is none — what an install picks. `files` must be newest first.
fn preferred<'a>(
    mut files: impl Iterator<Item = &'a PublishedFile> + Clone,
) -> Option<&'a PublishedFile> {
    files
        .clone()
        .find(|file| file.channel == Channel::Release)
        .or_else(|| files.next())
}

/// Whether a source publishes file histories at all.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum FileListing {
    Listed(FileHistory),
    /// The source publishes one current download and nothing older.
    NotOffered,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{AddonKey, SourceId};

    fn addon(key: &str) -> AddonId {
        AddonId::new(SourceId::CurseForge, AddonKey::new(key).expect("non-empty"))
    }

    fn file(id: &str, channel: Channel, published_at: &str) -> PublishedFile {
        PublishedFile {
            id: FileKey::new(id).expect("non-empty"),
            name: id.to_owned(),
            file_name: format!("{id}.zip"),
            channel,
            published_at: published_at.to_owned(),
            size: None,
            downloads: None,
            folders: Vec::new(),
            dependencies: Vec::new(),
        }
    }

    fn key(history: &FileHistory, pick: fn(&FileHistory) -> &Option<FileKey>) -> Option<&str> {
        pick(history).as_ref().map(FileKey::as_str)
    }

    fn questie_files() -> Vec<PublishedFile> {
        vec![
            file("1.4.2", Channel::Release, "2026-09-09T10:00:00Z"),
            file("1.5.0-beta", Channel::Beta, "2026-09-24T10:00:00Z"),
            file("1.4.1", Channel::Release, "2026-08-30T10:00:00Z"),
            file("1.4.3", Channel::Release, "2026-09-18T10:00:00Z"),
        ]
    }

    #[test]
    fn orders_files_newest_first() {
        let history = FileHistory::new(questie_files(), None, Vec::new());

        let order = history
            .files
            .iter()
            .map(|file| file.id.as_str())
            .collect::<Vec<_>>();
        assert_eq!(order, ["1.5.0-beta", "1.4.3", "1.4.2", "1.4.1"]);
    }

    #[test]
    fn targets_the_newest_release_over_a_newer_beta() {
        let history = FileHistory::new(questie_files(), None, Vec::new());

        assert_eq!(key(&history, |h| &h.target), Some("1.4.3"));
    }

    #[test]
    fn targets_the_newest_prerelease_when_nothing_is_stable() {
        let files = vec![
            file("r2", Channel::Alpha, "2026-09-22T05:00:00Z"),
            file("r1", Channel::Alpha, "2026-09-21T05:00:00Z"),
        ];
        let history = FileHistory::new(files, None, Vec::new());

        assert_eq!(key(&history, |h| &h.target), Some("r2"));
    }

    #[test]
    fn identifies_the_installed_file_by_install_time() {
        let history = FileHistory::new(questie_files(), Some("2026-09-01T12:00:00Z"), Vec::new());

        assert_eq!(key(&history, |h| &h.installed), Some("1.4.1"));
    }

    #[test]
    fn marks_nothing_installed_when_the_addon_is_not() {
        let history = FileHistory::new(questie_files(), None, Vec::new());

        assert_eq!(history.installed, None);
    }

    #[test]
    fn marks_nothing_installed_when_every_file_is_newer_than_the_install() {
        let history = FileHistory::new(questie_files(), Some("2026-01-01T00:00:00Z"), Vec::new());

        assert_eq!(history.installed, None);
    }

    #[test]
    fn lists_each_mentioned_addon_once_in_first_mentioned_order() {
        let mut files = questie_files();
        files[0].dependencies = vec![
            Dependency {
                addon: addon("2"),
                relation: Relation::Required,
            },
            Dependency {
                addon: addon("1"),
                relation: Relation::Embedded,
            },
        ];
        files[1].dependencies = vec![Dependency {
            addon: addon("2"),
            relation: Relation::Optional,
        }];

        assert_eq!(mentioned_addons(&files), [addon("2"), addon("1")]);
    }
}
