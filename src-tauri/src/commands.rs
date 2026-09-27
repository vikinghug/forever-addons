//! The Tauri command surface — the only place the frontend can reach.
//!
//! Commands sequence work and translate between the UI's shapes and the
//! domain's; the reasoning lives in the modules they call.

use std::path::PathBuf;

use tauri::{AppHandle, Emitter, State};

use crate::catalog::{self, Catalog, CatalogQuery, SearchResults};
use crate::domain::{
    AddonDetail, AddonFolder, AddonId, AddonKey, AddonSummary, Listing, LiveSource, SourceId,
};
use crate::error::{AppError, Result};
use crate::install::manifest::InstalledRecord;
use crate::install::{self, InstallProgress};
use crate::source::{SearchAccess, SourceNotice};
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

/// The UI's view of [`Listing`]: whether a source has a catalog to pull.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum ListingKind {
    Catalog,
    Live,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct SourceStatus {
    pub id: SourceId,
    pub name: &'static str,
    pub site_url: &'static str,
    pub enabled: bool,
    pub listing: ListingKind,
    /// Only for live sources: what this session learned about search.
    pub search_access: Option<SearchAccess>,
    pub requires_api_key: bool,
    /// Only meaningful when `requires_api_key` — whether a key is saved.
    pub api_key_configured: bool,
    /// Always 0 for a live source, which keeps no catalog.
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

    let mut sources = Vec::with_capacity(SourceId::ALL.len());
    for id in SourceId::ALL {
        let found = catalogs.iter().find(|catalog| catalog.source == id);
        let key_saved = match id {
            SourceId::CurseForge => settings.curseforge_api_key.as_deref(),
            SourceId::Wago => settings.wago_token.as_deref(),
            SourceId::GitHub => None,
        }
        .is_some_and(|key| !key.trim().is_empty());
        let (listing, search_access) = match id.listing() {
            Listing::Catalog(_) => (ListingKind::Catalog, None),
            Listing::Live(live) => (ListingKind::Live, Some(state.search_access(live).await)),
        };

        sources.push(SourceStatus {
            id,
            name: id.display_name(),
            site_url: id.site_url(),
            enabled: settings.enabled_sources.contains(&id),
            listing,
            search_access,
            requires_api_key: id.requires_api_key(),
            api_key_configured: id.requires_api_key() && key_saved,
            addon_count: found.map_or(0, |catalog| catalog.addons.len()),
            fetched_at: found.map(|catalog| catalog.fetched_at.clone()),
            tracked_repos: match id {
                SourceId::GitHub => settings.github_repos.clone(),
                _ => Vec::new(),
            },
        });
    }

    Ok(AppStatus {
        client_path: settings.client_path.clone(),
        install_valid,
        install_error,
        sources,
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

/// Pulls a catalog source's whole listing and replaces its cached catalog.
///
/// Progress is emitted page by page; a many-page source is otherwise a long
/// silence.
#[tauri::command]
pub async fn refresh_source(
    app: AppHandle,
    state: State<'_, AppState>,
    source: SourceId,
) -> Result<AppStatus> {
    let Listing::Catalog(catalog_source) = source.listing() else {
        return Err(AppError::LiveSource { source_id: source });
    };

    let auth = state.source_auth().await;
    let addons = state
        .sources()
        .fetch_catalog(catalog_source, &auth, &move |progress| {
            let _ = app.emit(CATALOG_PROGRESS_EVENT, progress);
        })
        .await?;

    state
        .store_catalog(Catalog::new(catalog_source, addons))
        .await?;
    get_status(state).await
}

/// The browse list: cached catalogs matched locally, plus each enabled live
/// source queried now.
///
/// A live source that fails becomes a notice rather than an error, so one
/// unreachable source does not blank the others' results. Only a source that
/// answered is counted as searched.
#[tauri::command]
pub async fn search_addons(
    state: State<'_, AppState>,
    query: CatalogQuery,
) -> Result<SearchResults> {
    let catalogs = state.enabled_catalogs().await;
    let mut searched = catalogs
        .iter()
        .map(|catalog| catalog.source)
        .collect::<Vec<_>>();
    let mut candidates = catalog::matching(catalogs.iter(), &query);
    let mut notices = Vec::new();
    let auth = state.source_auth().await;

    for live in state.enabled_live_sources().await {
        if !query.includes_source(live.id()) || !has_key(&auth, live) {
            continue;
        }

        let access = state.search_access(live).await;
        match state
            .sources()
            .browse_live(live, &query.text, query.sort, access, &auth)
            .await
        {
            Ok(results) => {
                state.record_search_access(live, results.access).await;
                searched.push(live.id());
                candidates.extend(results.addons);
                notices.extend(results.notice.map(|message| SourceNotice {
                    source: live.id(),
                    message,
                }));
            }
            Err(err) => notices.push(SourceNotice {
                source: live.id(),
                message: err.to_string(),
            }),
        }
    }

    Ok(catalog::assemble(candidates, &searched, &query, notices))
}

/// A live source without a key is skipped quietly; the Sources screen already
/// says the key is missing.
fn has_key(auth: &crate::source::SourceAuth, source: LiveSource) -> bool {
    let key = match source {
        LiveSource::CurseForge => auth.curseforge_api_key.as_deref(),
    };

    key.is_some_and(|key| !key.trim().is_empty())
}

#[tauri::command]
pub async fn get_addon_detail(state: State<'_, AppState>, id: AddonId) -> Result<AddonDetail> {
    let auth = state.source_auth().await;
    match id.source.listing() {
        Listing::Catalog(source) => {
            let summary = find_in_catalog(&state, &id).await?;
            state.sources().fetch_detail(source, &summary, &auth).await
        }
        Listing::Live(source) => {
            state
                .sources()
                .fetch_live_detail(source, &id.key, &auth)
                .await
        }
    }
}

#[tauri::command]
pub async fn list_installed(state: State<'_, AppState>) -> Result<InstalledView> {
    let install = state.wow_install().await?;
    let scanned = wow::scan_addons_dir(&install.addons_dir())?;
    let manifest = state.manifest().await;

    let mut published = state
        .all_catalogs()
        .await
        .into_iter()
        .flat_map(|catalog| catalog.addons)
        .collect::<Vec<_>>();
    published.extend(live_installed(&state, &manifest).await);

    Ok(view::build(scanned, &manifest, &published))
}

/// Installed addons from live sources, looked up in one request per source
/// so their update status is current. A failed lookup leaves the status
/// unknown rather than failing the whole list.
async fn live_installed(
    state: &State<'_, AppState>,
    manifest: &install::manifest::Manifest,
) -> Vec<AddonSummary> {
    let auth = state.source_auth().await;
    let mut found = Vec::new();

    for live in LiveSource::ALL {
        let keys = manifest
            .records()
            .filter(|record| record.id.source == live.id())
            .map(|record| record.id.key.clone())
            .collect::<Vec<AddonKey>>();
        if keys.is_empty() || !has_key(&auth, live) {
            continue;
        }

        if let Ok(addons) = state.sources().lookup_live_many(live, &keys, &auth).await {
            found.extend(addons);
        }
    }

    found
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
    let auth = state.source_auth().await;
    let summary = current_summary(&state, &auth, &id).await?;
    let previously_installed = recorded_folders(&state, &id).await;

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

    save_record(state, record).await
}

/// Installs one addon from a zip the user downloaded from its page — the
/// route for an author who allows downloads only on the source's website.
#[tauri::command]
pub async fn install_addon_file(
    app: AppHandle,
    state: State<'_, AppState>,
    id: AddonId,
    path: PathBuf,
) -> Result<InstalledView> {
    let install = state.wow_install().await?;
    let auth = state.source_auth().await;
    let summary = current_summary(&state, &auth, &id).await?;
    let previously_installed = recorded_folders(&state, &id).await;

    let record = install::install_from_file(
        &install,
        &summary,
        path,
        previously_installed,
        &move |progress: InstallProgress| {
            let _ = app.emit(INSTALL_PROGRESS_EVENT, progress);
        },
    )
    .await?;

    save_record(state, record).await
}

/// The addon as its source publishes it now: the cached catalog row, or a
/// fresh live lookup.
async fn current_summary(
    state: &State<'_, AppState>,
    auth: &crate::source::SourceAuth,
    id: &AddonId,
) -> Result<AddonSummary> {
    match id.source.listing() {
        Listing::Catalog(_) => find_in_catalog(state, id).await,
        Listing::Live(source) => state.sources().lookup_live(source, &id.key, auth).await,
    }
}

/// What the manifest recorded for this addon last time, so an upgrade can
/// clear folders the new archive no longer ships.
async fn recorded_folders(state: &State<'_, AppState>, id: &AddonId) -> Vec<AddonFolder> {
    state
        .manifest()
        .await
        .get(id)
        .map(|record| record.folders.clone())
        .unwrap_or_default()
}

/// Persists a finished install, then re-reads the installed view.
async fn save_record(state: State<'_, AppState>, record: InstalledRecord) -> Result<InstalledView> {
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
