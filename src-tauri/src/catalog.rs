//! The browsable set of addons, and its on-disk cache.
//!
//! A catalog refresh can cost a couple of dozen requests, so Wago and GitHub
//! catalogs are cached per source and only refetched when the user asks.
//! CurseForge has no catalog: its API terms forbid saving or caching data
//! obtained through the API, so it is queried live (see [`Listing`]) and its
//! results are merged in here per search.

use std::path::{Path, PathBuf};

use crate::domain::{AddonId, AddonSummary, CatalogSource, Listing, Sort, SourceId};
use crate::error::{AppError, Result};
use crate::source::SourceNotice;

/// One source's addons, as of one refresh.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Catalog {
    pub source: SourceId,
    /// RFC 3339, in UTC.
    pub fetched_at: String,
    pub addons: Vec<AddonSummary>,
}

impl Catalog {
    pub fn new(source: CatalogSource, addons: Vec<AddonSummary>) -> Self {
        Self {
            source: source.id(),
            fetched_at: crate::install::manifest::now_rfc3339(),
            addons,
        }
    }

    pub fn find(&self, id: &AddonId) -> Option<&AddonSummary> {
        self.addons.iter().find(|addon| &addon.id == id)
    }
}

/// How the browse list is narrowed and ordered. Every field is optional; an
/// empty query returns the whole catalog, most downloaded first.
#[derive(Debug, Clone, Default, serde::Deserialize)]
pub struct CatalogQuery {
    #[serde(default)]
    pub text: String,
    #[serde(default)]
    pub category: Option<String>,
    #[serde(default)]
    pub sources: Option<Vec<SourceId>>,
    #[serde(default)]
    pub sort: Sort,
}

impl CatalogQuery {
    pub fn includes_source(&self, source: SourceId) -> bool {
        self.sources
            .as_ref()
            .is_none_or(|sources| sources.contains(&source))
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
}

/// What the browse list renders for one query.
#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize)]
pub struct SearchResults {
    pub addons: Vec<AddonSummary>,
    /// How many addons the text matched before the source and category
    /// filters, so the list can say "4 of 19".
    pub text_matches: usize,
    /// Matches per searched source, counted before the source filter so a
    /// switched-off source still says what it would add. A live source the
    /// query left out was never asked and has no entry.
    pub sources: Vec<SourceCount>,
    /// Every category among the matches before the category filter, so the
    /// menu keeps offering the one currently selected.
    pub categories: Vec<CategoryCount>,
    /// Why a live source's results are partial or missing.
    pub notices: Vec<SourceNotice>,
}

/// How many of a query's matches one source holds.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
pub struct SourceCount {
    pub source: SourceId,
    pub count: usize,
}

/// How many of a query's matches list one category.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct CategoryCount {
    pub name: String,
    pub count: usize,
}

/// The catalog addons a query's text matches, from every catalog.
///
/// The source filter is left to [`assemble`], which counts each source's
/// matches before applying it.
pub fn matching<'a>(
    catalogs: impl IntoIterator<Item = &'a Catalog>,
    query: &CatalogQuery,
) -> Vec<AddonSummary> {
    catalogs
        .into_iter()
        .flat_map(|catalog| catalog.addons.iter())
        .filter(|addon| addon.matches_text(&query.text))
        .cloned()
        .collect()
}

/// Narrows every searched source's candidates by source and category, counts
/// each facet before its own filter, and puts the rest in the query's order.
///
/// Live sources are asked for the same order server-side, so the page they
/// return is the right one; sorting again here is what merges it with the
/// locally matched catalogs.
pub fn assemble(
    candidates: Vec<AddonSummary>,
    searched: &[SourceId],
    query: &CatalogQuery,
    notices: Vec<SourceNotice>,
) -> SearchResults {
    let text_matches = candidates.len();
    let sources = sources_of(&candidates, searched, query);
    let categories = categories_of(
        candidates
            .iter()
            .filter(|addon| query.includes_source(addon.id.source)),
    );

    let mut addons = candidates
        .into_iter()
        .filter(|addon| query.includes_source(addon.id.source) && query.matches_category(addon))
        .collect::<Vec<_>>();
    addons.sort_by(|left, right| query.sort.compare(left, right));

    SearchResults {
        addons,
        text_matches,
        sources,
        categories,
        notices,
    }
}

/// Each searched source's matches within the query's category.
fn sources_of(
    candidates: &[AddonSummary],
    searched: &[SourceId],
    query: &CatalogQuery,
) -> Vec<SourceCount> {
    searched
        .iter()
        .map(|&source| SourceCount {
            source,
            count: candidates
                .iter()
                .filter(|addon| addon.id.source == source && query.matches_category(addon))
                .count(),
        })
        .collect()
}

/// Every category the addons use, sorted and listed once with how many addons
/// use it, for the filter menu.
fn categories_of<'a>(addons: impl Iterator<Item = &'a AddonSummary>) -> Vec<CategoryCount> {
    let mut names = addons
        .flat_map(|addon| addon.categories.iter())
        .collect::<Vec<_>>();
    names.sort_by_key(|name| name.to_lowercase());

    let mut counts: Vec<CategoryCount> = Vec::new();
    for name in names {
        match counts.last_mut() {
            Some(last) if last.name.eq_ignore_ascii_case(name) => last.count += 1,
            _ => counts.push(CategoryCount {
                name: name.clone(),
                count: 1,
            }),
        }
    }
    counts
}

pub fn cache_path(data_dir: &Path, source: SourceId) -> PathBuf {
    data_dir.join(format!("catalog-{}.json", source.slug()))
}

/// A cache that cannot be read is simply absent; the user refreshes.
pub fn load_cached(data_dir: &Path, source: CatalogSource) -> Option<Catalog> {
    let raw = std::fs::read_to_string(cache_path(data_dir, source.id())).ok()?;
    let catalog: Catalog = serde_json::from_str(&raw).ok()?;

    (catalog.source == source.id()).then_some(catalog)
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

/// Deletes the catalog files earlier builds wrote for sources that are now
/// queried live — CurseForge's, whose terms forbid keeping it.
pub fn purge_live_source_caches(data_dir: &Path) -> Result<()> {
    let live = SourceId::ALL
        .into_iter()
        .filter(|source| matches!(source.listing(), Listing::Live(_)));

    for source in live {
        let path = cache_path(data_dir, source);
        match std::fs::remove_file(&path) {
            Ok(()) => {}
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => {}
            Err(err) => return Err(AppError::io("delete the catalog cache", &path, &err)),
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{AddonKey, Download, Expansion, SortDirection, SortField};

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
                CatalogSource::GitHub,
                vec![
                    addon(
                        SourceId::GitHub,
                        "tuller/bagnon",
                        "Bagnon",
                        "Tuller",
                        &["Inventory"],
                        500,
                    ),
                    addon(
                        SourceId::GitHub,
                        "aero/questie",
                        "Questie",
                        "Aero",
                        &["Quests"],
                        9_000,
                    ),
                ],
            ),
            Catalog::new(
                CatalogSource::Wago,
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

    fn results(catalogs: &[Catalog], query: &CatalogQuery) -> SearchResults {
        let searched = catalogs
            .iter()
            .map(|catalog| catalog.source)
            .collect::<Vec<_>>();
        assemble(matching(catalogs, query), &searched, query, Vec::new())
    }

    fn search(catalogs: &[Catalog], query: &CatalogQuery) -> Vec<AddonSummary> {
        results(catalogs, query).addons
    }

    fn category_names(results: &SearchResults) -> Vec<&str> {
        results
            .categories
            .iter()
            .map(|category| category.name.as_str())
            .collect()
    }

    #[test]
    fn returns_the_whole_catalog_for_an_empty_query() {
        let found = search(&catalogs(), &CatalogQuery::default());
        assert_eq!(found.len(), 3);
    }

    #[test]
    fn orders_results_by_popularity() {
        let found = search(&catalogs(), &CatalogQuery::default());
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
            let found = search(&catalogs(), &query);

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

        assert!(search(&catalogs(), &query).is_empty());
    }

    #[test]
    fn narrows_to_one_category() {
        let query = CatalogQuery {
            category: Some("boss encounters".to_owned()),
            ..CatalogQuery::default()
        };
        let found = search(&catalogs(), &query);

        assert_eq!(found.len(), 1);
        assert_eq!(found[0].name, "Deadly Boss Mods");
    }

    #[test]
    fn narrows_to_one_source() {
        let query = CatalogQuery {
            sources: Some(vec![SourceId::Wago]),
            ..CatalogQuery::default()
        };
        let found = search(&catalogs(), &query);

        assert_eq!(found.len(), 1);
        assert_eq!(found[0].id.source, SourceId::Wago);
    }

    #[test]
    fn lists_each_category_once_across_sources() {
        assert_eq!(
            category_names(&results(&catalogs(), &CatalogQuery::default())),
            vec!["Boss Encounters", "Combat", "Inventory", "Quests"]
        );
    }

    #[test]
    fn counts_the_addons_in_each_category() {
        let mut catalogs = catalogs();
        catalogs[0].addons[0].categories = vec!["quests".to_owned()];

        let found = results(&catalogs, &CatalogQuery::default());
        let quests = found
            .categories
            .iter()
            .find(|category| category.name.eq_ignore_ascii_case("quests"))
            .expect("quests is listed");

        assert_eq!(quests.count, 2);
        assert_eq!(found.categories.len(), 3);
    }

    #[test]
    fn counts_each_source_before_the_source_filter() {
        let query = CatalogQuery {
            sources: Some(vec![SourceId::Wago]),
            ..CatalogQuery::default()
        };
        let found = results(&catalogs(), &query);

        assert_eq!(found.addons.len(), 1);
        assert_eq!(found.text_matches, 3);
        assert_eq!(
            found.sources,
            vec![
                SourceCount {
                    source: SourceId::GitHub,
                    count: 2
                },
                SourceCount {
                    source: SourceId::Wago,
                    count: 1
                },
            ]
        );
    }

    #[test]
    fn counts_sources_within_the_selected_category() {
        let query = CatalogQuery {
            category: Some("Quests".to_owned()),
            ..CatalogQuery::default()
        };
        let counts = results(&catalogs(), &query)
            .sources
            .into_iter()
            .map(|entry| (entry.source, entry.count))
            .collect::<Vec<_>>();

        assert_eq!(counts, vec![(SourceId::GitHub, 1), (SourceId::Wago, 0)]);
    }

    #[test]
    fn lists_only_the_included_sources_categories() {
        let query = CatalogQuery {
            sources: Some(vec![SourceId::Wago]),
            ..CatalogQuery::default()
        };

        assert_eq!(
            category_names(&results(&catalogs(), &query)),
            vec!["Boss Encounters", "Combat"]
        );
    }

    #[test]
    fn gives_an_unsearched_source_no_count() {
        let query = CatalogQuery::default();
        let found = assemble(
            matching(&catalogs(), &query),
            &[SourceId::Wago],
            &query,
            Vec::new(),
        );

        assert_eq!(found.sources.len(), 1);
        assert_eq!(found.sources[0].source, SourceId::Wago);
    }

    #[test]
    fn keeps_offering_the_selected_category_after_filtering_by_it() {
        let query = CatalogQuery {
            category: Some("Quests".to_owned()),
            ..CatalogQuery::default()
        };
        let found = results(&catalogs(), &query);

        assert_eq!(found.addons.len(), 1);
        assert_eq!(found.categories.len(), 4);
    }

    #[test]
    fn merges_live_results_into_the_popularity_order() {
        let query = CatalogQuery::default();
        let mut candidates = matching(&catalogs(), &query);
        candidates.push(addon(
            SourceId::CurseForge,
            "3358",
            "Deadly Boss Mods Core",
            "MysticalOS",
            &["Boss Encounters"],
            90_000,
        ));

        let names = assemble(candidates, &[SourceId::CurseForge], &query, Vec::new())
            .addons
            .into_iter()
            .map(|addon| addon.name)
            .collect::<Vec<_>>();
        assert_eq!(
            names,
            vec![
                "Deadly Boss Mods Core",
                "Deadly Boss Mods",
                "Questie",
                "Bagnon"
            ]
        );
    }

    #[test]
    fn applies_the_query_sort_across_every_source() {
        let query = CatalogQuery {
            sort: Sort {
                field: SortField::Name,
                direction: SortDirection::Ascending,
            },
            ..CatalogQuery::default()
        };
        let mut candidates = matching(&catalogs(), &query);
        candidates.push(addon(
            SourceId::CurseForge,
            "3358",
            "Auctionator",
            "Borjamacare",
            &["Auction & Economy"],
            90_000,
        ));

        let names = assemble(candidates, &[SourceId::CurseForge], &query, Vec::new())
            .addons
            .into_iter()
            .map(|addon| addon.name)
            .collect::<Vec<_>>();
        assert_eq!(
            names,
            vec!["Auctionator", "Bagnon", "Deadly Boss Mods", "Questie"]
        );
    }

    #[test]
    fn round_trips_a_catalog_through_the_cache() {
        let temp = tempfile::tempdir().expect("temp dir");
        let catalog = catalogs().remove(1);

        save_cached(temp.path(), &catalog).expect("cache is written");
        assert_eq!(load_cached(temp.path(), CatalogSource::Wago), Some(catalog));
    }

    #[test]
    fn purges_a_curseforge_cache_left_by_an_earlier_build() {
        let temp = tempfile::tempdir().expect("temp dir");
        let stale = cache_path(temp.path(), SourceId::CurseForge);
        std::fs::write(&stale, b"{}").expect("fixture write");

        purge_live_source_caches(temp.path()).expect("purge succeeds");

        assert!(!stale.exists());
    }

    #[test]
    fn purging_leaves_catalog_source_caches_alone() {
        let temp = tempfile::tempdir().expect("temp dir");
        let catalog = catalogs().remove(1);
        save_cached(temp.path(), &catalog).expect("cache is written");

        purge_live_source_caches(temp.path()).expect("purge succeeds");

        assert_eq!(load_cached(temp.path(), CatalogSource::Wago), Some(catalog));
    }

    #[test]
    fn reports_no_cache_for_a_source_that_was_never_refreshed() {
        let temp = tempfile::tempdir().expect("temp dir");
        assert_eq!(load_cached(temp.path(), CatalogSource::Wago), None);
    }
}
