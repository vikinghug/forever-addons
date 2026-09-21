//! GitHub releases — the escape hatch for Forever addons that ship on GitHub
//! before (or instead of) the addon stores.
//!
//! Unlike the other sources there is no site-wide catalog: the user tracks
//! repositories by hand, and this source's "catalog" is the latest suitable
//! release of each tracked repository. GitHub's anonymous API allowance (60
//! requests/hour) is plenty for a handful of repositories refreshed on demand.
//!
//! Only uploaded `.zip` release assets are installed — packager-built addon
//! zips have correct folder layouts. A release with no such asset is kept as
//! an explicitly unsupported download rather than guessing at the source
//! archive, whose root folder is named after the commit and would never load.

use crate::domain::{AddonDetail, AddonSummary, Download, Expansion, Screenshot, SourceId};
use crate::error::{AppError, Result};
use crate::source::http::HttpClient;
use crate::source::{FetchProgress, addon_id, missing_field};

const SOURCE: SourceId = SourceId::GitHub;
const API: &str = "https://api.github.com";

/// A tracked repository, validated into `owner/name` form.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RepoSlug(String);

impl RepoSlug {
    /// Accepts `owner/name`, a full `https://github.com/owner/name` URL, or
    /// either with trailing paths (`/releases`, `.git`) pasted along.
    pub fn parse(raw: &str) -> Option<Self> {
        let trimmed = raw
            .trim()
            .strip_prefix("https://github.com/")
            .or_else(|| raw.trim().strip_prefix("github.com/"))
            .unwrap_or_else(|| raw.trim());

        let mut parts = trimmed.split('/').filter(|part| !part.is_empty());
        let owner = parts.next()?;
        let name = parts.next()?.trim_end_matches(".git");

        let valid = |part: &str| {
            !part.is_empty()
                && part
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
        };
        (valid(owner) && valid(name)).then(|| Self(format!("{owner}/{name}")))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// The subset of a repository document this application reads.
#[derive(Debug, Default, serde::Deserialize)]
#[serde(default)]
struct Repo {
    full_name: String,
    description: Option<String>,
    html_url: String,
    owner: Owner,
}

#[derive(Debug, Default, serde::Deserialize)]
#[serde(default)]
struct Owner {
    login: String,
    avatar_url: String,
}

#[derive(Debug, Default, serde::Deserialize)]
#[serde(default)]
struct Release {
    tag_name: String,
    name: Option<String>,
    prerelease: bool,
    draft: bool,
    published_at: String,
    html_url: String,
    body: Option<String>,
    assets: Vec<Asset>,
}

#[derive(Debug, Default, serde::Deserialize)]
#[serde(default)]
struct Asset {
    name: String,
    browser_download_url: String,
    download_count: u64,
}

/// The latest suitable release of every tracked repository.
pub(super) async fn fetch_catalog(
    http: &HttpClient,
    repos: &[String],
    on_progress: &(dyn Fn(FetchProgress) + Send + Sync),
) -> Result<Vec<AddonSummary>> {
    let total_pages = u32::try_from(repos.len()).unwrap_or(u32::MAX).max(1);
    let mut addons = Vec::new();

    for (index, raw) in repos.iter().enumerate() {
        let Some(slug) = RepoSlug::parse(raw) else {
            continue;
        };

        addons.push(fetch_repo_summary(http, &slug).await?);
        on_progress(FetchProgress {
            source: SOURCE,
            page: u32::try_from(index + 1).unwrap_or(u32::MAX),
            total_pages: Some(total_pages),
            addons_so_far: addons.len(),
        });
    }

    Ok(addons)
}

pub(super) async fn fetch_detail(http: &HttpClient, summary: &AddonSummary) -> Result<AddonDetail> {
    let Some(slug) = RepoSlug::parse(summary.id.key.as_str()) else {
        return Err(missing_field(
            SOURCE,
            summary.id.key.as_str(),
            "an owner/name repository key",
            "the tracked entry is not one",
        ));
    };

    let releases = fetch_releases(http, &slug).await?;
    let release = best_release(&releases);
    let description = release
        .and_then(|release| release.body.as_deref())
        .map(str::trim)
        .filter(|body| !body.is_empty())
        .map(str::to_owned)
        .unwrap_or_else(|| summary.summary.clone());

    Ok(AddonDetail {
        description,
        website_url: Some(format!("https://github.com/{}", slug.as_str())),
        screenshots: Vec::<Screenshot>::new(),
        summary: summary.clone(),
    })
}

/// Asset URLs are stable, so the catalog's direct URL is used as-is; this is
/// only consulted for entries that were cataloged without one.
pub(super) fn resolve_archive_url(summary: &AddonSummary) -> Result<String> {
    match &summary.download {
        Download::Direct { url } => Ok(url.clone()),
        Download::Unsupported { format, .. } => Err(AppError::UnsupportedArchive {
            addon_id: summary.id.clone(),
            format: format.clone(),
        }),
        Download::External { .. } => Err(AppError::ExternalDownloadOnly {
            addon_id: summary.id.clone(),
        }),
        Download::Brokered => Err(AppError::NoForeverDownload {
            addon_id: summary.id.clone(),
        }),
    }
}

/// Validates that a repository exists and is reachable before it is tracked.
pub(super) async fn fetch_repo_summary(http: &HttpClient, slug: &RepoSlug) -> Result<AddonSummary> {
    let repo_url = format!("{API}/repos/{}", slug.as_str());
    let json = http
        .get_json_authed(SOURCE, &repo_url, &accept_header())
        .await?;
    let repo: Repo = serde_json::from_value(json)
        .map_err(|err| missing_field(SOURCE, &repo_url, "a repository", err.to_string()))?;

    let releases = fetch_releases(http, slug).await?;
    Ok(to_summary(slug, &repo, best_release(&releases)))
}

async fn fetch_releases(http: &HttpClient, slug: &RepoSlug) -> Result<Vec<Release>> {
    let url = format!("{API}/repos/{}/releases?per_page=10", slug.as_str());
    let json = http.get_json_authed(SOURCE, &url, &accept_header()).await?;

    serde_json::from_value(json)
        .map_err(|err| missing_field(SOURCE, &url, "a release list", err.to_string()))
}

fn accept_header() -> [(&'static str, &'static str); 1] {
    [("accept", "application/vnd.github+json")]
}

fn to_summary(slug: &RepoSlug, repo: &Repo, release: Option<&Release>) -> AddonSummary {
    let id = addon_id(SOURCE, slug.as_str()).unwrap_or_else(|| {
        // A parsed RepoSlug is never empty; this arm is unreachable in
        // practice but cheaper to satisfy than to unwrap.
        crate::domain::AddonId::new(
            SOURCE,
            crate::domain::AddonKey::new("invalid/invalid").expect("literal is non-empty"),
        )
    });

    let name = repo
        .full_name
        .rsplit('/')
        .next()
        .filter(|name| !name.is_empty())
        .unwrap_or(slug.as_str())
        .to_owned();

    AddonSummary {
        name,
        summary: repo.description.clone().unwrap_or_default(),
        author: (!repo.owner.login.is_empty()).then(|| repo.owner.login.clone()),
        version: release.and_then(|release| crate::domain::AddonVersion::new(&release.tag_name)),
        updated_at: release
            .map(|release| release.published_at.clone())
            .filter(|date| !date.is_empty()),
        icon_url: (!repo.owner.avatar_url.is_empty()).then(|| repo.owner.avatar_url.clone()),
        page_url: match release {
            Some(release) if !release.html_url.is_empty() => release.html_url.clone(),
            _ => repo.html_url.clone(),
        },
        categories: Vec::new(),
        downloads: release.map(|release| {
            release
                .assets
                .iter()
                .map(|asset| asset.download_count)
                .sum()
        }),
        expansions: vec![Expansion::Forever],
        download: match release {
            Some(release) => classify(release, repo),
            None => Download::Unsupported {
                url: repo.html_url.clone(),
                format: "no release".to_owned(),
            },
        },
        id,
    }
}

/// The newest non-draft release, preferring full releases but accepting a
/// prerelease — beta-era Forever addons often publish nothing else.
fn best_release(releases: &[Release]) -> Option<&Release> {
    let published = releases.iter().filter(|release| !release.draft);
    published
        .clone()
        .find(|release| !release.prerelease)
        .or_else(|| published.clone().next())
}

/// The release's installable archive: a packager-style `.zip` asset, skipping
/// `nolib` variants when a full one exists.
fn classify(release: &Release, repo: &Repo) -> Download {
    let zips = release
        .assets
        .iter()
        .filter(|asset| asset.name.to_ascii_lowercase().ends_with(".zip"))
        .collect::<Vec<_>>();

    let chosen = zips
        .iter()
        .find(|asset| !asset.name.to_ascii_lowercase().contains("nolib"))
        .or_else(|| zips.first());

    match chosen {
        Some(asset) => Download::Direct {
            url: asset.browser_download_url.clone(),
        },
        None => Download::Unsupported {
            url: match release.html_url.is_empty() {
                true => repo.html_url.clone(),
                false => release.html_url.clone(),
            },
            format: "release without a zip asset".to_owned(),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn repo() -> Repo {
        serde_json::from_str(
            r#"{
                "full_name": "RevoltLive85/ForeverGuide",
                "description": "A leveling guide engine for WoW Forever.",
                "html_url": "https://github.com/RevoltLive85/ForeverGuide",
                "owner": { "login": "RevoltLive85", "avatar_url": "https://avatars.githubusercontent.com/u/1" }
            }"#,
        )
        .expect("fixture parses")
    }

    fn release(tag: &str, prerelease: bool, assets: &[&str]) -> Release {
        Release {
            tag_name: tag.to_owned(),
            name: Some(tag.to_owned()),
            prerelease,
            draft: false,
            published_at: "2026-09-18T12:00:00Z".to_owned(),
            html_url: format!("https://github.com/RevoltLive85/ForeverGuide/releases/{tag}"),
            body: Some("Changelog".to_owned()),
            assets: assets
                .iter()
                .map(|name| Asset {
                    name: (*name).to_owned(),
                    browser_download_url: format!("https://github.com/dl/{name}"),
                    download_count: 10,
                })
                .collect(),
        }
    }

    #[test]
    fn parses_a_slug_out_of_the_shapes_people_paste() {
        for raw in [
            "RevoltLive85/ForeverGuide",
            "https://github.com/RevoltLive85/ForeverGuide",
            "https://github.com/RevoltLive85/ForeverGuide.git",
            "https://github.com/RevoltLive85/ForeverGuide/releases/tag/v1.0",
            "  github.com/RevoltLive85/ForeverGuide  ",
        ] {
            assert_eq!(
                RepoSlug::parse(raw).map(|slug| slug.as_str().to_owned()),
                Some("RevoltLive85/ForeverGuide".to_owned()),
                "{raw}"
            );
        }
    }

    #[test]
    fn rejects_strings_that_are_not_a_repository() {
        for raw in ["", "just-an-owner", "https://gitlab.com/a/b", "a/b; rm -rf"] {
            assert_eq!(RepoSlug::parse(raw), None, "{raw:?}");
        }
    }

    #[test]
    fn normalizes_a_repo_and_release_into_a_catalog_summary() {
        let slug = RepoSlug::parse("RevoltLive85/ForeverGuide").expect("valid");
        let releases = [release("v1.4.0", false, &["ForeverGuide-v1.4.0.zip"])];

        let summary = to_summary(&slug, &repo(), best_release(&releases));

        assert_eq!(summary.id.to_string(), "github:RevoltLive85/ForeverGuide");
        assert_eq!(summary.name, "ForeverGuide");
        assert_eq!(summary.author.as_deref(), Some("RevoltLive85"));
        assert_eq!(
            summary.version.as_ref().map(|version| version.as_str()),
            Some("v1.4.0")
        );
        assert_eq!(
            summary.download,
            Download::Direct {
                url: "https://github.com/dl/ForeverGuide-v1.4.0.zip".to_owned()
            }
        );
    }

    #[test]
    fn prefers_a_full_release_over_a_newer_prerelease() {
        let releases = [
            release("v1.5.0-beta", true, &["ForeverGuide-v1.5.0-beta.zip"]),
            release("v1.4.0", false, &["ForeverGuide-v1.4.0.zip"]),
        ];

        assert_eq!(
            best_release(&releases).map(|release| release.tag_name.as_str()),
            Some("v1.4.0")
        );
    }

    #[test]
    fn accepts_a_prerelease_when_nothing_else_is_published() {
        let releases = [release("v1.5.0-beta", true, &["x.zip"])];

        assert_eq!(
            best_release(&releases).map(|release| release.tag_name.as_str()),
            Some("v1.5.0-beta")
        );
    }

    #[test]
    fn skips_nolib_zips_when_a_full_archive_exists() {
        let releases = [release(
            "v1.4.0",
            false,
            &["ForeverGuide-v1.4.0-nolib.zip", "ForeverGuide-v1.4.0.zip"],
        )];
        let summary = to_summary(
            &RepoSlug::parse("RevoltLive85/ForeverGuide").expect("valid"),
            &repo(),
            best_release(&releases),
        );

        assert_eq!(
            summary.download,
            Download::Direct {
                url: "https://github.com/dl/ForeverGuide-v1.4.0.zip".to_owned()
            }
        );
    }

    #[test]
    fn keeps_a_release_without_zip_assets_as_explicitly_unsupported() {
        let releases = [release("v1.4.0", false, &["ForeverGuide.tar.gz"])];
        let summary = to_summary(
            &RepoSlug::parse("RevoltLive85/ForeverGuide").expect("valid"),
            &repo(),
            best_release(&releases),
        );

        assert!(!summary.is_installable());
    }

    #[test]
    fn keeps_a_repo_with_no_releases_in_the_catalog_as_uninstallable() {
        let summary = to_summary(
            &RepoSlug::parse("RevoltLive85/ForeverGuide").expect("valid"),
            &repo(),
            None,
        );

        assert!(!summary.is_installable());
        assert_eq!(summary.version, None);
    }
}
