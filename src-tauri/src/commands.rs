//! The Tauri command surface — the only place the frontend can reach.
//!
//! Commands sequence work and translate between the UI's shapes and the
//! domain's; the reasoning lives in the modules they call.

use std::path::PathBuf;

use tauri::{AppHandle, Emitter, State};

use crate::catalog::{self, Catalog, CatalogQuery};
use crate::domain::{AddonDetail, AddonId, AddonSummary, SourceId};
use crate::error::{AppError, Result};
use crate::install::{self, InstallProgress};
use crate::state::AppState;
use crate::view::{self, InstalledView};
use crate::wow;

/// Emitted while a catalog refresh walks a source's pages.
pub const CATALOG_PROGRESS_EVENT: &str = "catalog:progress";
/// Emitted while an addon downloads and extracts.
pub const INSTALL_PROGRESS_EVENT: &str = "install:progress";

/// What the shell renders before anything is loaded: which client is
/// configured, which sources are on, and how fresh each catalog is.
#[derive(Debug, Clone, serde::Serialize)]
pub struct AppStatus {
    pub client_path: Option<PathBuf>,
    /// False when a path is configured but no longer holds a client.
    pub install_valid: bool,
    pub install_error: Option<String>,
    pub sources: Vec<SourceStatus>,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct SourceStatus {
    pub id: SourceId,
    pub name: &'static str,
    pub site_url: &'static str,
    pub enabled: bool,
    pub requires_api_key: bool,
    /// Only meaningful when `requires_api_key` — whether a key is saved.
    pub api_key_configured: bool,
    pub addon_count: usize,
    /// RFC 3339, or `None` when this source has never been refreshed.
    pub fetched_at: Option<String>,
    /// The repositories the user tracks — only ever non-empty for GitHub.
    pub tracked_repos: Vec<String>,
}

#[tauri::command]
pub async fn get_status(state: State<'_, AppState>) -> Result<AppStatus> {
    let settings = state.settings().await;
    let catalogs = state.all_catalogs().await;

    let (install_valid, install_error) = match settings.client_path.as_ref() {
        None => (false, None),
        Some(path) => match wow::WowInstall::open(path.clone()) {
            Ok(_) => (true, None),
            Err(err) => (false, Some(err.to_string())),
        },
    };

    Ok(AppStatus {
        client_path: settings.client_path.clone(),
        install_valid,
        install_error,
        sources: SourceId::ALL
            .into_iter()
            .map(|id| {
                let found = catalogs.iter().find(|catalog| catalog.source == id);
                let key_saved = match id {
                    SourceId::CurseForge => settings.curseforge_api_key.as_deref(),
                    SourceId::Wago => settings.wago_token.as_deref(),
                    SourceId::GitHub => None,
                }
                .is_some_and(|key| !key.trim().is_empty());

                SourceStatus {
                    id,
                    name: id.display_name(),
                    site_url: id.site_url(),
                    enabled: settings.enabled_sources.contains(&id),
                    requires_api_key: id.requires_api_key(),
                    api_key_configured: id.requires_api_key() && key_saved,
                    addon_count: found.map_or(0, |catalog| catalog.addons.len()),
                    fetched_at: found.map(|catalog| catalog.fetched_at.clone()),
                    tracked_repos: match id {
                        SourceId::GitHub => settings.github_repos.clone(),
                        _ => Vec::new(),
                    },
                }
            })
            .collect(),
    })
}

/// A guess at where the client lives, for the first-run prompt.
#[tauri::command]
pub async fn detect_wow_install() -> Result<Option<PathBuf>> {
    Ok(wow::detect_install().map(|install| install.root().to_path_buf()))
}

#[tauri::command]
pub async fn set_client_path(state: State<'_, AppState>, path: PathBuf) -> Result<AppStatus> {
    state.set_client_path(path).await?;
    get_status(state).await
}

#[tauri::command]
pub async fn set_enabled_sources(
    state: State<'_, AppState>,
    sources: Vec<SourceId>,
) -> Result<AppStatus> {
    state.set_enabled_sources(sources).await?;
    get_status(state).await
}

#[tauri::command]
pub async fn set_curseforge_api_key(
    state: State<'_, AppState>,
    key: Option<String>,
) -> Result<AppStatus> {
    state.set_curseforge_api_key(key).await?;
    get_status(state).await
}

#[tauri::command]
pub async fn set_wago_token(
    state: State<'_, AppState>,
    token: Option<String>,
) -> Result<AppStatus> {
    state.set_wago_token(token).await?;
    get_status(state).await
}

/// Tracks a GitHub repository for the GitHub source. Accepts `owner/name` or
/// a pasted GitHub URL; anything else is refused with the shape it expected.
#[tauri::command]
pub async fn add_github_repo(state: State<'_, AppState>, repo: String) -> Result<AppStatus> {
    let Some(slug) = crate::source::github::RepoSlug::parse(&repo) else {
        return Err(AppError::SourceShape {
            source_id: SourceId::GitHub,
            url: repo,
            expected: "an owner/name repository or a github.com URL",
            reason: "could not read an owner and repository name out of it".to_owned(),
        });
    };

    state.add_github_repo(slug.as_str().to_owned()).await?;
    get_status(state).await
}

#[tauri::command]
pub async fn remove_github_repo(state: State<'_, AppState>, repo: String) -> Result<AppStatus> {
    state.remove_github_repo(&repo).await?;
    get_status(state).await
}

/// Pulls a source's whole listing and replaces its cached catalog.
///
/// Progress is emitted page by page; a many-page source is otherwise a long
/// silence.
#[tauri::command]
pub async fn refresh_source(
    app: AppHandle,
    state: State<'_, AppState>,
    source: SourceId,
) -> Result<AppStatus> {
    let auth = state.source_auth().await;
    let addons = state
        .sources()
        .fetch_catalog(source, &auth, &move |progress| {
            let _ = app.emit(CATALOG_PROGRESS_EVENT, progress);
        })
        .await?;

    state.store_catalog(Catalog::new(source, addons)).await?;
    get_status(state).await
}

#[tauri::command]
pub async fn search_catalog(
    state: State<'_, AppState>,
    query: CatalogQuery,
) -> Result<Vec<AddonSummary>> {
    Ok(catalog::search(
        state.enabled_catalogs().await.iter(),
        &query,
    ))
}

#[tauri::command]
pub async fn list_categories(state: State<'_, AppState>) -> Result<Vec<String>> {
    Ok(catalog::categories(state.enabled_catalogs().await.iter()))
}

#[tauri::command]
pub async fn get_addon_detail(state: State<'_, AppState>, id: AddonId) -> Result<AddonDetail> {
    let summary = find_in_catalog(&state, &id).await?;
    let auth = state.source_auth().await;
    state.sources().fetch_detail(&summary, &auth).await
}

#[tauri::command]
pub async fn list_installed(state: State<'_, AppState>) -> Result<InstalledView> {
    let install = state.wow_install().await?;
    let scanned = wow::scan_addons_dir(&install.addons_dir())?;

    Ok(view::build(
        scanned,
        &state.manifest().await,
        state.all_catalogs().await.iter(),
    ))
}

/// Installs or upgrades one addon, then re-reads the installed view so the UI
/// reflects the client's actual state rather than an assumed one.
#[tauri::command]
pub async fn install_addon(
    app: AppHandle,
    state: State<'_, AppState>,
    id: AddonId,
) -> Result<InstalledView> {
    let install = state.wow_install().await?;
    let summary = find_in_catalog(&state, &id).await?;
    let auth = state.source_auth().await;
    let previously_installed = state
        .manifest()
        .await
        .get(&id)
        .map(|record| record.folders.clone())
        .unwrap_or_default();

    let record = install::install_addon(
        state.sources(),
        &auth,
        &install,
        &summary,
        previously_installed,
        &move |progress: InstallProgress| {
            let _ = app.emit(INSTALL_PROGRESS_EVENT, progress);
        },
    )
    .await?;

    state
        .update_manifest(move |manifest| {
            manifest.insert(record);
            Ok(())
        })
        .await?;

    list_installed(state).await
}

#[tauri::command]
pub async fn uninstall_addon(state: State<'_, AppState>, id: AddonId) -> Result<InstalledView> {
    let install = state.wow_install().await?;
    let record =
        state
            .manifest()
            .await
            .get(&id)
            .cloned()
            .ok_or_else(|| AppError::NotInstalled {
                addon_id: id.clone(),
            })?;

    install::uninstall_addon(&install, &record)?;
    state
        .update_manifest(move |manifest| {
            manifest.remove(&id);
            Ok(())
        })
        .await?;

    list_installed(state).await
}

/// Opens an addon's page on the source's site in the user's browser.
#[tauri::command]
pub async fn open_url(app: AppHandle, url: String) -> Result<()> {
    use tauri_plugin_opener::OpenerExt;

    app.opener()
        .open_url(url.clone(), None::<&str>)
        .map_err(|err| AppError::SourceRequest {
            source_id: SourceId::CurseForge,
            url,
            reason: err.to_string(),
        })
}

async fn find_in_catalog(state: &State<'_, AppState>, id: &AddonId) -> Result<AddonSummary> {
    state
        .all_catalogs()
        .await
        .iter()
        .find_map(|catalog| catalog.find(id).cloned())
        .ok_or_else(|| AppError::UnknownAddon {
            addon_id: id.clone(),
        })
}
