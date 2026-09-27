use std::fmt;

/// The addon providers this build knows how to talk to.
///
/// Adding a provider means adding a variant here and an implementation in
/// `crate::source`; the exhaustive matches will point at everything that needs
/// to learn about it.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, serde::Serialize, serde::Deserialize,
)]
#[serde(rename_all = "kebab-case")]
pub enum SourceId {
    CurseForge,
    Wago,
    /// GitHub releases of repositories the user tracks by hand — the escape
    /// hatch for addons that only ship on GitHub.
    GitHub,
}

impl SourceId {
    pub const ALL: [Self; 3] = [Self::CurseForge, Self::Wago, Self::GitHub];

    /// Stable machine name, used in ids, cache filenames, and the UI's routing.
    pub const fn slug(self) -> &'static str {
        match self {
            Self::CurseForge => "curseforge",
            Self::Wago => "wago",
            Self::GitHub => "github",
        }
    }

    pub const fn display_name(self) -> &'static str {
        match self {
            Self::CurseForge => "CurseForge",
            Self::Wago => "Wago Addons",
            Self::GitHub => "GitHub",
        }
    }

    pub const fn site_url(self) -> &'static str {
        match self {
            Self::CurseForge => {
                "https://www.curseforge.com/wow/search?class=addons&gameVersionTypeId=88568"
            }
            Self::Wago => "https://addons.wago.io/",
            Self::GitHub => "https://github.com/",
        }
    }

    /// How this source's addons reach the browse list.
    pub const fn listing(self) -> Listing {
        match self {
            Self::CurseForge => Listing::Live(LiveSource::CurseForge),
            Self::Wago => Listing::Catalog(CatalogSource::Wago),
            Self::GitHub => Listing::Catalog(CatalogSource::GitHub),
        }
    }

    /// True for sources that cannot be queried without a user-supplied key.
    pub const fn requires_api_key(self) -> bool {
        match self {
            Self::CurseForge | Self::Wago => true,
            Self::GitHub => false,
        }
    }

    pub fn from_slug(slug: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|source| source.slug() == slug)
    }
}

/// The two ways a source can be browsed. Splitting the sources this way lets
/// the compiler refuse a catalog pull for a source whose terms forbid one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Listing {
    /// Pulled whole on demand and cached on disk.
    Catalog(CatalogSource),
    /// Queried each time the browse list changes; nothing is kept.
    Live(LiveSource),
}

/// A source whose whole listing is pulled and cached.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum CatalogSource {
    Wago,
    GitHub,
}

impl CatalogSource {
    pub const ALL: [Self; 2] = [Self::Wago, Self::GitHub];

    pub const fn id(self) -> SourceId {
        match self {
            Self::Wago => SourceId::Wago,
            Self::GitHub => SourceId::GitHub,
        }
    }
}

/// A source queried live. CurseForge is one because its API terms forbid
/// saving or caching data obtained through the API.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum LiveSource {
    CurseForge,
}

impl LiveSource {
    pub const ALL: [Self; 1] = [Self::CurseForge];

    pub const fn id(self) -> SourceId {
        match self {
            Self::CurseForge => SourceId::CurseForge,
        }
    }
}

impl fmt::Display for SourceId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.display_name())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_every_source_through_its_slug() {
        for source in SourceId::ALL {
            assert_eq!(SourceId::from_slug(source.slug()), Some(source));
        }
    }

    #[test]
    fn every_source_round_trips_through_its_listing() {
        for source in SourceId::ALL {
            let back = match source.listing() {
                Listing::Catalog(catalog) => catalog.id(),
                Listing::Live(live) => live.id(),
            };
            assert_eq!(back, source);
        }
    }

    #[test]
    fn curseforge_is_never_a_catalog_source() {
        assert_eq!(
            SourceId::CurseForge.listing(),
            Listing::Live(LiveSource::CurseForge)
        );
    }

    #[test]
    fn rejects_an_unknown_slug() {
        assert_eq!(SourceId::from_slug("wowinterface"), None);
    }
}
