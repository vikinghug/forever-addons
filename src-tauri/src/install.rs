//! Installing, upgrading, and removing addons in the client's AddOns directory.
//!
//! The download is async because it is network I/O; everything that touches the
//! filesystem is synchronous and runs on a blocking thread, so the archive is
//! never half-applied while the UI thread waits on a future.

pub mod archive;
pub mod manifest;

use std::io::Cursor;
use std::path::{Path, PathBuf};

use futures_util::StreamExt;

use crate::domain::{AddonFolder, AddonId, AddonSummary, Download};
use crate::error::{AppError, Result};
use crate::install::archive::ArchivePlan;
use crate::install::manifest::InstalledRecord;
use crate::source::{SourceAuth, Sources};
use crate::wow::WowInstall;

/// Where an install has got to, for the UI's progress bar.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(tag = "stage", rename_all = "kebab-case")]
pub enum InstallStage {
    /// Asking the source for a download URL.
    Resolving,
    Downloading {
        received: u64,
        /// Absent when the server sends no `Content-Length`.
        total: Option<u64>,
    },
    Extracting,
    Installed {
        folders: Vec<AddonFolder>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct InstallProgress {
    pub addon_id: AddonId,
    #[serde(flatten)]
    pub stage: InstallStage,
}

/// Downloads an addon and applies it to the client, replacing any earlier
/// install of the same addon.
///
/// `previously_installed` is what the manifest recorded for this addon last
/// time, so an upgrade can clear folders the new archive no longer ships. The
/// returned record is what the caller should persist — recording an install
/// only after the archive is on disk keeps the manifest from claiming folders
/// that are not there.
pub async fn install_addon(
    sources: &Sources,
    auth: &SourceAuth,
    install: &WowInstall,
    summary: &AddonSummary,
    previously_installed: Vec<AddonFolder>,
    on_progress: &(dyn Fn(InstallProgress) + Send + Sync),
) -> Result<InstalledRecord> {
    match &summary.download {
        Download::Unsupported { format, .. } => {
            return Err(AppError::UnsupportedArchive {
                addon_id: summary.id.clone(),
                format: format.clone(),
            });
        }
        Download::External { .. } => {
            return Err(AppError::ExternalDownloadOnly {
                addon_id: summary.id.clone(),
            });
        }
        Download::Direct { .. } | Download::Brokered => {}
    }

    report(on_progress, &summary.id, InstallStage::Resolving);
    let url = sources.resolve_archive_url(summary, auth).await?;

    let bytes = download(sources, auth, summary, &url, on_progress).await?;

    report(on_progress, &summary.id, InstallStage::Extracting);
    let addons_dir = install.ensure_addons_dir()?;
    let folders = apply_archive(bytes, summary.clone(), addons_dir, previously_installed).await?;

    report(
        on_progress,
        &summary.id,
        InstallStage::Installed {
            folders: folders.clone(),
        },
    );

    Ok(InstalledRecord {
        id: summary.id.clone(),
        name: summary.name.clone(),
        version: summary.version.clone(),
        folders,
        page_url: summary.page_url.clone(),
        installed_at: manifest::now_rfc3339(),
    })
}

/// Removes the folders this application recorded for an addon.
///
/// Only recorded folders are touched, so an addon the user unpacked by hand
/// into the same directory is never caught by an uninstall. The caller drops
/// the manifest record once this returns.
pub fn uninstall_addon(install: &WowInstall, record: &InstalledRecord) -> Result<()> {
    let addons_dir = install.addons_dir();

    for folder in &record.folders {
        remove_folder(&addons_dir, folder)?;
    }

    Ok(())
}

async fn download(
    sources: &Sources,
    auth: &SourceAuth,
    summary: &AddonSummary,
    url: &str,
    on_progress: &(dyn Fn(InstallProgress) + Send + Sync),
) -> Result<Vec<u8>> {
    let source = summary.id.source;
    let mut request = sources.http().raw().get(url);

    // CurseForge's download CDN enforces the same API key as its API (since
    // 2026-07-16). The key is only ever attached to CurseForge's own hosts,
    // so it cannot leak to a third-party download server.
    if source == crate::domain::SourceId::CurseForge
        && is_curseforge_host(url)
        && let Some(key) = auth.curseforge_api_key.as_deref()
    {
        request = request.header("x-api-key", key.trim());
    }

    let response = request
        .send()
        .await
        .map_err(|err| AppError::SourceRequest {
            source_id: source,
            url: url.to_owned(),
            reason: err.to_string(),
        })?;

    let status = response.status();
    if !status.is_success() {
        return Err(AppError::SourceStatus {
            source_id: source,
            url: url.to_owned(),
            status: status.as_u16(),
        });
    }

    let total = response.content_length();
    let mut received = 0;
    let mut bytes = Vec::with_capacity(total.unwrap_or(0).min(16 * 1024 * 1024) as usize);
    let mut stream = response.bytes_stream();

    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|err| AppError::SourceRequest {
            source_id: source,
            url: url.to_owned(),
            reason: err.to_string(),
        })?;

        received += chunk.len() as u64;
        bytes.extend_from_slice(&chunk);
        report(
            on_progress,
            &summary.id,
            InstallStage::Downloading { received, total },
        );
    }

    Ok(bytes)
}

/// The filesystem half of an install, on a blocking thread.
async fn apply_archive(
    bytes: Vec<u8>,
    summary: AddonSummary,
    addons_dir: PathBuf,
    previous: Vec<AddonFolder>,
) -> Result<Vec<AddonFolder>> {
    let joined = tokio::task::spawn_blocking(move || {
        let mut zip = zip::ZipArchive::new(Cursor::new(bytes)).map_err(|err| {
            AppError::ArchiveUnreadable {
                addon_id: summary.id.clone(),
                reason: err.to_string(),
            }
        })?;

        let plan = archive::plan(&mut zip, &summary.id, &summary.name)?;

        // The addon's previous folders go first, so an upgrade that renames or
        // drops a folder does not leave the old one loading alongside the new.
        for folder in previous.iter().chain(plan.folders()) {
            remove_folder(&addons_dir, folder)?;
        }

        extract(&mut zip, &plan, &addons_dir, &summary.id)?;
        Ok(plan.folders().to_vec())
    })
    .await;

    joined.map_err(|err| AppError::Persist {
        action: "finish applying the archive",
        path: PathBuf::from("Interface/AddOns"),
        reason: err.to_string(),
    })?
}

fn extract<R: std::io::Read + std::io::Seek>(
    zip: &mut zip::ZipArchive<R>,
    plan: &ArchivePlan,
    addons_dir: &Path,
    addon_id: &AddonId,
) -> Result<()> {
    for index in 0..zip.len() {
        let mut entry = zip
            .by_index(index)
            .map_err(|err| AppError::ArchiveUnreadable {
                addon_id: addon_id.clone(),
                reason: err.to_string(),
            })?;

        let Some(relative) = plan.destination(entry.name()) else {
            continue;
        };
        let destination = addons_dir.join(&relative);

        if entry.is_dir() {
            std::fs::create_dir_all(&destination)
                .map_err(|err| AppError::io("create an addon directory", &destination, &err))?;
            continue;
        }

        if let Some(parent) = destination.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|err| AppError::io("create an addon directory", parent, &err))?;
        }

        let mut file = std::fs::File::create(&destination)
            .map_err(|err| AppError::io("write an addon file", &destination, &err))?;
        std::io::copy(&mut entry, &mut file)
            .map_err(|err| AppError::io("write an addon file", &destination, &err))?;
    }

    Ok(())
}

/// True for URLs on CurseForge's own hosts — its API and its forgecdn
/// download CDN — the only places the user's API key may be sent.
fn is_curseforge_host(url: &str) -> bool {
    let Some(host) = url
        .strip_prefix("https://")
        .and_then(|rest| rest.split(['/', '?', '#']).next())
    else {
        return false;
    };

    host == "curseforge.com"
        || host.ends_with(".curseforge.com")
        || host == "forgecdn.net"
        || host.ends_with(".forgecdn.net")
}

/// Deleting is the one operation that can destroy a user's data, so it only
/// ever happens one validated [`AddonFolder`] deep inside the AddOns directory.
fn remove_folder(addons_dir: &Path, folder: &AddonFolder) -> Result<()> {
    let path = addons_dir.join(folder.as_str());
    if !path.is_dir() {
        return Ok(());
    }

    std::fs::remove_dir_all(&path)
        .map_err(|err| AppError::io("remove an addon directory", &path, &err))
}

fn report(
    on_progress: &(dyn Fn(InstallProgress) + Send + Sync),
    addon_id: &AddonId,
    stage: InstallStage,
) {
    on_progress(InstallProgress {
        addon_id: addon_id.clone(),
        stage,
    });
}

#[cfg(test)]
mod tests {
    use std::io::Write;

    use super::*;
    use crate::domain::{AddonKey, SourceId};

    fn summary(name: &str) -> AddonSummary {
        AddonSummary {
            id: AddonId::new(
                SourceId::CurseForge,
                AddonKey::new("4857").expect("non-empty"),
            ),
            name: name.to_owned(),
            summary: String::new(),
            author: None,
            version: None,
            updated_at: None,
            icon_url: None,
            page_url: String::new(),
            categories: Vec::new(),
            downloads: None,
            expansions: Vec::new(),
            download: Download::Direct { url: String::new() },
        }
    }

    fn zip_bytes(entries: &[(&str, &str)]) -> Vec<u8> {
        let mut writer = zip::ZipWriter::new(Cursor::new(Vec::new()));
        let options = zip::write::SimpleFileOptions::default();

        for (name, body) in entries {
            writer.start_file(*name, options).expect("fixture file");
            writer.write_all(body.as_bytes()).expect("fixture body");
        }

        writer.finish().expect("fixture archive").into_inner()
    }

    async fn apply(
        entries: &[(&str, &str)],
        addons_dir: &Path,
        previous: Vec<AddonFolder>,
    ) -> Result<Vec<AddonFolder>> {
        apply_archive(
            zip_bytes(entries),
            summary("Bagnon"),
            addons_dir.to_path_buf(),
            previous,
        )
        .await
    }

    #[tokio::test]
    async fn writes_every_addon_folder_the_archive_carries() {
        let temp = tempfile::tempdir().expect("temp dir");
        let folders = apply(
            &[
                ("Bagnon/Bagnon.toc", "## Interface: 16001\n"),
                ("Bagnon/main.lua", "-- main"),
                ("Bagnon_Config/Bagnon_Config.toc", "## Interface: 16001\n"),
            ],
            temp.path(),
            Vec::new(),
        )
        .await
        .expect("archive applies");

        assert_eq!(folders.len(), 2);
        assert_eq!(
            std::fs::read_to_string(temp.path().join("Bagnon/main.lua")).expect("file written"),
            "-- main"
        );
        assert!(
            temp.path()
                .join("Bagnon_Config/Bagnon_Config.toc")
                .is_file()
        );
    }

    #[tokio::test]
    async fn removes_a_folder_an_upgrade_no_longer_ships() {
        let temp = tempfile::tempdir().expect("temp dir");
        std::fs::create_dir_all(temp.path().join("Bagnon_Forever")).expect("fixture dir");
        std::fs::write(temp.path().join("Bagnon_Forever/old.lua"), "old").expect("fixture file");

        let previous = vec![
            AddonFolder::new("Bagnon").expect("valid"),
            AddonFolder::new("Bagnon_Forever").expect("valid"),
        ];
        apply(
            &[("Bagnon/Bagnon.toc", "## Interface: 16001\n")],
            temp.path(),
            previous,
        )
        .await
        .expect("archive applies");

        assert!(!temp.path().join("Bagnon_Forever").exists());
        assert!(temp.path().join("Bagnon/Bagnon.toc").is_file());
    }

    #[tokio::test]
    async fn replaces_a_stale_file_left_by_an_earlier_version() {
        let temp = tempfile::tempdir().expect("temp dir");
        std::fs::create_dir_all(temp.path().join("Bagnon")).expect("fixture dir");
        std::fs::write(temp.path().join("Bagnon/removed.lua"), "stale").expect("fixture file");

        apply(
            &[("Bagnon/Bagnon.toc", "## Interface: 16001\n")],
            temp.path(),
            Vec::new(),
        )
        .await
        .expect("archive applies");

        assert!(!temp.path().join("Bagnon/removed.lua").exists());
    }

    #[tokio::test]
    async fn leaves_the_client_untouched_when_the_archive_holds_no_addon() {
        let temp = tempfile::tempdir().expect("temp dir");
        let error = apply(&[("readme.txt", "nothing here")], temp.path(), Vec::new())
            .await
            .expect_err("no addon in the archive");

        assert!(
            matches!(error, AppError::ArchiveHasNoAddon { .. }),
            "{error}"
        );
        assert_eq!(
            std::fs::read_dir(temp.path())
                .expect("dir readable")
                .count(),
            0
        );
    }

    #[test]
    fn ignores_a_folder_that_is_already_gone() {
        let temp = tempfile::tempdir().expect("temp dir");
        let folder = AddonFolder::new("NeverInstalled").expect("valid");

        assert!(remove_folder(temp.path(), &folder).is_ok());
    }
}
