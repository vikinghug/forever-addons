//! CurseForge — <https://www.curseforge.com/wow>, queried for World of
//! Warcraft: Forever addons.
//!
//! CurseForge's site is Cloudflare-fronted, so this source talks to the
//! official Core API (<https://api.curseforge.com>) instead of scraping.
//! The API requires a per-user key (from console.curseforge.com), which is why
//! every function here takes one.
//!
//! CurseForge's API terms forbid saving or caching data obtained through the
//! API, so there is no catalog: every browse, detail, install, and update
//! check is its own request, and nothing it returns outlives the command that
//! asked. Search is granted per key; a key without it gets a 403 from
//! `/mods/search`, and browsing then falls back to the featured listing.
//!
//! Mod authors can opt out of API distribution; those mods stay browsable as
//! [`Download::External`] with their page reachable, rather than vanishing.

use std::collections::HashSet;

use crate::domain::{
    AddonDetail, AddonId, AddonKey, AddonSummary, Description, Download, Expansion, Screenshot,
    Sort, SortDirection, SortField, SourceId,
};
use crate::error::{AppError, Result};
use crate::source::http::HttpClient;
use crate::source::{LiveResults, SearchAccess, addon_id, missing_field};

const SOURCE: SourceId = SourceId::CurseForge;
const API: &str = "https://api.curseforge.com/v1";
/// CurseForge's game id for World of Warcraft.
const GAME_WOW: u32 = 1;
/// CurseForge's class id for the "Addons" section.
const CLASS_ADDONS: u32 = 1;
/// CurseForge's version-type id for World of Warcraft: Forever.
const FOREVER_VERSION_TYPE: u64 = 88568;
/// A stable release, in CurseForge's release-type numbering (2 beta, 3 alpha).
const RELEASE: u8 = 1;
/// One page of search results — the API's maximum.
const SEARCH_PAGE_SIZE: u32 = 50;
/// Shown in place of search results when the key may not search.
const SEARCH_FORBIDDEN: &str = "This CurseForge API key isn't allowed to search, so only \
    CurseForge's popular and recently updated Forever addons are matched. To find any other \
    addon, enter its project ID, shown on its CurseForge page.";

/// The subset of a CurseForge mod this application reads.
///
/// A DTO rather than the domain type: the camelCase names and nullable fields
/// are the API's conventions, and they stop here. Fields default so one
/// sparsely-filled mod does not sink a whole catalog page.
#[derive(Debug, Default, serde::Deserialize)]
#[serde(rename_all = "camelCase", default)]
struct Mod {
    id: u64,
    /// `1` for addons; the featured listing does not filter by class.
    class_id: Option<u32>,
    name: String,
    slug: String,
    summary: String,
    links: Links,
    download_count: u64,
    date_modified: String,
    /// `Some(false)` when the author forbids API distribution; their files
    /// then carry no `downloadUrl` either.
    allow_mod_distribution: Option<bool>,
    logo: Option<Asset>,
    screenshots: Vec<Asset>,
    categories: Vec<Category>,
    authors: Vec<Author>,
    latest_files: Vec<File>,
    latest_files_indexes: Vec<FileIndex>,
}

/// CurseForge sends an unset link as an explicit `null`, which `default`
/// alone does not absorb, so every link is optional.
#[derive(Debug, Default, serde::Deserialize)]
#[serde(rename_all = "camelCase", default)]
struct Links {
    website_url: Option<String>,
    source_url: Option<String>,
}

#[derive(Debug, Default, serde::Deserialize)]
#[serde(rename_all = "camelCase", default)]
struct Asset {
    thumbnail_url: Option<String>,
    url: Option<String>,
}

#[derive(Debug, Default, serde::Deserialize)]
#[serde(default)]
struct Category {
    name: String,
}

#[derive(Debug, Default, serde::Deserialize)]
#[serde(default)]
struct Author {
    name: String,
}

#[derive(Debug, Default, serde::Deserialize)]
#[serde(rename_all = "camelCase", default)]
struct File {
    id: u64,
    file_date: String,
    download_url: Option<String>,
}

/// One row of `latestFilesIndexes` — the per-game-version pointer into
/// `latestFiles`, which is how a mod names its Forever build.
#[derive(Debug, Default, serde::Deserialize)]
#[serde(rename_all = "camelCase", default)]
struct FileIndex {
    file_id: u64,
    release_type: u8,
    game_version_type_id: Option<u64>,
}

#[derive(Debug, serde::Deserialize)]
struct DataMods {
    data: Vec<Mod>,
}

#[derive(Debug, serde::Deserialize)]
struct DataFeatured {
    data: Featured,
}

#[derive(Debug, Default, serde::Deserialize)]
#[serde(rename_all = "camelCase", default)]
struct Featured {
    featured: Vec<Mod>,
    popular: Vec<Mod>,
    recently_updated: Vec<Mod>,
}

/// What the browse box's text asks CurseForge for.
#[derive(Debug, PartialEq, Eq)]
enum Query {
    /// A bare project ID, which CurseForge shows on every addon's page. Looked
    /// up directly, so it works for a key without search.
    ProjectId(AddonKey),
    Search(SearchTerm),
}

#[derive(Debug, PartialEq, Eq)]
enum SearchTerm {
    /// Blank: every Forever addon, in the chosen order.
    Everything,
    Text(String),
    /// From a pasted addon page URL. Only search can map a slug to a project.
    Slug(String),
}

impl Query {
    fn parse(text: &str) -> Self {
        let trimmed = text.trim();
        if trimmed.is_empty() {
            return Self::Search(SearchTerm::Everything);
        }
        if trimmed.bytes().all(|byte| byte.is_ascii_digit())
            && let Some(key) = AddonKey::new(trimmed)
        {
            return Self::ProjectId(key);
        }
        if let Some(slug) = slug_from_page_url(trimmed) {
            return Self::Search(SearchTerm::Slug(slug));
        }

        Self::Search(SearchTerm::Text(trimmed.to_owned()))
    }
}

impl SearchTerm {
    /// The local stand-in for search, used on the featured listing when the
    /// key may not search.
    fn matches(&self, summary: &AddonSummary) -> bool {
        match self {
            Self::Everything => true,
            Self::Text(text) => summary.matches_text(text),
            Self::Slug(slug) => summary
                .page_url
                .trim_end_matches('/')
                .ends_with(&format!("/{slug}")),
        }
    }
}

/// `details` out of `https://www.curseforge.com/wow/addons/details/files`,
/// with or without the scheme.
fn slug_from_page_url(text: &str) -> Option<String> {
    let url = reqwest::Url::parse(text)
        .or_else(|_| reqwest::Url::parse(&format!("https://{text}")))
        .ok()?;
    let host = url.host_str()?;
    if host != "curseforge.com" && !host.ends_with(".curseforge.com") {
        return None;
    }

    let mut segments = url.path_segments()?;
    if segments.next()? != "wow" || segments.next()? != "addons" {
        return None;
    }

    let slug = segments.next()?.trim();
    (!slug.is_empty()).then(|| slug.to_owned())
}

/// What CurseForge has for the browse box's text right now, in `sort` order.
pub(super) async fn browse(
    http: &HttpClient,
    api_key: &str,
    text: &str,
    sort: Sort,
    access: SearchAccess,
) -> Result<LiveResults> {
    match Query::parse(text) {
        Query::ProjectId(key) => browse_project(http, api_key, &key, access).await,
        Query::Search(term) => search_or_fall_back(http, api_key, &term, sort, access).await,
    }
}

async fn browse_project(
    http: &HttpClient,
    api_key: &str,
    key: &AddonKey,
    access: SearchAccess,
) -> Result<LiveResults> {
    let notice = |message: String| LiveResults {
        addons: Vec::new(),
        access,
        notice: Some(message),
    };

    let found = match fetch_mod(http, api_key, key).await {
        Ok(found) => found,
        Err(AppError::SourceStatus { status: 404, .. }) => {
            return Ok(notice(format!("No CurseForge project has the ID {key}.")));
        }
        Err(err) => return Err(err),
    };

    Ok(match to_summary(&found) {
        Some(summary) => LiveResults {
            addons: vec![summary],
            access,
            notice: None,
        },
        None => notice(format!(
            "CurseForge project {key} has no World of Warcraft: Forever file."
        )),
    })
}

/// Searches when the key may, and otherwise matches the featured listing
/// locally — saying so for a typed search, since that is a small fraction of
/// CurseForge. A blank browse on the featured listing needs no explanation.
async fn search_or_fall_back(
    http: &HttpClient,
    api_key: &str,
    term: &SearchTerm,
    sort: Sort,
    access: SearchAccess,
) -> Result<LiveResults> {
    if access != SearchAccess::Forbidden {
        match search(http, api_key, term, sort).await {
            Ok(addons) => {
                return Ok(LiveResults {
                    addons,
                    access: SearchAccess::Allowed,
                    notice: None,
                });
            }
            Err(AppError::SourceStatus { status: 403, .. }) => {}
            Err(err) => return Err(err),
        }
    }

    let addons = fetch_featured(http, api_key)
        .await?
        .into_iter()
        .filter(|summary| term.matches(summary))
        .collect();

    Ok(LiveResults {
        addons,
        access: SearchAccess::Forbidden,
        notice: (*term != SearchTerm::Everything).then(|| SEARCH_FORBIDDEN.to_owned()),
    })
}

/// The first page of matches in `sort` order. The order has to be the
/// server's: sorting one page locally would reorder the most popular fifty
/// rather than return the first fifty by name.
async fn search(
    http: &HttpClient,
    api_key: &str,
    term: &SearchTerm,
    sort: Sort,
) -> Result<Vec<AddonSummary>> {
    let url = search_url(term, sort)?;
    let json = http
        .get_json_authed(SOURCE, &url, &auth_headers(api_key))
        .await?;

    let response: DataMods = serde_json::from_value(json)
        .map_err(|err| missing_field(SOURCE, &url, "a mod search page", err.to_string()))?;
    Ok(response.data.iter().filter_map(to_summary).collect())
}

/// CurseForge's featured, popular, and recently updated Forever addons —
/// about twenty, and the only listing a key without search can reach.
async fn fetch_featured(http: &HttpClient, api_key: &str) -> Result<Vec<AddonSummary>> {
    let url = format!("{API}/mods/featured");
    let body = serde_json::json!({
        "gameId": GAME_WOW,
        "excludedModIds": [],
        "gameVersionTypeId": FOREVER_VERSION_TYPE,
    });
    let json = http
        .post_json_authed(SOURCE, &url, &auth_headers(api_key), &body)
        .await?;

    let response: DataFeatured = serde_json::from_value(json)
        .map_err(|err| missing_field(SOURCE, &url, "a featured listing", err.to_string()))?;
    let Featured {
        featured,
        popular,
        recently_updated,
    } = response.data;

    Ok(unique_addons(
        featured.iter().chain(&popular).chain(&recently_updated),
    ))
}

/// The featured lists overlap; each addon is kept once, in first-seen order.
fn unique_addons<'a>(mods: impl Iterator<Item = &'a Mod>) -> Vec<AddonSummary> {
    let mut seen = HashSet::new();
    mods.filter(|found| found.class_id.is_none_or(|class| class == CLASS_ADDONS))
        .filter(|found| seen.insert(found.id))
        .filter_map(to_summary)
        .collect()
}

/// One addon as CurseForge publishes it this moment.
pub(super) async fn lookup(
    http: &HttpClient,
    api_key: &str,
    key: &AddonKey,
) -> Result<AddonSummary> {
    let found = fetch_mod(http, api_key, key).await?;
    to_summary(&found).ok_or_else(|| AppError::NoForeverDownload {
        addon_id: AddonId::new(SOURCE, key.clone()),
    })
}

/// Many addons in one request. Keys that are not CurseForge project IDs, and
/// projects with no Forever file, are left out.
pub(super) async fn lookup_many(
    http: &HttpClient,
    api_key: &str,
    keys: &[AddonKey],
) -> Result<Vec<AddonSummary>> {
    let ids = keys
        .iter()
        .filter_map(|key| key.as_str().parse::<u64>().ok())
        .collect::<Vec<_>>();
    if ids.is_empty() {
        return Ok(Vec::new());
    }

    let url = format!("{API}/mods");
    let body = serde_json::json!({ "modIds": ids });
    let json = http
        .post_json_authed(SOURCE, &url, &auth_headers(api_key), &body)
        .await?;

    let response: DataMods = serde_json::from_value(json)
        .map_err(|err| missing_field(SOURCE, &url, "a list of mods", err.to_string()))?;
    Ok(response.data.iter().filter_map(to_summary).collect())
}

pub(super) async fn fetch_detail(
    http: &HttpClient,
    api_key: &str,
    key: &AddonKey,
) -> Result<AddonDetail> {
    let found = fetch_mod(http, api_key, key).await?;
    let summary = to_summary(&found).ok_or_else(|| AppError::NoForeverDownload {
        addon_id: AddonId::new(SOURCE, key.clone()),
    })?;

    let description_url = format!("{API}/mods/{key}/description");
    let json = http
        .get_json_authed(SOURCE, &description_url, &auth_headers(api_key))
        .await?;
    let description: DataString = serde_json::from_value(json).map_err(|err| {
        missing_field(
            SOURCE,
            &description_url,
            "a description string",
            err.to_string(),
        )
    })?;

    Ok(AddonDetail {
        description: Description::or_plain(
            Some(&description.data),
            Description::Html,
            &summary.summary,
        ),
        website_url: found.links.source_url.as_deref().and_then(non_empty),
        screenshots: found
            .screenshots
            .iter()
            .filter_map(|asset| asset.url.as_deref().and_then(non_empty))
            .map(|url| Screenshot { url })
            .collect(),
        summary,
    })
}

/// The summary comes from a lookup made moments before the install, so its
/// distribution check and file are current; only a brokered file needs the
/// mod again, to name the file the download-url endpoint should mint a link
/// for.
pub(super) async fn resolve_archive_url(
    http: &HttpClient,
    api_key: &str,
    summary: &AddonSummary,
) -> Result<String> {
    match &summary.download {
        Download::Direct { url } => return Ok(url.clone()),
        Download::External { .. } => {
            return Err(AppError::ExternalDownloadOnly {
                addon_id: summary.id.clone(),
            });
        }
        Download::Unsupported { format, .. } => {
            return Err(AppError::UnsupportedArchive {
                addon_id: summary.id.clone(),
                format: format.clone(),
            });
        }
        Download::Brokered => {}
    }

    let found = fetch_mod(http, api_key, &summary.id.key).await?;
    match classify(&found) {
        Download::Direct { url } => Ok(url),
        Download::External { .. } => Err(AppError::ExternalDownloadOnly {
            addon_id: summary.id.clone(),
        }),
        Download::Unsupported { format, .. } => Err(AppError::UnsupportedArchive {
            addon_id: summary.id.clone(),
            format,
        }),
        // The index names a file that `latestFiles` no longer carries; the
        // download-url endpoint can still mint a link for it.
        Download::Brokered => {
            let Some(index) = forever_index(&found) else {
                return Err(AppError::NoForeverDownload {
                    addon_id: summary.id.clone(),
                });
            };

            let url = format!(
                "{API}/mods/{}/files/{}/download-url",
                summary.id.key, index.file_id
            );
            let json = http
                .get_json_authed(SOURCE, &url, &auth_headers(api_key))
                .await?;
            let link: DataString = serde_json::from_value(json)
                .map_err(|err| missing_field(SOURCE, &url, "a download URL", err.to_string()))?;

            non_empty(&link.data).ok_or_else(|| AppError::ExternalDownloadOnly {
                addon_id: summary.id.clone(),
            })
        }
    }
}

#[derive(Debug, serde::Deserialize)]
struct DataString {
    #[serde(default)]
    data: String,
}

#[derive(Debug, serde::Deserialize)]
struct DataMod {
    data: Mod,
}

async fn fetch_mod(http: &HttpClient, api_key: &str, key: &AddonKey) -> Result<Mod> {
    let url = format!("{API}/mods/{key}");
    let json = http
        .get_json_authed(SOURCE, &url, &auth_headers(api_key))
        .await?;

    let found: DataMod = serde_json::from_value(json)
        .map_err(|err| missing_field(SOURCE, &url, "a mod", err.to_string()))?;
    Ok(found.data)
}

fn auth_headers(api_key: &str) -> [(&'static str, &str); 2] {
    [("x-api-key", api_key), ("accept", "application/json")]
}

fn search_url(term: &SearchTerm, sort: Sort) -> Result<String> {
    let base = format!("{API}/mods/search");
    let mut params = vec![
        ("gameId", GAME_WOW.to_string()),
        ("classId", CLASS_ADDONS.to_string()),
        ("gameVersionTypeId", FOREVER_VERSION_TYPE.to_string()),
        ("sortField", sort_field(sort.field).to_string()),
        ("sortOrder", sort_order(sort.direction).to_owned()),
        ("pageSize", SEARCH_PAGE_SIZE.to_string()),
        ("index", "0".to_owned()),
    ];
    match term {
        SearchTerm::Everything => {}
        SearchTerm::Text(text) => params.push(("searchFilter", text.clone())),
        SearchTerm::Slug(slug) => params.push(("slug", slug.clone())),
    }

    reqwest::Url::parse_with_params(&base, &params)
        .map(String::from)
        .map_err(|err| AppError::SourceRequest {
            source_id: SOURCE,
            url: base.clone(),
            reason: err.to_string(),
        })
}

/// CurseForge's `ModsSearchSortField` number for a browse order. Downloads is
/// `TotalDownloads`, not `Popularity`: the merged list orders by the download
/// count every source reports, and the server page has to agree with it.
fn sort_field(field: SortField) -> u8 {
    match field {
        SortField::Updated => 3,
        SortField::Name => 4,
        SortField::Author => 5,
        SortField::Downloads => 6,
    }
}

fn sort_order(direction: SortDirection) -> &'static str {
    match direction {
        SortDirection::Ascending => "asc",
        SortDirection::Descending => "desc",
    }
}

/// `None` for mods this application cannot key on. A mod with no Forever file
/// index is kept out too: the version filter on search and featured should
/// prevent that, a direct lookup does not, and a filter is a weaker statement
/// than a published file.
fn to_summary(found: &Mod) -> Option<AddonSummary> {
    let index = forever_index(found)?;
    let id = addon_id(SOURCE, &found.id.to_string())?;
    let file = found
        .latest_files
        .iter()
        .find(|file| file.id == index.file_id);

    Some(AddonSummary {
        name: found.name.trim().to_owned(),
        summary: found.summary.trim().to_owned(),
        author: found
            .authors
            .first()
            .and_then(|author| non_empty(&author.name)),
        // CurseForge exposes no clean addon version, only file display names
        // that mix versions with release titles; the modification time is the
        // honest freshness signal.
        version: None,
        updated_at: file
            .and_then(|file| non_empty(&file.file_date))
            .or_else(|| non_empty(&found.date_modified)),
        icon_url: found.logo.as_ref().and_then(|logo| {
            let thumbnail = logo.thumbnail_url.as_deref().and_then(non_empty);
            thumbnail.or_else(|| logo.url.as_deref().and_then(non_empty))
        }),
        page_url: page_url(found),
        categories: found
            .categories
            .iter()
            .filter_map(|category| non_empty(&category.name))
            .collect(),
        downloads: Some(found.download_count),
        expansions: vec![Expansion::Forever],
        download: classify(found),
        id,
    })
}

/// The mod's newest Forever file, preferring a stable release over a beta or
/// alpha the way CurseForge's own install button does.
fn forever_index(found: &Mod) -> Option<&FileIndex> {
    let mut indexes = found
        .latest_files_indexes
        .iter()
        .filter(|index| index.game_version_type_id == Some(FOREVER_VERSION_TYPE));

    let first = indexes.next()?;
    if first.release_type == RELEASE {
        return Some(first);
    }

    indexes
        .find(|index| index.release_type == RELEASE)
        .or(Some(first))
}

fn classify(found: &Mod) -> Download {
    if found.allow_mod_distribution == Some(false) {
        return Download::External {
            url: page_url(found),
        };
    }

    let Some(index) = forever_index(found) else {
        return Download::External {
            url: page_url(found),
        };
    };

    match found
        .latest_files
        .iter()
        .find(|file| file.id == index.file_id)
    {
        Some(file) => match &file.download_url {
            Some(url) if !url.trim().is_empty() => Download::Direct { url: url.clone() },
            // Distribution allowed but no URL published: mint one at install.
            _ => Download::Brokered,
        },
        None => Download::Brokered,
    }
}

fn page_url(found: &Mod) -> String {
    found
        .links
        .website_url
        .as_deref()
        .and_then(non_empty)
        .unwrap_or_else(|| format!("https://www.curseforge.com/wow/addons/{}", found.slug))
}

fn non_empty(raw: &str) -> Option<String> {
    let trimmed = raw.trim();
    (!trimmed.is_empty()).then(|| trimmed.to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    const QUESTIE: &str = r#"{
        "id": 1032100,
        "name": "Questie Forever",
        "slug": "questie-forever",
        "summary": "Quest objectives on the map, for WoW Forever.",
        "links": {
            "websiteUrl": "https://www.curseforge.com/wow/addons/questie-forever",
            "sourceUrl": "https://github.com/Questie/Questie"
        },
        "downloadCount": 152340,
        "dateModified": "2026-09-19T10:02:11.123Z",
        "allowModDistribution": true,
        "logo": { "thumbnailUrl": "https://media.forgecdn.net/avatars/thumb.png", "url": "" },
        "screenshots": [{ "thumbnailUrl": "", "url": "https://media.forgecdn.net/shot.png" }],
        "categories": [{ "name": "Quests & Leveling" }],
        "authors": [{ "name": "Aero" }],
        "latestFiles": [
            {
                "id": 7001,
                "fileDate": "2026-09-18T20:00:00.000Z",
                "downloadUrl": "https://edge.forgecdn.net/files/7001/questie-forever.zip"
            },
            { "id": 7002, "fileDate": "2026-09-10T20:00:00.000Z", "downloadUrl": null }
        ],
        "latestFilesIndexes": [
            { "fileId": 7002, "releaseType": 2, "gameVersionTypeId": 88568 },
            { "fileId": 7001, "releaseType": 1, "gameVersionTypeId": 88568 },
            { "fileId": 6900, "releaseType": 1, "gameVersionTypeId": 517 }
        ]
    }"#;

    fn questie() -> Mod {
        serde_json::from_str(QUESTIE).expect("fixture parses")
    }

    #[test]
    fn normalizes_a_mod_into_a_catalog_summary() {
        let summary = to_summary(&questie()).expect("mod has a forever file");

        assert_eq!(summary.id.to_string(), "curseforge:1032100");
        assert_eq!(summary.name, "Questie Forever");
        assert_eq!(summary.author.as_deref(), Some("Aero"));
        assert_eq!(summary.downloads, Some(152_340));
        assert_eq!(summary.categories, vec!["Quests & Leveling"]);
        assert_eq!(summary.expansions, vec![Expansion::Forever]);
        assert_eq!(
            summary.icon_url.as_deref(),
            Some("https://media.forgecdn.net/avatars/thumb.png")
        );
        assert_eq!(
            summary.download,
            Download::Direct {
                url: "https://edge.forgecdn.net/files/7001/questie-forever.zip".to_owned()
            }
        );
    }

    #[test]
    fn reads_a_mod_whose_unset_links_are_explicit_nulls() {
        let raw = QUESTIE.replace(
            r#""sourceUrl": "https://github.com/Questie/Questie""#,
            r#""sourceUrl": null, "wikiUrl": null"#,
        );
        let found: Mod = serde_json::from_str(&raw).expect("null links parse");

        assert_eq!(found.links.source_url, None);
        assert!(to_summary(&found).is_some());
    }

    #[test]
    fn prefers_the_stable_forever_release_over_a_newer_beta() {
        let summary = to_summary(&questie()).expect("mod has a forever file");

        // File 7002 is the newer beta; 7001 is the stable release.
        assert_eq!(
            summary.updated_at.as_deref(),
            Some("2026-09-18T20:00:00.000Z")
        );
    }

    #[test]
    fn skips_a_mod_with_no_forever_file_at_all() {
        let mut found = questie();
        found
            .latest_files_indexes
            .retain(|index| index.game_version_type_id != Some(FOREVER_VERSION_TYPE));

        assert!(to_summary(&found).is_none());
    }

    #[test]
    fn keeps_a_distribution_blocked_mod_in_the_catalog_as_external() {
        let mut found = questie();
        found.allow_mod_distribution = Some(false);

        let summary = to_summary(&found).expect("mod stays in the catalog");

        assert!(!summary.is_installable());
        assert_eq!(
            summary.download,
            Download::External {
                url: "https://www.curseforge.com/wow/addons/questie-forever".to_owned()
            }
        );
    }

    #[test]
    fn brokers_a_file_the_latest_files_list_no_longer_carries() {
        let mut found = questie();
        found.latest_files.clear();

        let summary = to_summary(&found).expect("mod stays in the catalog");
        assert_eq!(summary.download, Download::Brokered);
        assert!(summary.is_installable());
    }

    #[test]
    fn falls_back_to_the_only_forever_file_when_no_stable_release_exists() {
        let mut found = questie();
        found
            .latest_files_indexes
            .retain(|index| index.file_id != 7001);

        let index = forever_index(&found).expect("the beta remains");
        assert_eq!(index.file_id, 7002);
    }

    #[test]
    fn builds_a_page_url_from_the_slug_when_the_api_omits_the_link() {
        let mut found = questie();
        found.links.website_url = None;

        assert_eq!(
            page_url(&found),
            "https://www.curseforge.com/wow/addons/questie-forever"
        );
    }

    #[test]
    fn reads_the_browse_box_as_everything_project_id_url_or_text() {
        let key = |raw: &str| AddonKey::new(raw).expect("non-empty");
        let cases = [
            ("", Query::Search(SearchTerm::Everything)),
            ("   ", Query::Search(SearchTerm::Everything)),
            (" 3358 ", Query::ProjectId(key("3358"))),
            (
                "https://www.curseforge.com/wow/addons/details/files",
                Query::Search(SearchTerm::Slug("details".to_owned())),
            ),
            (
                "curseforge.com/wow/addons/questie-forever",
                Query::Search(SearchTerm::Slug("questie-forever".to_owned())),
            ),
            (
                "boss mods",
                Query::Search(SearchTerm::Text("boss mods".to_owned())),
            ),
            (
                "https://example.com/wow/addons/details",
                Query::Search(SearchTerm::Text(
                    "https://example.com/wow/addons/details".to_owned(),
                )),
            ),
            (
                "https://www.curseforge.com/minecraft/mc-mods/jei",
                Query::Search(SearchTerm::Text(
                    "https://www.curseforge.com/minecraft/mc-mods/jei".to_owned(),
                )),
            ),
        ];

        for (text, expected) in cases {
            assert_eq!(Query::parse(text), expected, "{text:?}");
        }
    }

    #[test]
    fn matches_a_slug_against_the_page_url_when_search_is_unavailable() {
        let summary = to_summary(&questie()).expect("mod has a forever file");

        assert!(SearchTerm::Slug("questie-forever".to_owned()).matches(&summary));
        assert!(!SearchTerm::Slug("forever".to_owned()).matches(&summary));
        assert!(SearchTerm::Text("quest map".to_owned()).matches(&summary));
    }

    #[test]
    fn keeps_each_featured_addon_once_and_skips_other_classes() {
        let mut other_class = questie();
        other_class.id = 99;
        other_class.class_id = Some(6);

        let lists = [questie(), questie(), other_class];
        let addons = unique_addons(lists.iter());

        assert_eq!(addons.len(), 1);
        assert_eq!(addons[0].id.to_string(), "curseforge:1032100");
    }

    #[test]
    fn searches_forever_addons_with_the_term_encoded() {
        let text = SearchTerm::Text("boss & mods".to_owned());
        let url = search_url(&text, Sort::default()).expect("url builds");

        assert!(url.contains("gameVersionTypeId=88568"), "{url}");
        assert!(url.contains("classId=1"), "{url}");
        assert!(url.contains("searchFilter=boss+%26+mods"), "{url}");

        let slug = SearchTerm::Slug("details".to_owned());
        let url = search_url(&slug, Sort::default()).expect("url builds");
        assert!(url.ends_with("&slug=details"), "{url}");
    }

    #[test]
    fn searches_everything_without_a_filter() {
        let url = search_url(&SearchTerm::Everything, Sort::default()).expect("url builds");

        assert!(!url.contains("searchFilter"), "{url}");
        assert!(!url.contains("slug="), "{url}");
    }

    #[test]
    fn asks_the_server_for_the_chosen_order() {
        use SortDirection::{Ascending, Descending};

        let cases = [
            (
                SortField::Downloads,
                Descending,
                "sortField=6&sortOrder=desc",
            ),
            (SortField::Updated, Descending, "sortField=3&sortOrder=desc"),
            (SortField::Name, Ascending, "sortField=4&sortOrder=asc"),
            (SortField::Author, Ascending, "sortField=5&sortOrder=asc"),
        ];

        for (field, direction, expected) in cases {
            let sort = Sort { field, direction };
            let url = search_url(&SearchTerm::Everything, sort).expect("url builds");
            assert!(url.contains(expected), "{field:?} {direction:?}: {url}");
        }
    }
}
