//! The addon providers.
//!
//! Providers are a closed set known at compile time, so dispatch is an
//! exhaustive `match` rather than a trait object: adding a provider means
//! adding a `SourceId` variant, and the compiler then points at every place
//! that has to learn about it.

pub mod curseforge;
pub mod github;
pub mod http;
pub mod wago;

use crate::domain::{
    AddonDetail, AddonId, AddonKey, AddonSummary, CatalogSource, LiveSource, Sort, SourceId,
};
use crate::error::Result;
use crate::source::http::HttpClient;

/// Progress reported while a catalog is being pulled, so a multi-page fetch is
/// not a silent thirty-second pause in the UI.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
pub struct FetchProgress {
    pub source: SourceId,
    pub page: u32,
    pub total_pages: Option<u32>,
    pub addons_so_far: usize,
}

/// Whether a live source's key may use its search endpoint. CurseForge grants
/// search per key, and answers a key without it with a 403, so this is
/// learned from the first search of a session and never persisted.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum SearchAccess {
    #[default]
    Untested,
    Allowed,
    Forbidden,
}

/// A sentence the browse list shows about one source's results: why they are
/// partial, or why there are none.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct SourceNotice {
    pub source: SourceId,
    pub message: String,
}

/// One live query's answer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LiveResults {
    pub addons: Vec<AddonSummary>,
    /// What this query learned about the key's search access.
    pub access: SearchAccess,
    pub notice: Option<String>,
}

/// The per-source pieces of settings a request needs — credentials and the
/// user's tracked GitHub repositories — read at call time so a value saved a
/// moment ago is used without restarting anything.
#[derive(Debug, Clone, Default)]
pub struct SourceAuth {
    pub curseforge_api_key: Option<String>,
    pub wago_token: Option<String>,
    pub github_repos: Vec<String>,
}

impl SourceAuth {
    fn curseforge_key(&self) -> Result<&str> {
        required_key(&self.curseforge_api_key, SourceId::CurseForge)
    }

    fn wago_token(&self) -> Result<&str> {
        required_key(&self.wago_token, SourceId::Wago)
    }
}

fn required_key(key: &Option<String>, source_id: SourceId) -> Result<&str> {
    key.as_deref()
        .map(str::trim)
        .filter(|key| !key.is_empty())
        .ok_or(crate::error::AppError::MissingApiKey { source_id })
}

/// Every provider, sharing one HTTP client.
#[derive(Debug, Clone)]
pub struct Sources {
    http: HttpClient,
}

impl Sources {
    pub fn new() -> Result<Self> {
        Ok(Self {
            http: HttpClient::new()?,
        })
    }

    pub fn http(&self) -> &HttpClient {
        &self.http
    }

    /// Every compatible addon the source lists, in the source's own order.
    pub async fn fetch_catalog(
        &self,
        source: CatalogSource,
        auth: &SourceAuth,
        on_progress: &(dyn Fn(FetchProgress) + Send + Sync),
    ) -> Result<Vec<AddonSummary>> {
        match source {
            CatalogSource::Wago => {
                wago::fetch_catalog(&self.http, auth.wago_token()?, on_progress).await
            }
            CatalogSource::GitHub => {
                github::fetch_catalog(&self.http, &auth.github_repos, on_progress).await
            }
        }
    }

    /// A cataloged addon's own page: long description, screenshots, upstream
    /// link.
    pub async fn fetch_detail(
        &self,
        source: CatalogSource,
        summary: &AddonSummary,
        auth: &SourceAuth,
    ) -> Result<AddonDetail> {
        match source {
            CatalogSource::Wago => {
                wago::fetch_detail(&self.http, auth.wago_token()?, summary).await
            }
            CatalogSource::GitHub => github::fetch_detail(&self.http, summary).await,
        }
    }

    /// What a live source has for the browse box's text right now, asked
    /// for in `sort` order so the page it returns is the right one.
    pub async fn browse_live(
        &self,
        source: LiveSource,
        text: &str,
        sort: Sort,
        access: SearchAccess,
        auth: &SourceAuth,
    ) -> Result<LiveResults> {
        match source {
            LiveSource::CurseForge => {
                curseforge::browse(&self.http, auth.curseforge_key()?, text, sort, access).await
            }
        }
    }

    /// One live addon as the source publishes it this moment.
    pub async fn lookup_live(
        &self,
        source: LiveSource,
        key: &AddonKey,
        auth: &SourceAuth,
    ) -> Result<AddonSummary> {
        match source {
            LiveSource::CurseForge => {
                curseforge::lookup(&self.http, auth.curseforge_key()?, key).await
            }
        }
    }

    /// Several live addons in one request, for the installed list's update
    /// check. Addons the source no longer lists are left out.
    pub async fn lookup_live_many(
        &self,
        source: LiveSource,
        keys: &[AddonKey],
        auth: &SourceAuth,
    ) -> Result<Vec<AddonSummary>> {
        match source {
            LiveSource::CurseForge => {
                curseforge::lookup_many(&self.http, auth.curseforge_key()?, keys).await
            }
        }
    }

    /// A live addon's own page, fetched fresh.
    pub async fn fetch_live_detail(
        &self,
        source: LiveSource,
        key: &AddonKey,
        auth: &SourceAuth,
    ) -> Result<AddonDetail> {
        match source {
            LiveSource::CurseForge => {
                curseforge::fetch_detail(&self.http, auth.curseforge_key()?, key).await
            }
        }
    }

    /// The URL to download right now.
    ///
    /// Wago mints short-lived links, which is why this is resolved at install
    /// time instead of being cached with the catalog. A CurseForge summary is
    /// looked up moments before the install, so its file is already current.
    pub async fn resolve_archive_url(
        &self,
        summary: &AddonSummary,
        auth: &SourceAuth,
    ) -> Result<String> {
        match summary.id.source {
            SourceId::CurseForge => {
                curseforge::resolve_archive_url(&self.http, auth.curseforge_key()?, summary).await
            }
            SourceId::Wago => {
                wago::resolve_archive_url(&self.http, auth.wago_token()?, summary).await
            }
            SourceId::GitHub => github::resolve_archive_url(summary),
        }
    }
}

/// A source-local id, rendered for error messages before an `AddonId` exists.
pub(crate) fn missing_field(
    source: SourceId,
    url: &str,
    expected: &'static str,
    reason: impl Into<String>,
) -> crate::error::AppError {
    crate::error::AppError::SourceShape {
        source_id: source,
        url: url.to_owned(),
        expected,
        reason: reason.into(),
    }
}

/// Shorthand for the id every source builds the same way.
pub(crate) fn addon_id(source: SourceId, key: &str) -> Option<AddonId> {
    crate::domain::AddonKey::new(key).map(|key| AddonId::new(source, key))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_saved_key_is_trimmed_before_use() {
        let auth = SourceAuth {
            curseforge_api_key: Some("  user-key  ".to_owned()),
            ..SourceAuth::default()
        };

        assert_eq!(
            auth.curseforge_key().expect("saved key resolves"),
            "user-key"
        );
    }

    #[test]
    fn a_missing_or_blank_key_names_the_source_and_the_fix() {
        for auth in [
            SourceAuth::default(),
            SourceAuth {
                curseforge_api_key: Some("   ".to_owned()),
                ..SourceAuth::default()
            },
        ] {
            let error = auth.curseforge_key().expect_err("no usable key");
            assert!(error.to_string().contains("API key"), "{error}");

            let error = auth.wago_token().expect_err("no usable token");
            assert!(error.to_string().contains("Wago"), "{error}");
        }
    }
}
