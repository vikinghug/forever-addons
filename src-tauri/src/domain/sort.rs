//! The orders the browse list can be put in.
//!
//! Only fields every source can fill are offered. CurseForge alone ranks by
//! popularity or rating, and a merged list cannot pretend Wago and GitHub
//! share those, so they are left out.

use std::cmp::Ordering;

use time::OffsetDateTime;
use time::format_description::well_known::Rfc3339;

use crate::domain::AddonSummary;

/// What the browse list is ordered by.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SortField {
    #[default]
    Downloads,
    Updated,
    Name,
    Author,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SortDirection {
    Ascending,
    #[default]
    Descending,
}

/// A browse order. The default, most downloaded first, is what makes an
/// unfiltered list useful.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, serde::Deserialize)]
#[serde(default)]
pub struct Sort {
    pub field: SortField,
    pub direction: SortDirection,
}

impl Sort {
    /// Orders two addons by the field in the chosen direction.
    ///
    /// An addon missing the field goes last whichever way the list runs: an
    /// unknown download count is not "fewest downloads". Ties fall back to
    /// name, then id, so the order is stable across sources and refreshes.
    pub fn compare(self, left: &AddonSummary, right: &AddonSummary) -> Ordering {
        let by_field = match self.field {
            SortField::Downloads => self.compare_present(left.downloads, right.downloads),
            SortField::Updated => self.compare_present(updated_at(left), updated_at(right)),
            SortField::Name => self.direction.apply(name_key(left).cmp(&name_key(right))),
            SortField::Author => self.compare_present(author_key(left), author_key(right)),
        };

        by_field
            .then_with(|| name_key(left).cmp(&name_key(right)))
            .then_with(|| left.id.to_string().cmp(&right.id.to_string()))
    }

    fn compare_present<T: Ord>(self, left: Option<T>, right: Option<T>) -> Ordering {
        match (left, right) {
            (Some(left), Some(right)) => self.direction.apply(left.cmp(&right)),
            (Some(_), None) => Ordering::Less,
            (None, Some(_)) => Ordering::Greater,
            (None, None) => Ordering::Equal,
        }
    }
}

impl SortDirection {
    fn apply(self, ascending: Ordering) -> Ordering {
        match self {
            Self::Ascending => ascending,
            Self::Descending => ascending.reverse(),
        }
    }
}

/// A date that does not parse counts as missing rather than as the epoch.
fn updated_at(addon: &AddonSummary) -> Option<OffsetDateTime> {
    let raw = addon.updated_at.as_deref()?;
    OffsetDateTime::parse(raw, &Rfc3339).ok()
}

fn name_key(addon: &AddonSummary) -> String {
    addon.name.to_lowercase()
}

fn author_key(addon: &AddonSummary) -> Option<String> {
    let author = addon.author.as_deref()?.trim();
    (!author.is_empty()).then(|| author.to_lowercase())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{AddonId, AddonKey, Download, SourceId};

    fn addon(key: &str, name: &str) -> AddonSummary {
        AddonSummary {
            id: AddonId::new(SourceId::Wago, AddonKey::new(key).expect("non-empty")),
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
            download: Download::Brokered,
        }
    }

    fn fixtures() -> Vec<AddonSummary> {
        let mut bagnon = addon("1", "Bagnon");
        bagnon.author = Some("Tuller".to_owned());
        bagnon.downloads = Some(500);
        bagnon.updated_at = Some("2026-09-01T00:00:00Z".to_owned());

        let mut questie = addon("2", "questie");
        questie.author = Some("aero".to_owned());
        questie.downloads = Some(9_000);
        questie.updated_at = Some("2026-09-20T12:30:00.250Z".to_owned());

        let mut dbm = addon("3", "Deadly Boss Mods");
        dbm.author = Some("MysticalOS".to_owned());
        dbm.downloads = Some(50_000);
        dbm.updated_at = Some("2026-09-10T00:00:00+02:00".to_owned());

        let mut unknown = addon("4", "Atlas");
        unknown.updated_at = Some("last tuesday".to_owned());

        vec![bagnon, questie, dbm, unknown]
    }

    fn sorted(field: SortField, direction: SortDirection) -> Vec<String> {
        let sort = Sort { field, direction };
        let mut addons = fixtures();
        addons.sort_by(|left, right| sort.compare(left, right));
        addons.into_iter().map(|addon| addon.name).collect()
    }

    #[test]
    fn orders_by_each_field_in_both_directions() {
        use SortDirection::{Ascending, Descending};
        use SortField::{Author, Downloads, Name, Updated};

        let cases = [
            (
                Downloads,
                Descending,
                ["Deadly Boss Mods", "questie", "Bagnon", "Atlas"],
            ),
            (
                Downloads,
                Ascending,
                ["Bagnon", "questie", "Deadly Boss Mods", "Atlas"],
            ),
            (
                Updated,
                Descending,
                ["questie", "Deadly Boss Mods", "Bagnon", "Atlas"],
            ),
            (
                Updated,
                Ascending,
                ["Bagnon", "Deadly Boss Mods", "questie", "Atlas"],
            ),
            (
                Name,
                Ascending,
                ["Atlas", "Bagnon", "Deadly Boss Mods", "questie"],
            ),
            (
                Name,
                Descending,
                ["questie", "Deadly Boss Mods", "Bagnon", "Atlas"],
            ),
            (
                Author,
                Ascending,
                ["questie", "Deadly Boss Mods", "Bagnon", "Atlas"],
            ),
            (
                Author,
                Descending,
                ["Bagnon", "Deadly Boss Mods", "questie", "Atlas"],
            ),
        ];

        for (field, direction, expected) in cases {
            assert_eq!(
                sorted(field, direction),
                expected,
                "{field:?} {direction:?}"
            );
        }
    }

    #[test]
    fn keeps_missing_values_last_in_both_directions() {
        let directions = [SortDirection::Ascending, SortDirection::Descending];
        let fields = [SortField::Downloads, SortField::Updated, SortField::Author];
        let cases = directions
            .into_iter()
            .flat_map(|direction| fields.map(|field| (field, direction)));

        for (field, direction) in cases {
            let names = sorted(field, direction);
            let last = names.last().map(String::as_str);
            assert_eq!(last, Some("Atlas"), "{field:?} {direction:?}");
        }
    }

    #[test]
    fn breaks_ties_by_name_whatever_the_direction() {
        let sort = Sort {
            field: SortField::Downloads,
            direction: SortDirection::Descending,
        };
        let mut addons = [addon("1", "Zygor"), addon("2", "Auctionator")];
        addons.sort_by(|left, right| sort.compare(left, right));

        assert_eq!(addons[0].name, "Auctionator");
    }

    #[test]
    fn defaults_to_most_downloaded_first() {
        let sort: Sort = serde_json::from_str("{}").expect("empty sort parses");
        assert_eq!(
            sort,
            Sort {
                field: SortField::Downloads,
                direction: SortDirection::Descending,
            }
        );
    }
}
