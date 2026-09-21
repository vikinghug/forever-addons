use std::fmt;

/// A version string exactly as a source published it.
///
/// No source guarantees semver: one reports `2.13.3`, another often
/// carries the version only inside the archive filename, and `.toc` files carry
/// anything from `v1.16` to `10.2.5-1-gabc123`. Comparison is therefore
/// equality on a normalized form, never ordering — "different from what is
/// installed" is the only question this application asks.
#[derive(Debug, Clone, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[serde(transparent)]
pub struct AddonVersion(String);

impl AddonVersion {
    pub fn new(raw: impl Into<String>) -> Option<Self> {
        let raw = raw.into();
        let trimmed = raw.trim();
        (!trimmed.is_empty()).then(|| Self(trimmed.to_owned()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Ignores a leading `v` and case, so `v1.16` and `1.16` are one version.
    fn normalized(&self) -> &str {
        self.0
            .strip_prefix(['v', 'V'])
            .filter(|rest| rest.starts_with(|c: char| c.is_ascii_digit()))
            .unwrap_or(&self.0)
    }

    pub fn matches(&self, other: &Self) -> bool {
        self.normalized().eq_ignore_ascii_case(other.normalized())
    }
}

impl fmt::Display for AddonVersion {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn version(raw: &str) -> AddonVersion {
        AddonVersion::new(raw).expect("test versions are non-empty")
    }

    #[test]
    fn treats_a_blank_version_as_absent() {
        assert_eq!(AddonVersion::new("   "), None);
        assert_eq!(AddonVersion::new(""), None);
    }

    #[test]
    fn matches_across_a_leading_v_prefix() {
        assert!(version("v1.16").matches(&version("1.16")));
        assert!(version("V2.13.3").matches(&version("2.13.3")));
    }

    #[test]
    fn does_not_strip_a_v_that_starts_a_word() {
        assert!(!version("victory-1").matches(&version("ictory-1")));
    }

    #[test]
    fn distinguishes_genuinely_different_versions() {
        assert!(!version("1.16").matches(&version("1.17")));
    }
}
