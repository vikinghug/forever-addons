use std::fmt;

/// The game a source labels an addon for.
///
/// Every source this build queries is asked for World of Warcraft: Forever,
/// but sources can tag addons with other games' labels too; anything this
/// build does not recognise is preserved as `Unknown` rather than dropped.
#[derive(Debug, Clone, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[serde(tag = "kind", content = "label", rename_all = "kebab-case")]
pub enum Expansion {
    /// World of Warcraft: Forever — client 1.60.x.
    Forever,
    Unknown(String),
}

impl fmt::Display for Expansion {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Forever => f.write_str("WoW Forever"),
            Self::Unknown(label) => f.write_str(label),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serializes_forever_under_a_stable_kind() {
        let json = serde_json::to_string(&Expansion::Forever).expect("serializes");
        assert_eq!(json, r#"{"kind":"forever"}"#);
    }

    #[test]
    fn preserves_an_unrecognised_label_verbatim() {
        let round_tripped: Expansion =
            serde_json::from_str(r#"{"kind":"unknown","label":"Midnight"}"#).expect("parses");
        assert_eq!(round_tripped, Expansion::Unknown("Midnight".to_owned()));
    }
}
