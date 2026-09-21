//! CurseForge — <https://www.curseforge.com/wow>, queried for World of
//! Warcraft: Forever addons.
//!
//! Forever addons are published on CurseForge rather than the private-server
//! sites, and CurseForge's site is Cloudflare-fronted, so this source talks to
//! the official Core API (<https://api.curseforge.com>) instead of scraping.
//! The API requires a per-user key (free, from console.curseforge.com), which
//! is why every function here takes one.
//!
//! Mod authors can opt out of API distribution; those mods stay in the catalog
//! as [`Download::External`] with their page reachable, rather than vanishing.

use crate::domain::{AddonDetail, AddonSummary, Download, Expansion, Screenshot, SourceId};
use crate::error::{AppError, Result};
use crate::source::http::HttpClient;
use crate::source::{FetchProgress, addon_id, html_to_text, missing_field};

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
const PAGE_SIZE: u32 = 50;
/// The API refuses `index + pageSize` beyond 10 000; everything reachable
/// below that is ~200 pages.
const MAX_PAGES: u32 = 10_000 / PAGE_SIZE;

/// The subset of a CurseForge mod this application reads.
///
/// A DTO rather than the domain type: the camelCase names and nullable fields
/// are the API's conventions, and they stop here. Fields default so one
/// sparsely-filled mod does not sink a whole catalog page.
#[derive(Debug, Default, serde::Deserialize)]
#[serde(rename_all = "camelCase", default)]
struct Mod {
    id: u64,
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

#[derive(Debug, Default, serde::Deserialize)]
#[serde(rename_all = "camelCase", default)]
struct Links {
    website_url: String,
    source_url: String,
}

#[derive(Debug, Default, serde::Deserialize)]
#[serde(rename_all = "camelCase", default)]
struct Asset {
    thumbnail_url: String,
    url: String,
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
struct SearchResponse {
    data: Vec<Mod>,
    pagination: Pagination,
}

#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct Pagination {
    total_count: u64,
}

pub(super) async fn fetch_catalog(
    http: &HttpClient,
    api_key: &str,
    on_progress: &(dyn Fn(FetchProgress) + Send + Sync),
) -> Result<Vec<AddonSummary>> {
    let mut addons = Vec::new();
    let mut page = 0;
    let mut total_pages = None;

    while page < MAX_PAGES {
        let url = search_url(page);
        let json = http
            .get_json_authed(SOURCE, &url, &auth_headers(api_key))
            .await?;

        let response: SearchResponse = serde_json::from_value(json)
            .map_err(|err| missing_field(SOURCE, &url, "a mod search page", err.to_string()))?;
        if response.data.is_empty() {
            break;
        }

        total_pages = total_pages.or(Some(pages_for(response.pagination.total_count)));
        addons.extend(response.data.iter().filter_map(to_summary));
        on_progress(FetchProgress {
            source: SOURCE,
            page: page + 1,
            total_pages,
            addons_so_far: addons.len(),
        });

        page += 1;
        if total_pages.is_some_and(|total| page >= total) {
            break;
        }
    }

    Ok(addons)
}

pub(super) async fn fetch_detail(
    http: &HttpClient,
    api_key: &str,
    summary: &AddonSummary,
) -> Result<AddonDetail> {
    let found = fetch_mod(http, api_key, summary).await?;

    let description_url = format!("{API}/mods/{}/description", summary.id.key);
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
        description: html_to_text(&description.data)
            .filter(|text| !text.is_empty())
            .unwrap_or_else(|| summary.summary.clone()),
        website_url: non_empty(&found.links.source_url),
        screenshots: found
            .screenshots
            .iter()
            .filter_map(|asset| non_empty(&asset.url))
            .map(|url| Screenshot { url })
            .collect(),
        summary: summary.clone(),
    })
}

/// The catalog's file list can be stale by install time, so the mod is
/// re-fetched and its current Forever file resolved fresh.
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

    let found = fetch_mod(http, api_key, summary).await?;
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

async fn fetch_mod(http: &HttpClient, api_key: &str, summary: &AddonSummary) -> Result<Mod> {
    let url = format!("{API}/mods/{}", summary.id.key);
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

fn search_url(page: u32) -> String {
    let index = page * PAGE_SIZE;
    format!(
        "{API}/mods/search?gameId={GAME_WOW}&classId={CLASS_ADDONS}\
         &gameVersionTypeId={FOREVER_VERSION_TYPE}\
         &sortField=2&sortOrder=desc&pageSize={PAGE_SIZE}&index={index}"
    )
}

fn pages_for(total_count: u64) -> u32 {
    let reachable = total_count.min(10_000) as u32;
    reachable.div_ceil(PAGE_SIZE).max(1)
}

/// `None` for mods this application cannot key on. A mod with no Forever file
/// index is kept out too: the search filter should prevent that, but a filter
/// is a weaker statement than a published file.
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
        icon_url: found
            .logo
            .as_ref()
            .and_then(|logo| non_empty(&logo.thumbnail_url).or_else(|| non_empty(&logo.url))),
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
    non_empty(&found.links.website_url)
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
        found.links.website_url = String::new();

        assert_eq!(
            page_url(&found),
            "https://www.curseforge.com/wow/addons/questie-forever"
        );
    }

    #[test]
    fn stays_within_the_apis_reachable_window() {
        assert_eq!(pages_for(898), 18);
        assert_eq!(pages_for(0), 1);
        assert_eq!(pages_for(1_000_000), 10_000 / PAGE_SIZE);
    }
}
