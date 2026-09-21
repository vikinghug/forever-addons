//! Parsing of `.toc` files — the manifest WoW reads to load an addon.
//!
//! The format is line-oriented: `## Key: Value` directives, then a list of
//! files. Addon authors put anything they like in the key space, so unknown
//! directives are preserved rather than discarded.

use crate::domain::AddonVersion;

/// The `## Interface:` number. The WoW Forever beta (client 1.60.1) loads
/// `16001`.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, serde::Serialize, serde::Deserialize,
)]
#[serde(transparent)]
pub struct InterfaceVersion(u32);

impl InterfaceVersion {
    /// What the WoW Forever beta client reports.
    pub const FOREVER: Self = Self(16001);

    pub const fn new(raw: u32) -> Self {
        Self(raw)
    }

    pub const fn get(self) -> u32 {
        self.0
    }

    /// Whether the Forever client loads an addon declaring this number
    /// without the "out of date" gate. Forever numbers its client 1.60.x, so
    /// its interface band starts at 16000 and runs until a 2.x would begin.
    pub const fn loads_on_forever(self) -> bool {
        self.0 >= 16000 && self.0 < 20000
    }
}

/// One `## Key: Value` line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TocDirective {
    Interface(InterfaceVersion),
    Title(String),
    Version(AddonVersion),
    Author(String),
    Notes(String),
    /// Anything else the author wrote, kept verbatim: `X-Curse-Project-ID`,
    /// `Dependencies`, localized `Title-deDE`, and whatever comes next.
    Unknown {
        key: String,
        value: String,
    },
}

/// A parsed `.toc` file. Only the directives are modelled; the file list below
/// them is WoW's business, not this application's.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Toc {
    pub directives: Vec<TocDirective>,
}

impl Toc {
    pub fn parse(contents: &str) -> Self {
        let directives = contents
            .lines()
            .flat_map(parse_directive_line)
            .collect::<Vec<_>>();

        Self { directives }
    }

    pub fn interface(&self) -> Option<InterfaceVersion> {
        self.interfaces().first().copied()
    }

    /// Every interface number the addon declares. Modern clients accept a
    /// comma-delimited `## Interface:` list, one number per supported client.
    pub fn interfaces(&self) -> Vec<InterfaceVersion> {
        self.directives
            .iter()
            .filter_map(|directive| match directive {
                TocDirective::Interface(version) => Some(*version),
                _ => None,
            })
            .collect()
    }

    /// The `## Title:` line with WoW's `|cffRRGGBB … |r` colour escapes removed,
    /// because they render as noise outside the game client.
    pub fn title(&self) -> Option<String> {
        self.directives
            .iter()
            .find_map(|directive| match directive {
                TocDirective::Title(title) => Some(strip_color_escapes(title)),
                _ => None,
            })
    }

    pub fn version(&self) -> Option<&AddonVersion> {
        self.directives
            .iter()
            .find_map(|directive| match directive {
                TocDirective::Version(version) => Some(version),
                _ => None,
            })
    }

    pub fn author(&self) -> Option<&str> {
        self.directives
            .iter()
            .find_map(|directive| match directive {
                TocDirective::Author(author) => Some(author.as_str()),
                _ => None,
            })
    }

    pub fn notes(&self) -> Option<&str> {
        self.directives
            .iter()
            .find_map(|directive| match directive {
                TocDirective::Notes(notes) => Some(notes.as_str()),
                _ => None,
            })
    }
}

fn parse_directive_line(line: &str) -> Vec<TocDirective> {
    let Some(body) = line.trim().strip_prefix("##") else {
        return Vec::new();
    };
    let Some((key, value)) = body.split_once(':') else {
        return Vec::new();
    };
    let key = key.trim();
    let value = value.trim();

    if key.is_empty() {
        return Vec::new();
    }

    match key.to_ascii_lowercase().as_str() {
        "interface" => parse_interface_list(key, value),
        "title" => vec![TocDirective::Title(value.to_owned())],
        "version" => vec![
            AddonVersion::new(value)
                .map(TocDirective::Version)
                .unwrap_or_else(|| unknown(key, value)),
        ],
        "author" => vec![TocDirective::Author(value.to_owned())],
        "notes" => vec![TocDirective::Notes(value.to_owned())],
        _ => vec![unknown(key, value)],
    }
}

/// One `Interface` directive per declared number. A list any entry of which
/// does not parse is preserved verbatim as `Unknown` rather than half-read.
fn parse_interface_list(key: &str, value: &str) -> Vec<TocDirective> {
    let numbers = value
        .split(',')
        .map(|entry| entry.trim().parse::<u32>())
        .collect::<std::result::Result<Vec<_>, _>>();

    match numbers {
        Ok(numbers) if !numbers.is_empty() => numbers
            .into_iter()
            .map(|number| TocDirective::Interface(InterfaceVersion::new(number)))
            .collect(),
        _ => vec![unknown(key, value)],
    }
}

fn unknown(key: &str, value: &str) -> TocDirective {
    TocDirective::Unknown {
        key: key.to_owned(),
        value: value.to_owned(),
    }
}

/// Removes `|cffRRGGBB` openers and `|r` terminators, leaving the visible text.
fn strip_color_escapes(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len());
    let mut chars = raw.chars().peekable();

    while let Some(character) = chars.next() {
        if character != '|' {
            out.push(character);
            continue;
        }

        match chars.next() {
            // `|cAARRGGBB` — the eight hex digits are the colour.
            Some('c' | 'C') => skip_hex_digits(&mut chars, 8),
            Some('r' | 'R') => {}
            // `||` is a literal pipe; anything else is left as written.
            Some('|') => out.push('|'),
            Some(other) => {
                out.push('|');
                out.push(other);
            }
            None => out.push('|'),
        }
    }

    out.trim().to_owned()
}

fn skip_hex_digits(chars: &mut std::iter::Peekable<std::str::Chars<'_>>, count: usize) {
    for _ in 0..count {
        if !chars.peek().is_some_and(char::is_ascii_hexdigit) {
            return;
        }
        chars.next();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const BAGNON: &str = "## Interface: 16001\n\
        ## Title: Bagnon\n\
        ## Version: 2.13.3\n\
        ## Author: Tuller\n\
        ## Notes: Displays your bags as a single inventory\n\
        ## X-Curse-Project-ID: 3541\n\
        ## SavedVariables: BagnonDB\n\
        \n\
        main.lua\n\
        core\\bagnon.lua\n";

    #[test]
    fn reads_the_known_directives_from_a_real_toc() {
        let toc = Toc::parse(BAGNON);

        assert_eq!(toc.interface(), Some(InterfaceVersion::FOREVER));
        assert_eq!(toc.title().as_deref(), Some("Bagnon"));
        assert_eq!(toc.version().map(AddonVersion::as_str), Some("2.13.3"));
        assert_eq!(toc.author(), Some("Tuller"));
    }

    #[test]
    fn preserves_unknown_directives_instead_of_dropping_them() {
        let toc = Toc::parse(BAGNON);

        assert!(toc.directives.contains(&TocDirective::Unknown {
            key: "X-Curse-Project-ID".to_owned(),
            value: "3541".to_owned(),
        }));
        assert!(toc.directives.contains(&TocDirective::Unknown {
            key: "SavedVariables".to_owned(),
            value: "BagnonDB".to_owned(),
        }));
    }

    #[test]
    fn preserves_an_unparseable_interface_number_as_unknown() {
        let toc = Toc::parse("## Interface: 16001a\n");

        assert_eq!(toc.interface(), None);
        assert_eq!(
            toc.directives,
            vec![TocDirective::Unknown {
                key: "Interface".to_owned(),
                value: "16001a".to_owned(),
            }]
        );
    }

    #[test]
    fn ignores_file_lines_and_comments_without_a_key() {
        let toc = Toc::parse("# plain comment\nmain.lua\n## \n##NoColon\n");
        assert!(toc.directives.is_empty());
    }

    #[test]
    fn strips_color_escapes_from_a_title() {
        let toc = Toc::parse("## Title: |cff00ff00Deadly|r Boss Mods\n");
        assert_eq!(toc.title().as_deref(), Some("Deadly Boss Mods"));
    }

    #[test]
    fn keeps_an_escaped_literal_pipe_in_a_title() {
        let toc = Toc::parse("## Title: Damage || Healing\n");
        assert_eq!(toc.title().as_deref(), Some("Damage | Healing"));
    }

    #[test]
    fn accepts_only_the_forever_interface_band() {
        assert!(InterfaceVersion::FOREVER.loads_on_forever());
        assert!(InterfaceVersion::new(16000).loads_on_forever());
        assert!(!InterfaceVersion::new(11509).loads_on_forever());
        assert!(!InterfaceVersion::new(110200).loads_on_forever());
    }

    #[test]
    fn reads_every_number_of_a_comma_delimited_interface_list() {
        let toc = Toc::parse("## Interface: 110200, 16001\n");

        assert_eq!(
            toc.interfaces(),
            vec![InterfaceVersion::new(110200), InterfaceVersion::FOREVER]
        );
        assert_eq!(toc.interface(), Some(InterfaceVersion::new(110200)));
    }

    #[test]
    fn preserves_a_half_unparseable_interface_list_as_unknown() {
        let toc = Toc::parse("## Interface: 16001, next\n");

        assert!(toc.interfaces().is_empty());
        assert_eq!(
            toc.directives,
            vec![TocDirective::Unknown {
                key: "Interface".to_owned(),
                value: "16001, next".to_owned(),
            }]
        );
    }
}
