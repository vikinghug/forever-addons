//! Working out what an addon archive actually contains, before anything is
//! written to the client.
//!
//! Addon zips come in three shapes in the wild:
//!
//! * folders at the root — `Bagnon/Bagnon.toc`, `Bagnon_Config/…`
//! * a wrapper folder, typical of GitHub archives — `Bagnon-master/Bagnon/…`
//! * loose files at the root — `Bagnon.toc`, `main.lua`
//!
//! The plan normalizes all three to "these folder names, extracted from this
//! prefix", so the installer and the uninstaller agree on what an addon owns.

use std::io::{Read, Seek};

use crate::domain::{AddonFolder, AddonId};
use crate::error::{AppError, Result};

/// Junk that archiving tools add and WoW must never see.
const IGNORED_PREFIXES: [&str; 2] = ["__MACOSX/", ".git/"];
const IGNORED_NAMES: [&str; 2] = [".DS_Store", "Thumbs.db"];

/// What to extract, and where each entry lands under `Interface/AddOns`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArchivePlan {
    /// Stripped from the front of every entry path — the wrapper folder, when
    /// the archive has one.
    strip: String,
    /// Set only for archives whose files sit at the root and therefore need a
    /// folder invented for them.
    wrap_in: Option<AddonFolder>,
    /// Every folder this addon will own once installed. Uninstalling removes
    /// exactly these.
    folders: Vec<AddonFolder>,
}

impl ArchivePlan {
    pub fn folders(&self) -> &[AddonFolder] {
        &self.folders
    }

    /// Where an archive entry belongs, relative to `Interface/AddOns`, or
    /// `None` for entries outside the plan (the wrapper folder's own siblings,
    /// such as a top-level `README.md` in a GitHub archive).
    pub fn destination(&self, entry: &str) -> Option<String> {
        let normalized = entry.replace('\\', "/");
        let rest = normalized
            .strip_prefix(&self.strip)?
            .trim_start_matches('/');

        if rest.is_empty() || is_ignored(rest) {
            return None;
        }

        match &self.wrap_in {
            Some(folder) => Some(format!("{folder}/{rest}")),
            None => {
                let owns_first_component = rest
                    .split('/')
                    .next()
                    .and_then(|name| AddonFolder::new(name).ok())
                    .is_some_and(|folder| self.folders.contains(&folder));

                owns_first_component.then(|| rest.to_owned())
            }
        }
    }
}

/// Reads the archive's directory and decides what it installs.
///
/// `fallback_name` names the folder for an archive whose files sit at the root;
/// it is the addon's catalog name, which is the only name available then.
pub fn plan<R: Read + Seek>(
    archive: &mut zip::ZipArchive<R>,
    addon_id: &AddonId,
    fallback_name: &str,
) -> Result<ArchivePlan> {
    let entries = read_entry_names(archive, addon_id)?;
    let toc_paths = entries
        .iter()
        .filter(|entry| ends_with_toc(entry))
        .collect::<Vec<_>>();

    let depth = toc_paths
        .iter()
        .map(|path| path.matches('/').count())
        .min()
        .ok_or_else(|| AppError::ArchiveHasNoAddon {
            addon_id: addon_id.clone(),
        })?;

    match depth {
        0 => plan_loose_files(addon_id, fallback_name),
        _ => plan_nested_folders(&toc_paths, depth, addon_id),
    }
}

/// `Bagnon.toc` at the root: everything goes into one folder named for the addon.
fn plan_loose_files(addon_id: &AddonId, fallback_name: &str) -> Result<ArchivePlan> {
    let folder = AddonFolder::new(sanitize_folder_name(fallback_name)).map_err(|err| {
        AppError::ArchiveUnsafePath {
            addon_id: addon_id.clone(),
            entry: err.to_string(),
        }
    })?;

    Ok(ArchivePlan {
        strip: String::new(),
        wrap_in: Some(folder.clone()),
        folders: vec![folder],
    })
}

/// Every other archive: the shallowest `.toc` files sit one level inside their
/// addon folder, so everything above that level is a wrapper to strip.
///
/// `Bagnon/Bagnon.toc` strips nothing; `Bagnon-master/Bagnon/Bagnon.toc` strips
/// `Bagnon-master/`. Deeper `.toc` files belong to bundled libraries and take no
/// part in naming folders.
fn plan_nested_folders(
    toc_paths: &[&String],
    depth: usize,
    addon_id: &AddonId,
) -> Result<ArchivePlan> {
    let shallowest = toc_paths
        .iter()
        .find(|path| path.matches('/').count() == depth)
        .ok_or_else(|| AppError::ArchiveHasNoAddon {
            addon_id: addon_id.clone(),
        })?;

    let strip = shallowest
        .split('/')
        .take(depth - 1)
        .fold(String::new(), |mut acc, part| {
            acc.push_str(part);
            acc.push('/');
            acc
        });

    let folders = collect_folders(toc_paths, depth, &strip, addon_id)?;

    Ok(ArchivePlan {
        strip,
        wrap_in: None,
        folders,
    })
}

/// The distinct folder names holding a shallowest `.toc`, in sorted order.
fn collect_folders(
    toc_paths: &[&String],
    depth: usize,
    strip: &str,
    addon_id: &AddonId,
) -> Result<Vec<AddonFolder>> {
    let mut folders = Vec::new();

    for path in toc_paths {
        if path.matches('/').count() != depth {
            continue;
        }
        let Some(rest) = path.strip_prefix(strip) else {
            continue;
        };
        let Some(name) = rest.split('/').next() else {
            continue;
        };

        let folder = AddonFolder::new(name).map_err(|err| AppError::ArchiveUnsafePath {
            addon_id: addon_id.clone(),
            entry: err.to_string(),
        })?;

        if !folders.contains(&folder) {
            folders.push(folder);
        }
    }

    if folders.is_empty() {
        return Err(AppError::ArchiveHasNoAddon {
            addon_id: addon_id.clone(),
        });
    }

    folders.sort();
    Ok(folders)
}

/// Every entry path, rejecting anything that could escape the AddOns directory.
///
/// `enclosed_name` is the zip crate's own check for absolute paths, `..`
/// traversal, and Windows drive prefixes; an entry it refuses fails the whole
/// install rather than being silently skipped.
fn read_entry_names<R: Read + Seek>(
    archive: &mut zip::ZipArchive<R>,
    addon_id: &AddonId,
) -> Result<Vec<String>> {
    let mut names = Vec::with_capacity(archive.len());

    for index in 0..archive.len() {
        let entry = archive
            .by_index(index)
            .map_err(|err| AppError::ArchiveUnreadable {
                addon_id: addon_id.clone(),
                reason: err.to_string(),
            })?;

        let raw = entry.name().replace('\\', "/");
        if raw.is_empty() || is_ignored(&raw) {
            continue;
        }
        if entry.enclosed_name().is_none() {
            return Err(AppError::ArchiveUnsafePath {
                addon_id: addon_id.clone(),
                entry: raw,
            });
        }

        names.push(raw);
    }

    Ok(names)
}

fn is_ignored(path: &str) -> bool {
    IGNORED_PREFIXES
        .iter()
        .any(|prefix| path.starts_with(prefix))
        || path
            .rsplit('/')
            .next()
            .is_some_and(|name| IGNORED_NAMES.contains(&name))
}

fn ends_with_toc(path: &str) -> bool {
    std::path::Path::new(path)
        .extension()
        .and_then(|ext| ext.to_str())
        .is_some_and(|ext| ext.eq_ignore_ascii_case("toc"))
}

/// Turns a catalog name into something WoW will load as a folder name.
fn sanitize_folder_name(name: &str) -> String {
    let cleaned = name
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '_' || c == '-' {
                c
            } else {
                '_'
            }
        })
        .collect::<String>();

    cleaned.trim_matches('_').to_owned()
}

#[cfg(test)]
mod tests {
    use std::io::{Cursor, Write};

    use super::*;
    use crate::domain::{AddonKey, SourceId};

    fn addon_id() -> AddonId {
        AddonId::new(
            SourceId::CurseForge,
            AddonKey::new("4857").expect("non-empty key"),
        )
    }

    fn archive_of(entries: &[&str]) -> zip::ZipArchive<Cursor<Vec<u8>>> {
        let mut writer = zip::ZipWriter::new(Cursor::new(Vec::new()));
        for entry in entries {
            add_entry(&mut writer, entry);
        }

        let buffer = writer.finish().expect("fixture archive");
        zip::ZipArchive::new(buffer).expect("fixture archive reads back")
    }

    fn add_entry(writer: &mut zip::ZipWriter<Cursor<Vec<u8>>>, entry: &str) {
        let options = zip::write::SimpleFileOptions::default();

        if let Some(dir) = entry.strip_suffix('/') {
            writer.add_directory(dir, options).expect("fixture dir");
            return;
        }

        writer.start_file(entry, options).expect("fixture file");
        writer.write_all(b"x").expect("fixture body");
    }

    fn plan_of(entries: &[&str]) -> Result<ArchivePlan> {
        plan(&mut archive_of(entries), &addon_id(), "Bagnon")
    }

    fn folder_names(plan: &ArchivePlan) -> Vec<&str> {
        plan.folders().iter().map(AddonFolder::as_str).collect()
    }

    #[test]
    fn plans_an_archive_whose_addon_folders_sit_at_the_root() {
        let plan = plan_of(&[
            "Bagnon/Bagnon.toc",
            "Bagnon/main.lua",
            "Bagnon_Config/Bagnon_Config.toc",
        ])
        .expect("archive holds addons");

        assert_eq!(folder_names(&plan), vec!["Bagnon", "Bagnon_Config"]);
        assert_eq!(
            plan.destination("Bagnon/main.lua").as_deref(),
            Some("Bagnon/main.lua")
        );
    }

    #[test]
    fn strips_the_wrapper_folder_a_github_archive_adds() {
        let plan = plan_of(&[
            "Bagnon-master/README.md",
            "Bagnon-master/Bagnon/Bagnon.toc",
            "Bagnon-master/Bagnon/main.lua",
        ])
        .expect("archive holds addons");

        assert_eq!(folder_names(&plan), vec!["Bagnon"]);
        assert_eq!(
            plan.destination("Bagnon-master/Bagnon/main.lua").as_deref(),
            Some("Bagnon/main.lua")
        );
    }

    #[test]
    fn strips_every_level_above_the_shallowest_addon_folder() {
        let plan = plan_of(&[
            "release/forever/Questie/Questie.toc",
            "release/forever/Questie/init.lua",
        ])
        .expect("archive holds an addon");

        assert_eq!(folder_names(&plan), vec!["Questie"]);
        assert_eq!(
            plan.destination("release/forever/Questie/init.lua")
                .as_deref(),
            Some("Questie/init.lua")
        );
    }

    #[test]
    fn leaves_files_beside_the_wrapper_folder_uninstalled() {
        let plan = plan_of(&["Bagnon-master/README.md", "Bagnon-master/Bagnon/Bagnon.toc"])
            .expect("archive holds addons");

        assert_eq!(plan.destination("Bagnon-master/README.md"), None);
    }

    #[test]
    fn wraps_loose_root_files_in_a_folder_named_for_the_addon() {
        let plan = plan_of(&["Bagnon.toc", "main.lua"]).expect("archive holds an addon");

        assert_eq!(folder_names(&plan), vec!["Bagnon"]);
        assert_eq!(
            plan.destination("main.lua").as_deref(),
            Some("Bagnon/main.lua")
        );
    }

    #[test]
    fn ignores_folders_at_the_root_that_carry_no_toc() {
        let plan = plan_of(&["Bagnon/Bagnon.toc", "Screenshots/preview.png"])
            .expect("archive holds an addon");

        assert_eq!(folder_names(&plan), vec!["Bagnon"]);
        assert_eq!(plan.destination("Screenshots/preview.png"), None);
    }

    #[test]
    fn does_not_treat_a_nested_library_toc_as_a_separate_addon() {
        let plan = plan_of(&[
            "Bagnon/Bagnon.toc",
            "Bagnon/Libs/Ace3/Ace3.toc",
            "Bagnon/Libs/Ace3/Ace3.lua",
        ])
        .expect("archive holds an addon");

        assert_eq!(folder_names(&plan), vec!["Bagnon"]);
        assert_eq!(
            plan.destination("Bagnon/Libs/Ace3/Ace3.lua").as_deref(),
            Some("Bagnon/Libs/Ace3/Ace3.lua")
        );
    }

    #[test]
    fn drops_archiver_metadata_entries() {
        let plan = plan_of(&["Bagnon/Bagnon.toc", "__MACOSX/._Bagnon", "Bagnon/.DS_Store"])
            .expect("archive holds an addon");

        assert_eq!(plan.destination("__MACOSX/._Bagnon"), None);
        assert_eq!(plan.destination("Bagnon/.DS_Store"), None);
    }

    #[test]
    fn rejects_an_archive_with_no_toc_at_all() {
        let error = plan_of(&["notes.txt", "images/preview.png"]).expect_err("no addon");
        assert!(
            matches!(error, AppError::ArchiveHasNoAddon { .. }),
            "{error}"
        );
    }

    #[test]
    fn rejects_an_archive_entry_that_escapes_the_addons_directory() {
        let error =
            plan_of(&["../../evil.toc", "Bagnon/Bagnon.toc"]).expect_err("traversal is refused");
        assert!(
            matches!(error, AppError::ArchiveUnsafePath { .. }),
            "{error}"
        );
    }

    #[test]
    fn matches_toc_extensions_case_insensitively() {
        let plan = plan_of(&["Questie/Questie.TOC"]).expect("archive holds an addon");
        assert_eq!(folder_names(&plan), vec!["Questie"]);
    }

    #[test]
    fn sanitizes_a_catalog_name_into_a_loadable_folder_name() {
        assert_eq!(
            sanitize_folder_name("Details! DamageMeter"),
            "Details__DamageMeter"
        );
        assert_eq!(sanitize_folder_name("../evil"), "evil");
    }
}
