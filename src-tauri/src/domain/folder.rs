use std::fmt;

/// The name of one directory directly under `Interface/AddOns`.
///
/// This is the unit WoW loads and the unit this application installs and
/// deletes, so the type exists to make "a name that could escape the AddOns
/// directory" unrepresentable. Every deletion path takes an `AddonFolder`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, serde::Serialize)]
#[serde(transparent)]
pub struct AddonFolder(String);

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum FolderNameError {
    #[error("addon folder name is empty")]
    Empty,
    #[error("addon folder name {name:?} contains a path separator")]
    ContainsSeparator { name: String },
    #[error("addon folder name {name:?} is a relative path component")]
    RelativeComponent { name: String },
    #[error("addon folder name {name:?} contains a character WoW cannot load: {character:?}")]
    IllegalCharacter { name: String, character: char },
}

impl AddonFolder {
    pub fn new(name: impl Into<String>) -> Result<Self, FolderNameError> {
        let name = name.into();
        let trimmed = name.trim();

        if trimmed.is_empty() {
            return Err(FolderNameError::Empty);
        }
        if trimmed.contains('/') || trimmed.contains('\\') {
            return Err(FolderNameError::ContainsSeparator { name });
        }
        if trimmed == "." || trimmed == ".." {
            return Err(FolderNameError::RelativeComponent { name });
        }
        if let Some(character) = trimmed.chars().find(|c| c.is_control() || *c == '\0') {
            return Err(FolderNameError::IllegalCharacter { name, character });
        }

        Ok(Self(trimmed.to_owned()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// WoW's addon folder names are case-insensitive in practice; installed
    /// folders are matched against archive folders through this key.
    pub fn match_key(&self) -> String {
        self.0.to_ascii_lowercase()
    }
}

impl fmt::Display for AddonFolder {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl<'de> serde::Deserialize<'de> for AddonFolder {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let raw = String::deserialize(deserializer)?;
        Self::new(raw).map_err(serde::de::Error::custom)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_an_ordinary_addon_folder_name() {
        let folder = AddonFolder::new("Bagnon").expect("plain name is valid");
        assert_eq!(folder.as_str(), "Bagnon");
    }

    #[test]
    fn trims_surrounding_whitespace_from_archive_entries() {
        let folder = AddonFolder::new("  Details  ").expect("padded name is valid");
        assert_eq!(folder.as_str(), "Details");
    }

    #[test]
    fn rejects_names_that_could_escape_the_addons_directory() {
        for name in ["..", ".", "../evil", "evil/nested", "evil\\nested", "   "] {
            assert!(AddonFolder::new(name).is_err(), "{name} should be rejected");
        }
    }

    #[test]
    fn matches_folders_case_insensitively() {
        let installed = AddonFolder::new("WeakAuras").expect("valid");
        let archived = AddonFolder::new("weakauras").expect("valid");
        assert_eq!(installed.match_key(), archived.match_key());
    }

    #[test]
    fn rejects_a_deserialized_folder_name_with_a_separator() {
        let error = serde_json::from_str::<AddonFolder>("\"../etc\"");
        assert!(error.is_err());
    }
}
