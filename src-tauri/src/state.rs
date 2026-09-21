//! The application's shared state: settings, manifest, and loaded catalogs.
//!
//! Commands read through a read lock and write through a short write lock;
//! network work always happens outside the lock, so a long catalog refresh
//! never blocks the installed list from rendering.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use tokio::sync::RwLock;

use crate::catalog::{self, Catalog};
use crate::config::{self, Settings};
use crate::domain::SourceId;
use crate::error::{AppError, Result};
use crate::install::manifest::{self, Manifest};
use crate::source::{SourceAuth, Sources};
use crate::wow::WowInstall;

pub struct AppState {
    data_dir: PathBuf,
    sources: Sources,
    inner: RwLock<Inner>,
}

struct Inner {
    settings: Settings,
    manifest: Manifest,
    catalogs: BTreeMap<SourceId, Catalog>,
}

impl AppState {
    /// Reads everything persisted from a previous run. Absent or damaged files
    /// yield defaults, so a first launch and a corrupted cache behave alike.
    pub fn load(data_dir: PathBuf) -> Result<Self> {
        let settings = Settings::load(&config::settings_path(&data_dir));
        let manifest = Manifest::load(&manifest::manifest_path(&data_dir));
        let catalogs = SourceId::ALL
            .into_iter()
            .filter_map(|source| {
                catalog::load_cached(&data_dir, source).map(|catalog| (source, catalog))
            })
            .collect();

        Ok(Self {
            sources: Sources::new()?,
            inner: RwLock::new(Inner {
                settings,
                manifest,
                catalogs,
            }),
            data_dir,
        })
    }

    pub fn sources(&self) -> &Sources {
        &self.sources
    }

    pub fn data_dir(&self) -> &Path {
        &self.data_dir
    }

    pub async fn settings(&self) -> Settings {
        self.inner.read().await.settings.clone()
    }

    /// The per-source settings requests need, read fresh so a just-saved
    /// value applies.
    pub async fn source_auth(&self) -> SourceAuth {
        let inner = self.inner.read().await;
        SourceAuth {
            curseforge_api_key: inner.settings.curseforge_api_key.clone(),
            wago_token: inner.settings.wago_token.clone(),
            github_repos: inner.settings.github_repos.clone(),
        }
    }

    /// The configured client, validated now rather than at configuration time —
    /// a drive can be unmounted between launches.
    pub async fn wow_install(&self) -> Result<WowInstall> {
        let path = self
            .inner
            .read()
            .await
            .settings
            .client_path
            .clone()
            .ok_or(AppError::NoInstallConfigured)?;

        WowInstall::open(path)
    }

    /// Stores a client path only once it validates.
    pub async fn set_client_path(&self, path: PathBuf) -> Result<WowInstall> {
        let install = WowInstall::open(path.clone())?;
        self.change_settings(|settings| settings.client_path = Some(path))
            .await?;

        Ok(install)
    }

    pub async fn set_enabled_sources(&self, sources: Vec<SourceId>) -> Result<Settings> {
        self.change_settings(|settings| settings.enabled_sources = sources)
            .await
    }

    /// Stores the CurseForge key; `None` or a blank string clears it.
    pub async fn set_curseforge_api_key(&self, key: Option<String>) -> Result<Settings> {
        self.change_settings(|settings| settings.curseforge_api_key = normalize_secret(key))
            .await
    }

    /// Stores the Wago access token; `None` or a blank string clears it.
    pub async fn set_wago_token(&self, token: Option<String>) -> Result<Settings> {
        self.change_settings(|settings| settings.wago_token = normalize_secret(token))
            .await
    }

    /// Tracks a GitHub repository. Re-adding an already tracked repository
    /// moves it to the end rather than duplicating it.
    pub async fn add_github_repo(&self, slug: String) -> Result<Settings> {
        self.change_settings(|settings| {
            settings
                .github_repos
                .retain(|existing| !existing.eq_ignore_ascii_case(&slug));
            settings.github_repos.push(slug);
        })
        .await
    }

    pub async fn remove_github_repo(&self, slug: &str) -> Result<Settings> {
        self.change_settings(|settings| {
            settings
                .github_repos
                .retain(|existing| !existing.eq_ignore_ascii_case(slug));
        })
        .await
    }

    async fn change_settings(&self, change: impl FnOnce(&mut Settings)) -> Result<Settings> {
        let mut inner = self.inner.write().await;
        change(&mut inner.settings);
        inner
            .settings
            .save(&config::settings_path(&self.data_dir))?;

        Ok(inner.settings.clone())
    }

    /// The catalogs of enabled sources, in the order the user arranged them.
    pub async fn enabled_catalogs(&self) -> Vec<Catalog> {
        let inner = self.inner.read().await;
        inner
            .settings
            .enabled_sources
            .iter()
            .filter_map(|source| inner.catalogs.get(source).cloned())
            .collect()
    }

    pub async fn all_catalogs(&self) -> Vec<Catalog> {
        self.inner.read().await.catalogs.values().cloned().collect()
    }

    pub async fn store_catalog(&self, catalog: Catalog) -> Result<()> {
        catalog::save_cached(&self.data_dir, &catalog)?;
        self.inner
            .write()
            .await
            .catalogs
            .insert(catalog.source, catalog);

        Ok(())
    }

    pub async fn manifest(&self) -> Manifest {
        self.inner.read().await.manifest.clone()
    }

    /// Applies a change to the manifest and persists it under one write lock,
    /// so a concurrent install cannot drop the other's record.
    pub async fn update_manifest<T>(
        &self,
        change: impl FnOnce(&mut Manifest) -> Result<T>,
    ) -> Result<T> {
        let mut inner = self.inner.write().await;
        let outcome = change(&mut inner.manifest)?;
        inner
            .manifest
            .save(&manifest::manifest_path(&self.data_dir))?;

        Ok(outcome)
    }
}

/// `None` for an absent or blank secret, the trimmed value otherwise.
fn normalize_secret(raw: Option<String>) -> Option<String> {
    raw.map(|value| value.trim().to_owned())
        .filter(|trimmed| !trimmed.is_empty())
}
