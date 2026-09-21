//! Wago Addons — <https://addons.wago.io/>, queried for World of Warcraft:
//! Forever addons.
//!
//! Wago's external API (the one WowUp and instawow use) authenticates with a
//! personal access token that any Wago account can generate in its settings —
//! self-serve, unlike CurseForge's reviewed application. The API has no
//! "list everything" endpoint, so the catalog is the `popular` listing for the
//! Forever game version.
//!
//! Download links expire (`logical_timestamp`), so downloads are always
//! brokered: the addon is re-fetched at install time for a fresh link.

use std::collections::BTreeMap;

use crate::domain::{AddonDetail, AddonSummary, Download, Expansion, Screenshot, SourceId};
use crate::error::{AppError, Result};
use crate::source::http::HttpClient;
use crate::source::{FetchProgress, addon_id, html_to_text, missing_field};

const SOURCE: SourceId = SourceId::Wago;
const API: &str = "https://addons.wago.io/api/external";
/// Wago's `game_version` label for World of Warcraft: Forever.
const GAME_VERSION: &str = "forever";

/// One row of the `popular` or `_search` listings.
///
/// A DTO rather than the domain type: field names and the per-stability
/// release map are Wago's conventions, and they stop here. Fields default so
/// one sparse row does not sink the listing.
#[derive(Debug, Default, serde::Deserialize)]
#[serde(default)]
struct ListedAddon {
    id: String,
    display_name: String,
    summary: String,
    thumbnail_image: Option<String>,
    authors: Vec<String>,
    download_count: u64,
    website_url: String,
    releases: BTreeMap<String, Release>,
}

/// The `/addons/{id}` detail document.
#[derive(Debug, Default, serde::Deserialize)]
#[serde(default)]
struct AddonDocument {
    description: String,
    /// The author's own site, distinct from `website_url` (the Wago page).
    website: String,
    gallery: Vec<String>,
    /// The detail route spells the release map `recent_release`; other routes
    /// use `recent_releases`. Both are accepted and merged.
    recent_release: BTreeMap<String, Release>,
    recent_releases: BTreeMap<String, Release>,
}

impl AddonDocument {
    fn releases(&self) -> &BTreeMap<String, Release> {
        match self.recent_release.is_empty() {
            true => &self.recent_releases,
            false => &self.recent_release,
        }
    }
}

#[derive(Debug, Default, Clone, serde::Deserialize)]
#[serde(default)]
struct Release {
    /// The version, as the author labelled it.
    label: String,
    /// ISO date-time.
    created_at: String,
    download_link: String,
}

#[derive(Debug, serde::Deserialize)]
struct Listing {
    #[serde(default)]
    data: Vec<ListedAddon>,
}

pub(super) async fn fetch_catalog(
    http: &HttpClient,
    token: &str,
    on_progress: &(dyn Fn(FetchProgress) + Send + Sync),
) -> Result<Vec<AddonSummary>> {
    let url = format!("{API}/addons/popular?game_version={GAME_VERSION}");
    let bearer = format!("Bearer {token}");
    let json = http
        .get_json_authed(SOURCE, &url, &auth_headers(&bearer))
        .await?;

    let listing: Listing = serde_json::from_value(json)
        .map_err(|err| missing_field(SOURCE, &url, "a popular-addons listing", err.to_string()))?;

    let addons = listing
        .data
        .iter()
        .filter_map(to_summary)
        .collect::<Vec<_>>();

    on_progress(FetchProgress {
        source: SOURCE,
        page: 1,
        total_pages: Some(1),
        addons_so_far: addons.len(),
    });

    Ok(addons)
}

pub(super) async fn fetch_detail(
    http: &HttpClient,
    token: &str,
    summary: &AddonSummary,
) -> Result<AddonDetail> {
    let document = fetch_document(http, token, summary).await?;

    Ok(AddonDetail {
        description: html_to_text(&document.description)
            .filter(|text| !text.is_empty())
            .unwrap_or_else(|| summary.summary.clone()),
        website_url: non_empty(&document.website),
        screenshots: document
            .gallery
            .iter()
            .filter_map(|url| non_empty(url))
            .map(|url| Screenshot { url })
            .collect(),
        summary: summary.clone(),
    })
}

/// Wago signs download links with an expiry, so the link is minted fresh here
/// rather than trusted from a possibly hours-old catalog.
pub(super) async fn resolve_archive_url(
    http: &HttpClient,
    token: &str,
    summary: &AddonSummary,
) -> Result<String> {
    let document = fetch_document(http, token, summary).await?;

    best_release(document.releases())
        .and_then(|release| non_empty(&release.download_link))
        .ok_or_else(|| AppError::NoForeverDownload {
            addon_id: summary.id.clone(),
        })
}

async fn fetch_document(
    http: &HttpClient,
    token: &str,
    summary: &AddonSummary,
) -> Result<AddonDocument> {
    let url = format!(
        "{API}/addons/{}?game_version={GAME_VERSION}",
        summary.id.key
    );
    let bearer = format!("Bearer {token}");
    let json = http
        .get_json_authed(SOURCE, &url, &auth_headers(&bearer))
        .await?;

    serde_json::from_value(json)
        .map_err(|err| missing_field(SOURCE, &url, "an addon document", err.to_string()))
}

fn auth_headers(bearer: &str) -> [(&'static str, &str); 2] {
    [("authorization", bearer), ("accept", "application/json")]
}

/// `None` for rows this application cannot key on or that list no release for
/// the Forever game version.
fn to_summary(listed: &ListedAddon) -> Option<AddonSummary> {
    let id = addon_id(SOURCE, &listed.id)?;
    let release = best_release(&listed.releases)?;

    Some(AddonSummary {
        name: listed.display_name.trim().to_owned(),
        summary: listed.summary.trim().to_owned(),
        author: listed.authors.first().and_then(|author| non_empty(author)),
        version: crate::domain::AddonVersion::new(&release.label),
        updated_at: non_empty(&release.created_at),
        icon_url: listed.thumbnail_image.as_deref().and_then(non_empty),
        page_url: non_empty(&listed.website_url)
            .unwrap_or_else(|| format!("https://addons.wago.io/addons/{}", listed.id)),
        categories: Vec::new(),
        downloads: Some(listed.download_count),
        expansions: vec![Expansion::Forever],
        download: Download::Brokered,
        id,
    })
}

/// The stable release when there is one, else the newest of what exists —
/// beta-era Forever addons often publish only alphas.
fn best_release(releases: &BTreeMap<String, Release>) -> Option<&Release> {
    releases.get("stable").or_else(|| {
        releases
            .values()
            .max_by(|left, right| left.created_at.cmp(&right.created_at))
    })
}

fn non_empty(raw: &str) -> Option<String> {
    let trimmed = raw.trim();
    (!trimmed.is_empty()).then(|| trimmed.to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    const FOREVERAURAS: &str = r#"{
        "id": "qN5mGYzr",
        "display_name": "ForeverAuras",
        "summary": "A WeakAuras replacement for WoW Forever.",
        "thumbnail_image": "https://media.wago.io/thumb/foreveraura.png",
        "authors": ["Stanzilla"],
        "download_count": 48211,
        "website_url": "https://addons.wago.io/addons/foreverauras",
        "releases": {
            "stable": {
                "label": "1.2.0",
                "created_at": "2026-09-17T08:00:00Z",
                "download_link": "https://addons.wago.io/api/external/addons/qN5mGYzr/download/stable?token=abc",
                "logical_timestamp": 1789999999
            },
            "beta": {
                "label": "1.3.0-beta1",
                "created_at": "2026-09-19T08:00:00Z",
                "download_link": "https://addons.wago.io/api/external/addons/qN5mGYzr/download/beta?token=def",
                "logical_timestamp": 1789999999
            }
        }
    }"#;

    fn foreveraura() -> ListedAddon {
        serde_json::from_str(FOREVERAURAS).expect("fixture parses")
    }

    #[test]
    fn normalizes_a_listing_row_into_a_catalog_summary() {
        let summary = to_summary(&foreveraura()).expect("row has a release");

        assert_eq!(summary.id.to_string(), "wago:qN5mGYzr");
        assert_eq!(summary.name, "ForeverAuras");
        assert_eq!(summary.author.as_deref(), Some("Stanzilla"));
        assert_eq!(summary.downloads, Some(48_211));
        assert_eq!(summary.expansions, vec![Expansion::Forever]);
        assert_eq!(summary.download, Download::Brokered);
    }

    #[test]
    fn prefers_the_stable_release_over_a_newer_beta() {
        let summary = to_summary(&foreveraura()).expect("row has a release");

        assert_eq!(
            summary.version.as_ref().map(|version| version.as_str()),
            Some("1.2.0")
        );
        assert_eq!(summary.updated_at.as_deref(), Some("2026-09-17T08:00:00Z"));
    }

    #[test]
    fn falls_back_to_the_newest_prerelease_when_nothing_is_stable() {
        let mut listed = foreveraura();
        listed.releases.remove("stable");

        let release = best_release(&listed.releases).expect("the beta remains");
        assert_eq!(release.label, "1.3.0-beta1");
    }

    #[test]
    fn skips_a_row_that_lists_no_release_at_all() {
        let mut listed = foreveraura();
        listed.releases.clear();

        assert!(to_summary(&listed).is_none());
    }

    #[test]
    fn reads_the_release_map_under_either_spelling_of_the_detail_field() {
        let with_typo: AddonDocument = serde_json::from_str(
            r#"{ "recent_release": { "stable": { "label": "1.0", "created_at": "2026-09-01T00:00:00Z", "download_link": "https://x/1.zip" } } }"#,
        )
        .expect("fixture parses");
        let without: AddonDocument = serde_json::from_str(
            r#"{ "recent_releases": { "stable": { "label": "2.0", "created_at": "2026-09-01T00:00:00Z", "download_link": "https://x/2.zip" } } }"#,
        )
        .expect("fixture parses");

        assert_eq!(
            best_release(with_typo.releases()).map(|release| release.label.as_str()),
            Some("1.0")
        );
        assert_eq!(
            best_release(without.releases()).map(|release| release.label.as_str()),
            Some("2.0")
        );
    }
}
