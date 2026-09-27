//! Addon descriptions: third-party HTML and Markdown, sanitized into markup
//! the UI can insert as-is.

use std::borrow::Cow;
use std::collections::{HashMap, HashSet};

use ammonia::{Builder, UrlRelative};
use pulldown_cmark::{Options, Parser};
use url::Url;

use crate::domain::AddonSummary;

/// Description markup that has been through [`Markup`]: allowlisted tags,
/// no styling, and every link and image an absolute http(s) URL.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(transparent)]
pub struct SafeHtml(String);

impl SafeHtml {
    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

/// Undoes a source's redirect wrapper around outbound links, or returns the
/// link unchanged.
pub type UnwrapLink = fn(&str) -> Cow<'_, str>;

/// Leaves links as the source wrote them.
pub fn keep_link(href: &str) -> Cow<'_, str> {
    Cow::Borrowed(href)
}

/// How to read one addon's description: its page is the base for relative
/// links, and its name and summary drive the cleanup and the fallback.
pub struct Markup<'a> {
    name: &'a str,
    summary: &'a str,
    links: Links,
}

/// Turns a description's links and image sources into absolute http(s) URLs.
/// Owned, because ammonia keeps its attribute filter for `'static`.
#[derive(Clone)]
struct Links {
    base: Option<Url>,
    unwrap: UnwrapLink,
}

const TAGS: [&str; 41] = [
    "a",
    "abbr",
    "b",
    "blockquote",
    "br",
    "caption",
    "code",
    "dd",
    "del",
    "details",
    "div",
    "dl",
    "dt",
    "em",
    "figcaption",
    "figure",
    "h1",
    "h2",
    "h3",
    "h4",
    "h5",
    "h6",
    "hr",
    "i",
    "img",
    "ins",
    "kbd",
    "li",
    "mark",
    "ol",
    "p",
    "pre",
    "s",
    "small",
    "strong",
    "sub",
    "summary",
    "sup",
    "table",
    "ul",
    "u",
];
const TABLE_TAGS: [&str; 5] = ["tbody", "td", "tfoot", "th", "thead"];
/// Removed with everything inside them, rather than unwrapped to their text.
const DROPPED_WITH_CONTENT: [&str; 11] = [
    "button", "embed", "iframe", "noscript", "object", "script", "select", "style", "template",
    "textarea", "title",
];
/// Blocks a source leaves empty (`<p>&nbsp;</p>`) purely as spacing.
const SPACER_BLOCKS: [(&str, &str); 7] = [
    ("<p>", "</p>"),
    ("<h1>", "</h1>"),
    ("<h2>", "</h2>"),
    ("<h3>", "</h3>"),
    ("<h4>", "</h4>"),
    ("<h5>", "</h5>"),
    ("<h6>", "</h6>"),
];
/// The headings a description opens with when it restates the addon's name.
const TITLE_HEADINGS: [(&str, &str); 3] = [("<h1>", "</h1>"), ("<h2>", "</h2>"), ("<h3>", "</h3>")];

impl<'a> Markup<'a> {
    pub fn new(summary: &'a AddonSummary, unwrap: UnwrapLink) -> Self {
        Self {
            name: &summary.name,
            summary: &summary.summary,
            links: Links {
                base: Url::parse(&summary.page_url).ok(),
                unwrap,
            },
        }
    }

    /// Markup that belongs to a page rather than to an addon's description —
    /// a file's changelog: nothing to fall back on and no title to drop.
    pub fn for_page(page_url: &str, unwrap: UnwrapLink) -> Markup<'static> {
        Markup {
            name: "",
            summary: "",
            links: Links {
                base: Url::parse(page_url).ok(),
                unwrap,
            },
        }
    }

    /// The visible text of `html` on one line, or `None` when it has none —
    /// for captions, which the UI renders as text rather than markup.
    pub fn text(&self, html: &str) -> Option<String> {
        let text = text_of(&self.sanitize(html));
        let line = text.split_whitespace().collect::<Vec<_>>().join(" ");
        (!line.is_empty()).then_some(line)
    }

    /// Sanitized `html`, or the plain summary when nothing is left of it.
    pub fn html(&self, html: &str) -> SafeHtml {
        let cleaned = self.sanitize(html);
        let cleaned = drop_spacers(&cleaned);
        let cleaned = drop_leading_title(&cleaned, self.name);

        if is_blank(cleaned) {
            return self.plain();
        }
        SafeHtml(cleaned.trim().to_owned())
    }

    /// `markdown` (GitHub-flavoured) rendered, then sanitized like [`Self::html`].
    pub fn markdown(&self, markdown: &str) -> SafeHtml {
        let options = Options::ENABLE_TABLES | Options::ENABLE_STRIKETHROUGH;
        let mut html = String::new();
        pulldown_cmark::html::push_html(&mut html, Parser::new_ext(markdown, options));
        self.html(&html)
    }

    /// The summary as one paragraph; nothing at all when it is blank too.
    pub fn plain(&self) -> SafeHtml {
        let text = self.summary.trim();
        if text.is_empty() {
            return SafeHtml(String::new());
        }
        SafeHtml(format!("<p>{}</p>", escape_text(text)))
    }

    fn sanitize(&self, html: &str) -> String {
        let links = self.links.clone();
        Builder::empty()
            .tags(TAGS.into_iter().chain(TABLE_TAGS).chain(["tr"]).collect())
            .clean_content_tags(DROPPED_WITH_CONTENT.into())
            .tag_attributes(allowed_attributes())
            .set_tag_attribute_value("img", "loading", "lazy")
            .url_schemes(HashSet::from(["http", "https"]))
            // Relative URLs survive the scheme check so `Links::rewrite` can
            // resolve them; it drops whatever it cannot make absolute.
            .url_relative(UrlRelative::PassThrough)
            .link_rel(None)
            .attribute_filter(move |element, attribute, value| {
                links.rewrite(element, attribute, value)
            })
            .clean(html)
            .to_string()
    }
}

impl Links {
    /// Runs after ammonia's scheme check, which only saw the raw value, so the
    /// rewritten URL is validated again here.
    fn rewrite<'v>(&self, element: &str, attribute: &str, value: &'v str) -> Option<Cow<'v, str>> {
        match (element, attribute) {
            ("a", "href") => self.resolve(value).map(|url| Cow::Owned(url.into())),
            ("img", "src") => self
                .resolve(value)
                .and_then(upgrade_to_https)
                .map(Cow::Owned),
            _ => Some(Cow::Borrowed(value)),
        }
    }

    /// An absolute http(s) URL, or `None` for anything that should not open:
    /// fragments, `mailto:`, `javascript:`, or a relative link with no base.
    fn resolve(&self, raw: &str) -> Option<Url> {
        let target = (self.unwrap)(raw.trim());
        if target.is_empty() || target.starts_with('#') {
            return None;
        }

        let url = match &self.base {
            Some(base) => base.join(&target).ok()?,
            None => Url::parse(&target).ok()?,
        };
        matches!(url.scheme(), "http" | "https").then_some(url)
    }
}

fn allowed_attributes() -> HashMap<&'static str, HashSet<&'static str>> {
    let cells = HashSet::from(["colspan", "rowspan"]);
    HashMap::from([
        ("a", HashSet::from(["href", "title"])),
        ("img", HashSet::from(["src", "alt", "title"])),
        ("ol", HashSet::from(["start"])),
        ("td", cells.clone()),
        ("th", cells),
    ])
}

/// The webview only loads images over https; most hosts serve both.
fn upgrade_to_https(mut url: Url) -> Option<String> {
    if url.scheme() == "http" && url.set_scheme("https").is_err() {
        return None;
    }
    Some(url.into())
}

/// Removes empty spacing blocks from sanitized markup.
///
/// Relies on ammonia's serialization: the blocks carry no attributes, text
/// `<` is always escaped, and a `<p>` or heading never nests another of its
/// kind, so the first closing tag after an opening one is its own.
fn drop_spacers(html: &str) -> String {
    let mut kept = String::with_capacity(html.len());
    let mut rest = html;

    while let Some(start) = rest.find('<') {
        kept.push_str(&rest[..start]);
        rest = &rest[start..];

        match skip_spacer(rest) {
            Some(after) => rest = after,
            None => {
                kept.push('<');
                rest = &rest[1..];
            }
        }
    }
    kept.push_str(rest);
    kept
}

/// The markup after a spacer block that `html` starts with.
fn skip_spacer(html: &str) -> Option<&str> {
    SPACER_BLOCKS.iter().find_map(|(open, close)| {
        let (inner, after) = split_block(html, open, close)?;
        is_blank(inner).then_some(after)
    })
}

/// `html` without a leading heading whose text is the addon's name; the
/// detail pane already shows it.
fn drop_leading_title<'h>(html: &'h str, name: &str) -> &'h str {
    let html = html.trim_start();

    TITLE_HEADINGS
        .iter()
        .find_map(|(open, close)| {
            let (inner, after) = split_block(html, open, close)?;
            same_words(&text_of(inner), name).then_some(after)
        })
        .unwrap_or(html)
}

/// The content of the `open`…`close` block `html` starts with, and the
/// markup after it.
fn split_block<'h>(html: &'h str, open: &str, close: &str) -> Option<(&'h str, &'h str)> {
    let body = html.strip_prefix(open)?;
    let end = body.find(close)?;
    Some((&body[..end], &body[end + close.len()..]))
}

/// Equal ignoring ASCII case and how the words are spaced.
fn same_words(a: &str, b: &str) -> bool {
    a.split_whitespace()
        .map(str::to_ascii_lowercase)
        .eq(b.split_whitespace().map(str::to_ascii_lowercase))
}

/// No visible text and no image.
fn is_blank(html: &str) -> bool {
    text_of(html).trim().is_empty() && !html.contains("<img")
}

/// The text of sanitized markup: tags removed, the entities ammonia's
/// serializer emits for text decoded.
fn text_of(html: &str) -> String {
    let mut text = String::with_capacity(html.len());
    let mut rest = html;

    while let Some(start) = rest.find('<') {
        text.push_str(&rest[..start]);
        let Some(end) = rest[start..].find('>') else {
            break;
        };
        rest = &rest[start + end + 1..];
    }
    text.push_str(rest);

    text.replace("&nbsp;", " ")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&amp;", "&")
}

fn escape_text(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{AddonId, AddonKey, Download, SourceId};

    fn addon(page_url: &str) -> AddonSummary {
        AddonSummary {
            id: AddonId::new(
                SourceId::CurseForge,
                AddonKey::new("1032100").expect("non-empty key"),
            ),
            name: "Questie Forever".to_owned(),
            summary: "Quests on the map & minimap.".to_owned(),
            author: None,
            version: None,
            updated_at: None,
            icon_url: None,
            page_url: page_url.to_owned(),
            categories: Vec::new(),
            downloads: None,
            expansions: Vec::new(),
            download: Download::Brokered,
        }
    }

    fn html(input: &str) -> String {
        let addon = addon("https://www.curseforge.com/wow/addons/questie");
        Markup::new(&addon, keep_link)
            .html(input)
            .as_str()
            .to_owned()
    }

    #[test]
    fn resolves_links_and_images_against_the_addon_page() {
        let cases = [
            (
                r#"<a href="/wow/addons/questie/pages/faq">FAQ</a>"#,
                r#"<a href="https://www.curseforge.com/wow/addons/questie/pages/faq">FAQ</a>"#,
            ),
            (
                r#"<a href="https://github.com/Questie">GitHub</a>"#,
                r#"<a href="https://github.com/Questie">GitHub</a>"#,
            ),
            (
                r#"<img src="//media.forgecdn.net/a.png">"#,
                r#"<img src="https://media.forgecdn.net/a.png" loading="lazy">"#,
            ),
            (
                r#"<img src="http://i.imgur.com/a.png" width="600" alt="map">"#,
                r#"<img src="https://i.imgur.com/a.png" alt="map" loading="lazy">"#,
            ),
        ];

        for (input, expected) in cases {
            assert_eq!(html(input), expected, "input: {input}");
        }
    }

    #[test]
    fn drops_links_that_should_not_open() {
        let cases = [
            r#"<a href="javascript:alert(1)">x</a>"#,
            r##"<a href="#options">x</a>"##,
            r#"<a href="mailto:author@example.com">x</a>"#,
            r#"<a href="data:text/html,hi">x</a>"#,
        ];

        for input in cases {
            assert_eq!(html(input), "<a>x</a>", "input: {input}");
        }
    }

    #[test]
    fn drops_a_relative_link_when_the_page_url_is_unusable() {
        let addon = addon("");
        let safe = Markup::new(&addon, keep_link).html(r#"<a href="/faq">FAQ</a>"#);

        assert_eq!(safe.as_str(), "<a>FAQ</a>");
    }

    #[test]
    fn validates_a_link_after_unwrapping_it() {
        fn to_script(_: &str) -> Cow<'_, str> {
            Cow::Borrowed("javascript:alert(1)")
        }
        let addon = addon("https://www.curseforge.com/wow/addons/questie");
        let safe = Markup::new(&addon, to_script).html(r#"<a href="/linkout">x</a>"#);

        assert_eq!(safe.as_str(), "<a>x</a>");
    }

    #[test]
    fn removes_scripts_styles_and_inline_styling() {
        let input = r#"<script>alert("never runs")</script><style>p{color:red}</style>
<p style="color:#808080" class="big" onclick="x()"><span style="font-size:18px">Styled</span></p>
<iframe src="https://youtube.com/embed/x">fallback</iframe>"#;

        assert_eq!(html(input), "<p>Styled</p>");
    }

    #[test]
    fn drops_spacer_blocks() {
        let input = "<p>&nbsp;</p><p>Kept</p><p> <br> </p><h2><strong>&nbsp;</strong></h2><p><img src=\"https://a.io/x.png\"></p>";

        assert_eq!(
            html(input),
            r#"<p>Kept</p><p><img src="https://a.io/x.png" loading="lazy"></p>"#
        );
    }

    #[test]
    fn drops_a_leading_heading_that_repeats_the_name() {
        assert_eq!(
            html("<p>&nbsp;</p>\n<h1><strong>Questie  Forever</strong></h1><p>Tracks quests.</p>"),
            "<p>Tracks quests.</p>"
        );
        assert_eq!(
            html("<h1>questie forever</h1><h2>Download</h2>"),
            "<h2>Download</h2>"
        );
    }

    #[test]
    fn keeps_a_leading_heading_that_is_not_the_name() {
        assert_eq!(
            html("<h2>Download</h2><p>Questie Forever</p>"),
            "<h2>Download</h2><p>Questie Forever</p>"
        );
    }

    #[test]
    fn falls_back_to_the_escaped_summary_when_nothing_is_left() {
        for input in [
            "",
            "  ",
            "<p>&nbsp;</p>",
            "<h1>Questie Forever</h1>",
            "<script>x</script>",
        ] {
            assert_eq!(
                html(input),
                "<p>Quests on the map &amp; minimap.</p>",
                "input: {input:?}"
            );
        }
    }

    #[test]
    fn is_empty_when_both_description_and_summary_are_blank() {
        let mut addon = addon("https://github.com/o/r");
        addon.summary = " ".to_owned();

        assert!(Markup::new(&addon, keep_link).html("").is_empty());
    }

    #[test]
    fn renders_markdown_release_notes() {
        let addon = addon("https://github.com/Questie/Questie/releases/tag/v1.4.0");
        let notes = "## Changes\n\n- Fixed the `/guide` command\n- Added **Forever** support\n\n\
                     [Compare](https://github.com/Questie/Questie/compare/v1.3...v1.4)\n\n\
                     <script>alert(1)</script>";

        assert_eq!(
            Markup::new(&addon, keep_link).markdown(notes).as_str(),
            "<h2>Changes</h2>\n<ul>\n<li>Fixed the <code>/guide</code> command</li>\n\
             <li>Added <strong>Forever</strong> support</li>\n</ul>\n\
             <p><a href=\"https://github.com/Questie/Questie/compare/v1.3...v1.4\">Compare</a></p>"
        );
    }

    #[test]
    fn keeps_tables_from_markdown() {
        let addon = addon("https://github.com/o/r");
        let safe = Markup::new(&addon, keep_link).markdown("| a | b |\n|---|---|\n| 1 | 2 |");

        assert!(
            safe.as_str().starts_with("<table><thead><tr><th>a</th>"),
            "{}",
            safe.as_str()
        );
    }

    #[test]
    fn reads_a_caption_as_one_line_of_text() {
        let markup = Markup::for_page("https://www.curseforge.com/wow/addons/", keep_link);

        assert_eq!(
            markup
                .text("<p>Some raid\n warnings &amp; <b>timers</b>\n</p>")
                .as_deref(),
            Some("Some raid warnings & timers")
        );
        assert_eq!(markup.text("<p>&nbsp;</p>"), None);
    }

    #[test]
    fn keeps_a_changelog_heading_that_a_description_would_drop() {
        let markup = Markup::for_page("https://www.curseforge.com/wow/addons/", keep_link);

        assert_eq!(
            markup.html("<h2>Questie</h2><p>Fixes</p>").as_str(),
            "<h2>Questie</h2><p>Fixes</p>"
        );
    }
}
