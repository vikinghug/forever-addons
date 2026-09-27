import type { AddonSummary, SortField } from "../types";
import { SOURCE_NAMES } from "../types";
import { ago, count, initials } from "../format";

/** What the left stripe on a row encodes. */
export type RowState = "none" | "installed" | "outdated" | "broken";

interface Props {
  addon: AddonSummary;
  state: RowState;
  selected: boolean;
  busy: string | null;
  /** The row's stat shows the value the list is ordered by. */
  sortField: SortField;
  /** Search words to underline in the name and author. */
  words: string[];
  onSelect: () => void;
}

/** One search result. Actions live in the detail pane; the row only says where things stand. */
export function AddonRow({ addon, state, selected, busy, sortField, words, onSelect }: Props) {
  return (
    <div
      className="row"
      data-state={state}
      role="option"
      aria-selected={selected}
      tabIndex={0}
      onClick={onSelect}
      onKeyDown={(event) => {
        if (event.key === "Enter" || event.key === " ") {
          event.preventDefault();
          onSelect();
        }
      }}
    >
      <span className="row-icon" aria-hidden="true">
        {addon.icon_url ? <img src={addon.icon_url} alt="" loading="lazy" /> : initials(addon.name)}
      </span>

      <span className="row-body">
        <span className="row-head">
          <span className="row-name">
            <Highlight text={addon.name} words={words} />
          </span>
          {addon.author && (
            <span className="row-author" data-sorted={sortField === "author" || undefined}>
              by <Highlight text={addon.author} words={words} />
            </span>
          )}
        </span>
        <span className="row-summary">{addon.summary || "No description published."}</span>
        <span className="row-meta">
          <span className="chip">{SOURCE_NAMES[addon.id.source]}</span>
          {addon.categories.slice(0, 1).map((category) => (
            <span className="chip" key={category}>
              {category}
            </span>
          ))}
          <SortStat addon={addon} field={sortField} />
          <RowTag addon={addon} state={state} busy={busy} />
        </span>
      </span>
    </div>
  );
}

function SortStat({ addon, field }: { addon: AddonSummary; field: SortField }) {
  if (field === "updated") {
    return (
      <span className="stat" data-sorted title="Last updated">
        {ago(addon.updated_at)}
      </span>
    );
  }

  return (
    <span className="stat" data-sorted={field === "downloads" || undefined} title="Downloads">
      {count(addon.downloads)}
    </span>
  );
}

/** Underlines each search word where it appears; a match inside a longer
 * word still counts, as it does for the search itself. */
function Highlight({ text, words }: { text: string; words: string[] }) {
  if (words.length === 0) return text;

  const longestFirst = [...words].sort((a, b) => b.length - a.length);
  const pattern = new RegExp(`(${longestFirst.map(escapeRegExp).join("|")})`, "gi");

  // Splitting on a capturing group puts every match at an odd index.
  return text
    .split(pattern)
    .map((part, index) => (index % 2 === 1 ? <mark key={index}>{part}</mark> : part));
}

const escapeRegExp = (value: string) => value.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");

function RowTag({
  addon,
  state,
  busy,
}: {
  addon: AddonSummary;
  state: RowState;
  busy: string | null;
}) {
  if (busy) return <span className="tag" data-tone="mute">{busy}</span>;
  switch (state) {
    case "outdated":
      return <span className="tag" data-tone="frost">Update</span>;
    case "installed":
      return <span className="tag" data-tone="moss">Installed</span>;
    case "broken":
      return <span className="tag" data-tone="rust">Folders missing</span>;
    case "none":
      return <DownloadTag addon={addon} />;
  }
}

/** Flags, before the row is opened, an addon this app cannot fetch itself. */
function DownloadTag({ addon }: { addon: AddonSummary }) {
  switch (addon.download.kind) {
    case "direct":
    case "brokered":
      return null;
    case "external":
      return (
        <span
          className="tag"
          data-tone="gold"
          title="The author turned off downloads through apps. Download the zip from the addon's page, then install it from the details pane."
        >
          Download on site
        </span>
      );
    case "unsupported":
      return (
        <span
          className="tag"
          data-tone="mute"
          title={`Published as a .${addon.download.format} archive, which this app cannot unpack.`}
        >
          .{addon.download.format} only
        </span>
      );
  }
}
