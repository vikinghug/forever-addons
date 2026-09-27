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

use std::borrow::Cow;
use std::collections::HashSet;

use percent_encoding::percent_decode_str;

use crate::domain::{
    AddonDetail, AddonFolder, AddonId, AddonKey, AddonSummary, Channel, Dependency, Download,
    Expansion, FileHistory, FileKey, Markup, PublishedFile, RelatedAddon, Relation, SafeHtml,
    Screenshot, Sort, SortDirection, SortField, SourceId, mentioned_addons,
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
/// The API's largest page. Files come newest first, so one page is the
/// recent history, which is all the Files tab shows.
const FILES_PAGE_SIZE: u32 = 50;
/// The base for relative links in a changelog, which has no addon page.
const ADDONS_PAGE: &str = "https://www.curseforge.com/wow/addons/";
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
    /// Often just the uploaded filename.
    title: Option<String>,
    /// HTML.
    description: Option<String>,
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
    display_name: String,
    file_name: String,
    release_type: u8,
    file_date: String,
    file_length: Option<u64>,
    download_count: Option<u64>,
    download_url: Option<String>,
    dependencies: Vec<FileDependency>,
    modules: Vec<Module>,
}

#[derive(Debug, Default, serde::Deserialize)]
#[serde(rename_all = "camelCase", default)]
struct FileDependency {
    mod_id: u64,
    relation_type: u8,
}

/// One top-level folder of a file's archive.
#[derive(Debug, Default, serde::Deserialize)]
#[serde(default)]
struct Module {
    name: String,
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

    let markup = Markup::new(&summary, unwrap_linkout);
    Ok(AddonDetail {
        description: markup.html(&description.data),
        website_url: found.links.source_url.as_deref().and_then(non_empty),
        screenshots: found
            .screenshots
            .iter()
            .filter_map(|asset| to_screenshot(asset, &markup))
            .collect(),
        summary,
    })
}

fn to_screenshot(asset: &Asset, markup: &Markup<'_>) -> Option<Screenshot> {
    let url = asset.url.as_deref().and_then(non_empty)?;
    Some(Screenshot {
        url,
        thumbnail_url: asset.thumbnail_url.as_deref().and_then(non_empty),
        title: asset.title.as_deref().and_then(caption_title),
        description: asset
            .description
            .as_deref()
            .and_then(|html| markup.text(html)),
    })
}

/// A screenshot title, unless it is only the uploaded filename.
fn caption_title(raw: &str) -> Option<String> {
    let title = non_empty(raw)?;
    let lower = title.to_ascii_lowercase();
    let is_filename = [".png", ".jpg", ".jpeg", ".gif", ".webp", ".bmp"]
        .iter()
        .any(|extension| lower.ends_with(extension));
    (!is_filename).then_some(title)
}

/// Every Forever file of an addon, with the addons they depend on looked up
/// in one more request.
pub(super) async fn fetch_files(
    http: &HttpClient,
    api_key: &str,
    key: &AddonKey,
    installed_at: Option<&str>,
) -> Result<FileHistory> {
    let url = format!(
        "{API}/mods/{key}/files?gameVersionTypeId={FOREVER_VERSION_TYPE}&pageSize={FILES_PAGE_SIZE}&index=0"
    );
    let json = http
        .get_json_authed(SOURCE, &url, &auth_headers(api_key))
        .await?;
    let response: DataFiles = serde_json::from_value(json)
        .map_err(|err| missing_field(SOURCE, &url, "a list of files", err.to_string()))?;

    let files = response
        .data
        .iter()
        .filter_map(to_published_file)
        .collect::<Vec<_>>();
    let related = lookup_related(http, api_key, &mentioned_addons(&files)).await?;
    Ok(FileHistory::new(files, installed_at, related))
}

/// One file's changelog, sanitized like a description.
pub(super) async fn fetch_changelog(
    http: &HttpClient,
    api_key: &str,
    key: &AddonKey,
    file: &FileKey,
) -> Result<SafeHtml> {
    let url = format!("{API}/mods/{key}/files/{file}/changelog");
    let json = http
        .get_json_authed(SOURCE, &url, &auth_headers(api_key))
        .await?;
    let changelog: DataString = serde_json::from_value(json)
        .map_err(|err| missing_field(SOURCE, &url, "a changelog string", err.to_string()))?;

    Ok(Markup::for_page(ADDONS_PAGE, unwrap_linkout).html(&changelog.data))
}

/// The addons files depend on, in `ids` order. Unlike [`lookup_many`], a mod
/// with no Forever file is kept — a library that only ships embedded is
/// still worth naming — and a mod CurseForge leaves out is kept as unlisted.
async fn lookup_related(
    http: &HttpClient,
    api_key: &str,
    ids: &[AddonId],
) -> Result<Vec<RelatedAddon>> {
    let mod_ids = ids
        .iter()
        .filter_map(|id| id.key.as_str().parse::<u64>().ok())
        .collect::<Vec<_>>();
    if mod_ids.is_empty() {
        return Ok(Vec::new());
    }

    let url = format!("{API}/mods");
    let body = serde_json::json!({ "modIds": mod_ids });
    let json = http
        .post_json_authed(SOURCE, &url, &auth_headers(api_key), &body)
        .await?;
    let response: DataMods = serde_json::from_value(json)
        .map_err(|err| missing_field(SOURCE, &url, "a list of mods", err.to_string()))?;

    Ok(ids
        .iter()
        .map(|id| related_addon(id, &response.data))
        .collect())
}

fn related_addon(id: &AddonId, found: &[Mod]) -> RelatedAddon {
    let Some(found) = found
        .iter()
        .find(|found| found.id.to_string() == id.key.as_str())
    else {
        return RelatedAddon::Unlisted { id: id.clone() };
    };

    match to_summary(found) {
        Some(summary) => RelatedAddon::Listed(Box::new(summary)),
        None => RelatedAddon::NoForeverFile {
            id: id.clone(),
            name: found.name.trim().to_owned(),
            page_url: page_url(found),
        },
    }
}

/// `None` only for a file with no id to key it by.
fn to_published_file(file: &File) -> Option<PublishedFile> {
    Some(PublishedFile {
        id: FileKey::new(file.id.to_string()).filter(|_| file.id != 0)?,
        name: non_empty(&file.display_name).unwrap_or_else(|| file.file_name.clone()),
        file_name: file.file_name.clone(),
        channel: channel(file.release_type),
        published_at: file.file_date.clone(),
        size: file.file_length,
        downloads: file.download_count,
        // A module name that could not be a folder is no folder this file
        // installs; the archive planner refuses such paths too.
        folders: file
            .modules
            .iter()
            .filter_map(|module| AddonFolder::new(module.name.as_str()).ok())
            .collect(),
        dependencies: file
            .dependencies
            .iter()
            .filter_map(|dependency| {
                Some(Dependency {
                    addon: addon_id(SOURCE, &dependency.mod_id.to_string())?,
                    relation: relation(dependency.relation_type),
                })
            })
            .collect(),
    })
}

fn channel(release_type: u8) -> Channel {
    match release_type {
        1 => Channel::Release,
        2 => Channel::Beta,
        3 => Channel::Alpha,
        code => Channel::Unknown { code },
    }
}

fn relation(relation_type: u8) -> Relation {
    match relation_type {
        1 => Relation::Embedded,
        2 => Relation::Optional,
        3 => Relation::Required,
        4 => Relation::Tool,
        5 => Relation::Incompatible,
        6 => Relation::Included,
        code => Relation::Unknown { code },
    }
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
    let Some(index) = forever_index(&found) else {
        return Err(AppError::NoForeverDownload {
            addon_id: summary.id.clone(),
        });
    };

    match classify(&found, index) {
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

#[derive(Debug, serde::Deserialize)]
struct DataFiles {
    data: Vec<File>,
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
        download: classify(found, index),
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

/// How the Forever file named by `index` can be fetched. A mod with no
/// Forever file never gets this far — it is no Forever addon at all — so
/// [`Download::External`] only ever means the author's opt-out.
fn classify(found: &Mod, index: &FileIndex) -> Download {
    if found.allow_mod_distribution == Some(false) {
        return Download::External {
            url: page_url(found),
        };
    }

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

/// The target of a CurseForge outbound link. Descriptions wrap them as
/// `/linkout?remoteUrl=…`, usually with the target percent-encoded twice.
fn unwrap_linkout(href: &str) -> Cow<'_, str> {
    let Some((_, query)) = href.split_once("/linkout?") else {
        return Cow::Borrowed(href);
    };
    let Some(encoded) = query
        .split('&')
        .find_map(|pair| pair.strip_prefix("remoteUrl="))
    else {
        return Cow::Borrowed(href);
    };

    let once = percent_decode_str(encoded).decode_utf8_lossy();
    if once.contains("://") {
        return Cow::Owned(once.into_owned());
    }
    Cow::Owned(percent_decode_str(&once).decode_utf8_lossy().into_owned())
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

    #[test]
    fn unwraps_linkout_redirects_to_their_target() {
        let cases = [
            (
                "/linkout?remoteUrl=https%253a%252f%252fcurseforge.overwolf.com%252f",
                "https://curseforge.overwolf.com/",
            ),
            (
                "https://www.curseforge.com/linkout?remoteUrl=https%3a%2f%2fdiscord.gg%2fquestie&x=1",
                "https://discord.gg/questie",
            ),
            ("/wow/addons/questie", "/wow/addons/questie"),
            ("/linkout?other=1", "/linkout?other=1"),
        ];

        for (href, expected) in cases {
            assert_eq!(unwrap_linkout(href), expected, "href: {href}");
        }
    }

    #[test]
    fn a_description_linkout_opens_its_decoded_target() {
        let found = questie();
        let summary = to_summary(&found).expect("a Forever mod");
        let html =
            r#"<a href="/linkout?remoteUrl=https%253a%252f%252fgithub.com%252fQuestie">GitHub</a>"#;

        assert_eq!(
            Markup::new(&summary, unwrap_linkout).html(html).as_str(),
            r#"<a href="https://github.com/Questie">GitHub</a>"#
        );
    }

    /// Two files as `/mods/{id}/files` returns them, trimmed.
    const DBM_DUNGEONS_FILES: &str = r#"{ "data": [
        {
            "id": 8945003,
            "displayName": "DBM-Dungeons-r264-8-g0cd2508",
            "fileName": "DBM-Dungeons-r264-8-g0cd2508.zip",
            "releaseType": 3,
            "fileDate": "2026-09-22T05:03:45.77Z",
            "fileLength": 1837914,
            "downloadCount": 5568,
            "dependencies": [
                { "modId": 3358, "relationType": 3 },
                { "modId": 14320, "relationType": 9 }
            ],
            "modules": [
                { "name": "DBM-Party-Forever", "fingerprint": 1 },
                { "name": "../escape", "fingerprint": 2 }
            ]
        },
        {
            "id": 8934525,
            "displayName": "",
            "fileName": "DBM-Dungeons-r264.zip",
            "releaseType": 1,
            "fileDate": "2026-09-18T10:00:00Z",
            "dependencies": [],
            "modules": []
        }
    ] }"#;

    fn dbm_files() -> Vec<PublishedFile> {
        let response: DataFiles = serde_json::from_str(DBM_DUNGEONS_FILES).expect("fixture parses");
        response.data.iter().filter_map(to_published_file).collect()
    }

    #[test]
    fn reads_a_listed_file_with_its_channel_folders_and_dependencies() {
        let file = &dbm_files()[0];

        assert_eq!(file.id.as_str(), "8945003");
        assert_eq!(file.channel, Channel::Alpha);
        assert_eq!(file.size, Some(1_837_914));
        assert_eq!(
            file.folders
                .iter()
                .map(AddonFolder::as_str)
                .collect::<Vec<_>>(),
            ["DBM-Party-Forever"]
        );
        assert_eq!(file.dependencies[0].addon.to_string(), "curseforge:3358");
        assert_eq!(file.dependencies[0].relation, Relation::Required);
    }

    #[test]
    fn preserves_an_unknown_relation_code() {
        assert_eq!(
            dbm_files()[0].dependencies[1].relation,
            Relation::Unknown { code: 9 }
        );
    }

    #[test]
    fn names_a_file_by_its_archive_when_the_display_name_is_blank() {
        assert_eq!(dbm_files()[1].name, "DBM-Dungeons-r264.zip");
    }

    #[test]
    fn keeps_a_related_mod_without_a_forever_file_by_name() {
        let mut library = questie();
        library.name = "LibSharedMedia-3.0".to_owned();
        library.latest_files_indexes.clear();
        let id = addon_id(SOURCE, "1032100").expect("valid key");

        assert_eq!(
            related_addon(&id, &[library]),
            RelatedAddon::NoForeverFile {
                id: id.clone(),
                name: "LibSharedMedia-3.0".to_owned(),
                page_url: "https://www.curseforge.com/wow/addons/questie-forever".to_owned(),
            }
        );
        assert!(matches!(
            related_addon(&id, &[questie()]),
            RelatedAddon::Listed(_)
        ));
    }

    #[test]
    fn keeps_a_related_mod_curseforge_left_out_as_unlisted() {
        let id = addon_id(SOURCE, "999").expect("valid key");

        assert_eq!(
            related_addon(&id, &[questie()]),
            RelatedAddon::Unlisted { id: id.clone() }
        );
    }

    #[test]
    fn captions_a_screenshot_but_not_with_its_filename() {
        let summary = to_summary(&questie()).expect("mod has a forever file");
        let markup = Markup::new(&summary, unwrap_linkout);
        let asset = Asset {
            thumbnail_url: Some("https://media.forgecdn.net/thumb.png".to_owned()),
            url: Some("https://media.forgecdn.net/full.png".to_owned()),
            title: Some("dbm-warnings1.PNG".to_owned()),
            description: Some("<p>Some raid warnings\n</p>".to_owned()),
        };

        let shot = to_screenshot(&asset, &markup).expect("has a url");
        assert_eq!(shot.title, None);
        assert_eq!(shot.description.as_deref(), Some("Some raid warnings"));
        assert_eq!(caption_title("Quest pins").as_deref(), Some("Quest pins"));
    }
}
