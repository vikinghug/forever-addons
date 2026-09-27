import { useEffect, useRef, type KeyboardEvent, type RefObject } from "react";

import type {
  AddonSummary,
  BrowseQuery,
  InstalledView,
  SearchResults,
  SourceStatus,
} from "../types";
import { SOURCE_NAMES, addonKey, needsUpdate } from "../types";
import { AddonRow, type RowState } from "./AddonRow";
import { CategoryMenu, SortMenu, SourceToggles } from "./BrowseFilters";

interface Props {
  results: SearchResults;
  /** Sources that can answer a search right now, in the user's order. */
  browsable: SourceStatus[];
  query: BrowseQuery;
  installed: InstalledView;
  selected: string | null;
  busy: Record<string, string>;
  loading: boolean;
  onQuery: (change: Partial<BrowseQuery>) => void;
  onSelect: (addon: AddonSummary) => void;
  onGoToSources: () => void;
}

/** The left panel of the browse view: the search, then what it found. */
export function Browse({
  results,
  browsable,
  query,
  installed,
  selected,
  busy,
  loading,
  onQuery,
  onSelect,
  onGoToSources,
}: Props) {
  const search = useRef<HTMLInputElement>(null);
  const list = useRef<HTMLDivElement>(null);
  useSlashToSearch(search);

  const visible = query.hideInstalled
    ? results.addons.filter((addon) => stateOf(addon, installed) === "none")
    : results.addons;
  const hidden = results.addons.length - visible.length;
  const narrowed = query.category !== null || query.sources !== null || query.hideInstalled;

  return (
    <section className="browse-list" aria-label="Search addons">
      <div className="filters">
        <SearchField
          inputRef={search}
          text={query.text}
          onText={(text) => onQuery({ text })}
          onLeave={() => list.current?.querySelector<HTMLElement>(".row")?.focus()}
        />
        <SourceToggles
          browsable={browsable}
          selected={query.sources}
          counts={results.sources}
          onChange={(sources) => onQuery({ sources })}
        />
      </div>

      {browsable.length > 0 && (
        <div className="resultbar">
          <ResultCount
            shown={visible.length}
            of={narrowed ? results.text_matches : null}
            loading={loading}
          />
          <CategoryMenu
            selected={query.category}
            categories={results.categories}
            onChange={(category) => onQuery({ category })}
          />
          <SortMenu
            sort={query.sort}
            hideInstalled={query.hideInstalled}
            onSort={(sort) => onQuery({ sort })}
            onHideInstalled={(hideInstalled) => onQuery({ hideInstalled })}
          />
        </div>
      )}

      {results.notices.map((notice) => (
        <div className="banner" key={`${notice.source}:${notice.message}`}>
          <span className="banner-dot" aria-hidden="true">●</span>
          <span>
            {SOURCE_NAMES[notice.source]}: {notice.message}
          </span>
        </div>
      ))}

      <div className="scroller" ref={list}>
        {browsable.length === 0 ? (
          <NothingToBrowse onGoToSources={onGoToSources} />
        ) : visible.length === 0 ? (
          <NoMatches query={query} hidden={hidden} loading={loading} onQuery={onQuery} />
        ) : (
          <>
            <Rows
              addons={visible}
              query={query}
              installed={installed}
              selected={selected}
              busy={busy}
              onSelect={onSelect}
              onLeaveTop={() => search.current?.focus()}
            />
            {hidden > 0 && (
              <div className="list-end">
                <span>
                  {hidden} installed {hidden === 1 ? "addon" : "addons"} hidden.
                </span>
                <button
                  type="button"
                  className="linkish"
                  onClick={() => onQuery({ hideInstalled: false })}
                >
                  Show them
                </button>
              </div>
            )}
          </>
        )}
      </div>
    </section>
  );
}

function SearchField({
  inputRef,
  text,
  onText,
  onLeave,
}: {
  inputRef: RefObject<HTMLInputElement | null>;
  text: string;
  onText: (text: string) => void;
  /** Arrow down moves from the field into the results. */
  onLeave: () => void;
}) {
  const onKey = (event: KeyboardEvent<HTMLInputElement>) => {
    if (event.key === "Escape") {
      event.preventDefault();
      if (text) onText("");
      else event.currentTarget.blur();
    }
    if (event.key === "ArrowDown") {
      event.preventDefault();
      onLeave();
    }
  };

  return (
    <div className="field">
      <span className="field-icon" aria-hidden="true">⌕</span>
      <input
        ref={inputRef}
        id="browse-search"
        type="search"
        value={text}
        placeholder="Search name, author, or CurseForge ID"
        aria-label="Search addons"
        autoComplete="off"
        spellCheck={false}
        onChange={(event) => onText(event.target.value)}
        onKeyDown={onKey}
      />
      {text ? (
        <button
          type="button"
          className="field-clear"
          aria-label="Clear search"
          onClick={() => {
            onText("");
            inputRef.current?.focus();
          }}
        >
          ×
        </button>
      ) : (
        <kbd aria-hidden="true">/</kbd>
      )}
    </div>
  );
}

/** "12 addons", or "4 of 19" once a filter narrows what the text matched. */
function ResultCount({
  shown,
  of,
  loading,
}: {
  shown: number;
  of: number | null;
  loading: boolean;
}) {
  if (loading && shown === 0) return <span className="count">searching…</span>;

  return (
    <span className="count" data-busy={loading || undefined} aria-live="polite">
      <b>{shown}</b>
      {of !== null ? ` of ${of}` : shown === 1 ? " addon" : " addons"}
    </span>
  );
}

function Rows({
  addons,
  query,
  installed,
  selected,
  busy,
  onSelect,
  onLeaveTop,
}: {
  addons: AddonSummary[];
  query: BrowseQuery;
  installed: InstalledView;
  selected: string | null;
  busy: Record<string, string>;
  onSelect: (addon: AddonSummary) => void;
  onLeaveTop: () => void;
}) {
  const words = searchWords(query.text);

  // Arrow keys walk the rows; up from the first returns to the search field.
  const onKey = (event: KeyboardEvent<HTMLDivElement>) => {
    const row = (event.target as HTMLElement).closest<HTMLElement>(".row");
    if (!row || (event.key !== "ArrowDown" && event.key !== "ArrowUp")) return;
    event.preventDefault();

    const next = event.key === "ArrowDown" ? row.nextElementSibling : row.previousElementSibling;
    if (next instanceof HTMLElement) next.focus();
    else if (event.key === "ArrowUp") onLeaveTop();
  };

  return (
    <div className="rows" role="listbox" aria-label="Addons" onKeyDown={onKey}>
      {addons.map((addon) => (
        <AddonRow
          key={addonKey(addon.id)}
          addon={addon}
          state={stateOf(addon, installed)}
          selected={selected === addonKey(addon.id)}
          busy={busy[addonKey(addon.id)] ?? null}
          sortField={query.sort.field}
          words={words}
          onSelect={() => onSelect(addon)}
        />
      ))}
    </div>
  );
}

function NothingToBrowse({ onGoToSources }: { onGoToSources: () => void }) {
  return (
    <div className="state">
      <h2>Nothing to browse yet</h2>
      <p>
        CurseForge is searched live once its API key is saved. Wago and GitHub catalogs are
        pulled once and cached on disk until you refresh them; Wago needs a token first, and
        GitHub just needs a repository to track.
      </p>
      <button type="button" className="btn" data-variant="primary" onClick={onGoToSources}>
        Go to sources
      </button>
    </div>
  );
}

/** An empty list names what emptied it, with one button per filter. */
function NoMatches({
  query,
  hidden,
  loading,
  onQuery,
}: {
  query: BrowseQuery;
  hidden: number;
  loading: boolean;
  onQuery: (change: Partial<BrowseQuery>) => void;
}) {
  if (loading) {
    return (
      <div className="state">
        <h2>Searching…</h2>
        <p>Asking the sources.</p>
      </div>
    );
  }

  const text = query.text.trim();
  const filtered = query.category !== null || query.sources !== null || hidden > 0;

  return (
    <div className="state">
      <h2>{text ? `Nothing matches “${text}”` : "Nothing left to show"}</h2>
      <p>{filtered ? "Loosen one of these:" : "Try fewer or shorter words."}</p>
      <div className="state-actions">
        {text && filtered && (
          <button type="button" className="btn" onClick={() => onQuery({ text: "" })}>
            Clear search
          </button>
        )}
        {query.category !== null && (
          <button type="button" className="btn" onClick={() => onQuery({ category: null })}>
            Any category
          </button>
        )}
        {query.sources !== null && (
          <button type="button" className="btn" onClick={() => onQuery({ sources: null })}>
            All sources
          </button>
        )}
        {hidden > 0 && (
          <button type="button" className="btn" onClick={() => onQuery({ hideInstalled: false })}>
            Show {hidden} installed
          </button>
        )}
      </div>
    </div>
  );
}

/** `/` focuses the search from anywhere that is not already a text field. */
function useSlashToSearch(input: RefObject<HTMLInputElement | null>) {
  useEffect(() => {
    const onKey = (event: globalThis.KeyboardEvent) => {
      if (event.key !== "/" || event.ctrlKey || event.metaKey || event.altKey) return;
      const target = event.target as HTMLElement | null;
      if (target?.closest("input, textarea, select, [contenteditable='true']")) return;

      event.preventDefault();
      input.current?.focus();
      input.current?.select();
    };

    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [input]);
}

/** The words a search underlines. A bare number is a CurseForge ID, which
 * matches the key rather than any visible text. */
function searchWords(text: string): string[] {
  const trimmed = text.trim();
  if (/^\d+$/.test(trimmed)) return [];
  return trimmed.split(/\s+/).filter(Boolean);
}

/** What the row stripe and the action button say about an addon. */
export function stateOf(addon: AddonSummary, installed: InstalledView): RowState {
  const record = installed.managed.find((entry) => addonKey(entry.id) === addonKey(addon.id));
  if (!record) return "none";
  if (!record.present) return "broken";
  return needsUpdate(record.update) ? "outdated" : "installed";
}
