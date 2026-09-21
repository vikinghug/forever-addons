//! The browsable set of addons, and its on-disk cache.
//!
//! A catalog refresh can cost a couple of dozen requests, so it is cached per
//! source and only refetched when the user asks.

use std::path::{Path, PathBuf};

use crate::domain::{AddonId, AddonSummary, SourceId};
use crate::error::{AppError, Result};

/// One source's addons, as of one refresh.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Catalog {
    pub source: SourceId,
    /// RFC 3339, in UTC.
    pub fetched_at: String,
    pub addons: Vec<AddonSummary>,
}

impl Catalog {
    pub fn new(source: SourceId, addons: Vec<AddonSummary>) -> Self {
        Self {
            source,
            fetched_at: crate::install::manifest::now_rfc3339(),
            addons,
        }
    }

    pub fn find(&self, id: &AddonId) -> Option<&AddonSummary> {
        self.addons.iter().find(|addon| &addon.id == id)
    }
}

/// How the browse list is narrowed. Every field is optional; an empty query
/// returns the whole catalog.
#[derive(Debug, Clone, Default, serde::Deserialize)]
pub struct CatalogQuery {
    #[serde(default)]
    pub text: String,
    #[serde(default)]
    pub category: Option<String>,
    #[serde(default)]
    pub sources: Option<Vec<SourceId>>,
}

impl CatalogQuery {
    fn matches(&self, addon: &AddonSummary) -> bool {
        self.matches_source(addon) && self.matches_category(addon) && self.matches_text(addon)
    }

    fn matches_source(&self, addon: &AddonSummary) -> bool {
        self.sources
            .as_ref()
            .is_none_or(|sources| sources.contains(&addon.id.source))
    }

    fn matches_category(&self, addon: &AddonSummary) -> bool {
        let Some(category) = self.category.as_deref() else {
            return true;
        };

        addon
            .categories
            .iter()
            .any(|found| found.eq_ignore_ascii_case(category))
    }

    fn matches_text(&self, addon: &AddonSummary) -> bool {
        let needle = self.text.trim().to_lowercase();
        if needle.is_empty() {
            return true;
        }

        needle.split_whitespace().all(|word| {
            addon.name.to_lowercase().contains(word)
                || addon.summary.to_lowercase().contains(word)
                || addon
                    .author
                    .as_ref()
                    .is_some_and(|author| author.to_lowercase().contains(word))
        })
    }
}

/// Applies a query across every loaded catalog, most-downloaded first.
///
/// Popularity is the only ordering the sources agree on, and it is what makes
/// an unfiltered list useful; ties fall back to name so the order is stable.
pub fn search<'a>(
    catalogs: impl IntoIterator<Item = &'a Catalog>,
    query: &CatalogQuery,
) -> Vec<AddonSummary> {
    let mut found = catalogs
        .into_iter()
        .flat_map(|catalog| catalog.addons.iter())
        .filter(|addon| query.matches(addon))
        .cloned()
        .collect::<Vec<_>>();

    found.sort_by(|left, right| {
        right
            .downloads
            .unwrap_or(0)
            .cmp(&left.downloads.unwrap_or(0))
            .then_with(|| left.name.to_lowercase().cmp(&right.name.to_lowercase()))
    });

    found
}

/// Every category any loaded catalog uses, sorted, for the filter menu.
pub fn categories<'a>(catalogs: impl IntoIterator<Item = &'a Catalog>) -> Vec<String> {
    let mut names = catalogs
        .into_iter()
        .flat_map(|catalog| catalog.addons.iter())
        .flat_map(|addon| addon.categories.iter().cloned())
        .collect::<Vec<_>>();

    names.sort_by_key(|name| name.to_lowercase());
    names.dedup_by(|left, right| left.eq_ignore_ascii_case(right));
    names
}

pub fn cache_path(data_dir: &Path, source: SourceId) -> PathBuf {
    data_dir.join(format!("catalog-{}.json", source.slug()))
}

/// A cache that cannot be read is simply absent; the user refreshes.
pub fn load_cached(data_dir: &Path, source: SourceId) -> Option<Catalog> {
    let raw = std::fs::read_to_string(cache_path(data_dir, source)).ok()?;
    let catalog: Catalog = serde_json::from_str(&raw).ok()?;

    (catalog.source == source).then_some(catalog)
}

pub fn save_cached(data_dir: &Path, catalog: &Catalog) -> Result<()> {
    std::fs::create_dir_all(data_dir)
        .map_err(|err| AppError::io("create the data directory", data_dir, &err))?;

    let path = cache_path(data_dir, catalog.source);
    let body = serde_json::to_string(catalog).map_err(|err| AppError::Persist {
        action: "serialize the catalog cache",
        path: path.clone(),
        reason: err.to_string(),
    })?;

    std::fs::write(&path, body).map_err(|err| AppError::io("write the catalog cache", &path, &err))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{AddonKey, Download, Expansion};

    fn addon(
        source: SourceId,
        key: &str,
        name: &str,
        author: &str,
        categories: &[&str],
        downloads: u64,
    ) -> AddonSummary {
        AddonSummary {
            id: AddonId::new(source, AddonKey::new(key).expect("non-empty")),
            name: name.to_owned(),
            summary: format!("{name} does things"),
            author: Some(author.to_owned()),
            version: None,
            updated_at: None,
            icon_url: None,
            page_url: String::new(),
            categories: categories.iter().map(|c| (*c).to_owned()).collect(),
            downloads: Some(downloads),
            expansions: vec![Expansion::Forever],
            download: Download::Brokered,
        }
    }

    fn catalogs() -> Vec<Catalog> {
        vec![
            Catalog::new(
                SourceId::CurseForge,
                vec![
                    addon(
                        SourceId::CurseForge,
                        "1",
                        "Bagnon",
                        "Tuller",
                        &["Inventory"],
                        500,
                    ),
                    addon(
                        SourceId::CurseForge,
                        "2",
                        "Questie",
                        "Aero",
                        &["Quests"],
                        9_000,
                    ),
                ],
            ),
            Catalog::new(
                SourceId::Wago,
                vec![addon(
                    SourceId::Wago,
                    "3",
                    "Deadly Boss Mods",
                    "Tandanu",
                    &["Boss Encounters", "Combat"],
                    50_000,
                )],
            ),
        ]
    }

    #[test]
    fn returns_the_whole_catalog_for_an_empty_query() {
        let found = search(catalogs().iter(), &CatalogQuery::default());
        assert_eq!(found.len(), 3);
    }

    #[test]
    fn orders_results_by_popularity() {
        let found = search(catalogs().iter(), &CatalogQuery::default());
        let names = found.iter().map(|a| a.name.as_str()).collect::<Vec<_>>();

        assert_eq!(names, vec!["Deadly Boss Mods", "Questie", "Bagnon"]);
    }

    #[test]
    fn matches_text_against_name_summary_and_author() {
        for text in ["bagnon", "TULLER", "Bagnon does"] {
            let query = CatalogQuery {
                text: text.to_owned(),
                ..CatalogQuery::default()
            };
            let found = search(catalogs().iter(), &query);

            assert_eq!(found.len(), 1, "{text}");
            assert_eq!(found[0].name, "Bagnon");
        }
    }

    #[test]
    fn requires_every_word_of_a_multi_word_query_to_match() {
        let query = CatalogQuery {
            text: "deadly bagnon".to_owned(),
            ..CatalogQuery::default()
        };

        assert!(search(catalogs().iter(), &query).is_empty());
    }

    #[test]
    fn narrows_to_one_category() {
        let query = CatalogQuery {
            category: Some("boss encounters".to_owned()),
            ..CatalogQuery::default()
        };
        let found = search(catalogs().iter(), &query);

        assert_eq!(found.len(), 1);
        assert_eq!(found[0].name, "Deadly Boss Mods");
    }

    #[test]
    fn narrows_to_one_source() {
        let query = CatalogQuery {
            sources: Some(vec![SourceId::Wago]),
            ..CatalogQuery::default()
        };
        let found = search(catalogs().iter(), &query);

        assert_eq!(found.len(), 1);
        assert_eq!(found[0].id.source, SourceId::Wago);
    }

    #[test]
    fn lists_each_category_once_across_sources() {
        assert_eq!(
            categories(catalogs().iter()),
            vec!["Boss Encounters", "Combat", "Inventory", "Quests"]
        );
    }

    #[test]
    fn round_trips_a_catalog_through_the_cache() {
        let temp = tempfile::tempdir().expect("temp dir");
        let catalog = catalogs().remove(0);

        save_cached(temp.path(), &catalog).expect("cache is written");
        assert_eq!(
            load_cached(temp.path(), SourceId::CurseForge),
            Some(catalog)
        );
    }

    #[test]
    fn reports_no_cache_for_a_source_that_was_never_refreshed() {
        let temp = tempfile::tempdir().expect("temp dir");
        assert_eq!(load_cached(temp.path(), SourceId::Wago), None);
    }
}
