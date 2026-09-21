use std::fmt;

use crate::domain::{AddonVersion, Expansion, SourceId};

/// A source-local identifier, opaque to everything but the source that minted
/// it: a mod id on CurseForge, an addon id on Wago, `owner/name` on GitHub.
#[derive(
    Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, serde::Serialize, serde::Deserialize,
)]
#[serde(transparent)]
pub struct AddonKey(String);

impl AddonKey {
    pub fn new(raw: impl Into<String>) -> Option<Self> {
        let raw = raw.into();
        let trimmed = raw.trim();
        (!trimmed.is_empty()).then(|| Self(trimmed.to_owned()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for AddonKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// An addon's identity across the whole application: which source, and which
/// addon within it. Two sources hosting "Bagnon" are two distinct addons.
#[derive(
    Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, serde::Serialize, serde::Deserialize,
)]
pub struct AddonId {
    pub source: SourceId,
    pub key: AddonKey,
}

impl AddonId {
    pub fn new(source: SourceId, key: AddonKey) -> Self {
        Self { source, key }
    }
}

impl fmt::Display for AddonId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}:{}", self.source.slug(), self.key)
    }
}

/// How the archive for an addon is obtained.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum Download {
    /// A stable archive URL the source publishes in its own data.
    Direct { url: String },
    /// The source issues a short-lived URL bound to the requesting client, so
    /// it can only be resolved at install time by the source implementation.
    Brokered,
    /// The source publishes an archive in a format this build cannot open.
    /// The addon stays in the catalog with its page reachable, rather than
    /// silently disappearing from it.
    Unsupported { url: String, format: String },
    /// The author distributes downloads only through the source's website —
    /// CurseForge mods can opt out of API distribution. The addon stays in the
    /// catalog with its page reachable, rather than silently disappearing.
    External { url: String },
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Screenshot {
    pub url: String,
}

/// One catalog row: everything the browse list renders, and nothing that costs
/// an extra request to obtain.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct AddonSummary {
    pub id: AddonId,
    pub name: String,
    pub summary: String,
    pub author: Option<String>,
    pub version: Option<AddonVersion>,
    /// When the source last changed this addon, RFC 3339 in UTC.
    ///
    /// Not every source publishes clean versions, so this is often the only
    /// signal that an installed copy has fallen behind.
    pub updated_at: Option<String>,
    pub icon_url: Option<String>,
    pub page_url: String,
    pub categories: Vec<String>,
    pub downloads: Option<u64>,
    /// Every game the source lists this addon under, unrecognised labels
    /// included.
    pub expansions: Vec<Expansion>,
    pub download: Download,
}

impl AddonSummary {
    /// False when the archive cannot be obtained by this build: an archive
    /// format it cannot open, or an author who only distributes via their page.
    pub fn is_installable(&self) -> bool {
        !matches!(
            self.download,
            Download::Unsupported { .. } | Download::External { .. }
        )
    }
}

/// A catalog row plus the fields that require visiting the addon's own page.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct AddonDetail {
    #[serde(flatten)]
    pub summary: AddonSummary,
    pub description: String,
    pub website_url: Option<String>,
    pub screenshots: Vec<Screenshot>,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn summary(expansions: Vec<Expansion>) -> AddonSummary {
        AddonSummary {
            id: AddonId::new(
                SourceId::CurseForge,
                AddonKey::new("6572").expect("non-empty key"),
            ),
            name: "Killshot".to_owned(),
            summary: String::new(),
            author: None,
            version: None,
            updated_at: None,
            icon_url: None,
            page_url: String::new(),
            categories: Vec::new(),
            downloads: None,
            expansions,
            download: Download::Brokered,
        }
    }

    #[test]
    fn renders_an_addon_id_as_source_qualified() {
        assert_eq!(summary(Vec::new()).id.to_string(), "curseforge:6572");
    }

    #[test]
    fn treats_a_blank_key_as_absent() {
        assert_eq!(AddonKey::new(" \n"), None);
    }

    #[test]
    fn is_not_installable_when_the_archive_format_is_unsupported() {
        let mut addon = summary(vec![Expansion::Forever]);
        addon.download = Download::Unsupported {
            url: "https://example.com/x.7z".to_owned(),
            format: "7z".to_owned(),
        };

        assert!(!addon.is_installable());
        assert!(summary(Vec::new()).is_installable());
    }

    #[test]
    fn is_not_installable_when_the_author_distributes_only_via_their_page() {
        let mut addon = summary(vec![Expansion::Forever]);
        addon.download = Download::External {
            url: "https://www.curseforge.com/wow/addons/details".to_owned(),
        };

        assert!(!addon.is_installable());
    }
}
