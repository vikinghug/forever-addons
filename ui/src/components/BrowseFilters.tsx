import { Fragment, useEffect, useRef, useState, type KeyboardEvent } from "react";

import type { CategoryCount, Sort, SourceCount, SourceId, SourceStatus } from "../types";
import { SORT_OPTIONS, SOURCE_NAMES, sortName } from "../types";

/** The toggles are narrow; "Wago Addons" stays in the accessible name. */
const SHORT_NAMES: Record<SourceId, string> = {
  curseforge: "CurseForge",
  wago: "Wago",
  github: "GitHub",
};

/**
 * One toggle per browsable source, with how many matches each holds.
 *
 * `selected: null` means every source. The last lit toggle stays lit, so an
 * empty list always has a filter the user can see and undo.
 */
export function SourceToggles({
  browsable,
  selected,
  counts,
  onChange,
}: {
  browsable: SourceStatus[];
  selected: SourceId[] | null;
  counts: SourceCount[];
  onChange: (sources: SourceId[] | null) => void;
}) {
  if (browsable.length < 2) return null;

  const ids = browsable.map((source) => source.id);
  const included = (id: SourceId) => selected === null || selected.includes(id);
  const lit = ids.filter(included);

  const toggle = (id: SourceId) => {
    const next = included(id) ? lit.filter((entry) => entry !== id) : [...lit, id];
    if (next.length === 0) return;
    onChange(next.length === ids.length ? null : next);
  };

  return (
    <div className="sources" role="group" aria-label="Sources to include">
      {browsable.map((source) => {
        const on = included(source.id);
        // A live source that is switched off is never asked, so it has no count.
        const count = counts.find((entry) => entry.source === source.id)?.count;
        const title =
          on && lit.length === 1
            ? "At least one source stays on"
            : !on && count === undefined
              ? `${source.name} is searched live; switch it on to ask it`
              : undefined;

        return (
          <button
            key={source.id}
            type="button"
            className="src"
            aria-pressed={on}
            aria-label={`${SOURCE_NAMES[source.id]}${count === undefined ? "" : `, ${count}`}`}
            data-empty={(on && count === 0) || undefined}
            title={title}
            onClick={() => toggle(source.id)}
          >
            {SHORT_NAMES[source.id]}
            {count !== undefined && <span className="src-n">{count}</span>}
          </button>
        );
      })}
    </div>
  );
}

/** A filterable category menu with a count per category. Hidden while no
 * match has a category and none is picked. */
export function CategoryMenu({
  selected,
  categories,
  onChange,
}: {
  selected: string | null;
  categories: CategoryCount[];
  onChange: (category: string | null) => void;
}) {
  const { open, wrap, trigger, toggle, close } = useMenu();
  const [filter, setFilter] = useState("");
  const list = useRef<HTMLDivElement>(null);

  if (categories.length === 0 && selected === null) return null;

  const needle = filter.trim().toLowerCase();
  const shown = categories.filter((category) => category.name.toLowerCase().includes(needle));
  const isSelected = (name: string) => selected?.toLowerCase() === name.toLowerCase();
  // Switching a source off can take the picked category out of the matches.
  const selectedGone = selected !== null && !categories.some((category) => isSelected(category.name));

  const pick = (category: string | null) => {
    onChange(category);
    close();
  };

  const onFilterKey = (event: KeyboardEvent<HTMLInputElement>) => {
    if (event.key === "ArrowDown") {
      event.preventDefault();
      list.current?.querySelector<HTMLElement>(".cat")?.focus();
    }
    if (event.key === "Enter" && shown[0]) {
      event.preventDefault();
      pick(shown[0].name);
    }
  };

  return (
    <span className="pill-wrap" ref={wrap}>
      <button
        ref={trigger}
        type="button"
        className="pill"
        data-active={selected !== null || undefined}
        aria-haspopup="menu"
        aria-expanded={open}
        aria-label={selected ? `Category: ${selected}. Change` : "Filter by category"}
        onClick={() => {
          setFilter("");
          toggle();
        }}
      >
        <span className="pill-label">{selected ?? "Any category"}</span>
        <span className="pill-caret" aria-hidden="true">▾</span>
      </button>
      {selected !== null && (
        <button
          type="button"
          className="pill-x"
          aria-label={`Clear the ${selected} category`}
          onClick={() => onChange(null)}
        >
          ×
        </button>
      )}

      {open && (
        <div className="pop">
          <input
            autoFocus
            className="pop-filter"
            type="search"
            value={filter}
            placeholder="Filter categories"
            aria-label="Filter categories"
            autoComplete="off"
            onChange={(event) => setFilter(event.target.value)}
            onKeyDown={onFilterKey}
          />
          <div className="cat-list" role="menu" aria-label="Categories" ref={list} onKeyDown={moveFocus}>
            {!needle && (
              <CategoryOption label="Any category" checked={selected === null} onPick={() => pick(null)} />
            )}
            {shown.map((category) => (
              <CategoryOption
                key={category.name}
                label={category.name}
                count={category.count}
                checked={isSelected(category.name)}
                onPick={() => pick(category.name)}
              />
            ))}
            {selectedGone && !needle && (
              <CategoryOption label={selected} count={0} checked onPick={() => pick(selected)} />
            )}
            {needle && shown.length === 0 && (
              <div className="cat-none">No category matches “{filter.trim()}”.</div>
            )}
          </div>
          <div className="pop-foot">
            Only CurseForge publishes categories. Picking one leaves Wago and GitHub addons out.
          </div>
        </div>
      )}
    </span>
  );
}

function CategoryOption({
  label,
  count,
  checked,
  onPick,
}: {
  label: string;
  count?: number;
  checked: boolean;
  onPick: () => void;
}) {
  return (
    <button type="button" className="cat" role="menuitemradio" aria-checked={checked} onClick={onPick}>
      <span className="cat-tick" aria-hidden="true">
        {checked ? "✓" : ""}
      </span>
      <span>{label}</span>
      {count !== undefined && <span className="cat-n">{count}</span>}
    </button>
  );
}

/** The order, named in the field's own terms, plus the view's one toggle:
 * hiding what is already installed. */
export function SortMenu({
  sort,
  hideInstalled,
  onSort,
  onHideInstalled,
}: {
  sort: Sort;
  hideInstalled: boolean;
  onSort: (sort: Sort) => void;
  onHideInstalled: (hide: boolean) => void;
}) {
  const { open, wrap, trigger, toggle, close } = useMenu();

  return (
    <span className="pill-wrap" ref={wrap}>
      <button
        ref={trigger}
        type="button"
        className="pill"
        aria-haspopup="menu"
        aria-expanded={open}
        aria-label={`Order: ${sortName(sort)}. Change`}
        onClick={toggle}
      >
        <span className="pill-label">{sortName(sort)}</span>
        <span className="pill-caret" aria-hidden="true">▾</span>
      </button>

      {open && (
        <div className="pop">
          <div className="pop-head">Order by</div>
          <div className="sort-grid">
            {SORT_OPTIONS.map((option) => (
              <Fragment key={option.field}>
                <span className="sort-field" id={`sort-${option.field}`}>
                  {option.label}
                </span>
                <span className="seg" role="group" aria-labelledby={`sort-${option.field}`}>
                  {option.choices.map((choice) => {
                    const on = sort.field === option.field && sort.direction === choice.direction;
                    return (
                      <button
                        key={choice.direction}
                        type="button"
                        aria-pressed={on}
                        aria-label={choice.long}
                        autoFocus={on}
                        onClick={() => {
                          onSort({ field: option.field, direction: choice.direction });
                          close();
                        }}
                      >
                        {choice.short}
                      </button>
                    );
                  })}
                </span>
              </Fragment>
            ))}
          </div>
          <label className="check">
            <input
              type="checkbox"
              checked={hideInstalled}
              onChange={(event) => onHideInstalled(event.target.checked)}
            />
            Hide addons I already have
          </label>
        </div>
      )}
    </span>
  );
}

/** Open state for a pill's popover: a click outside it or Escape closes it,
 * and closing hands focus back to the pill. */
function useMenu() {
  const [open, setOpen] = useState(false);
  const wrap = useRef<HTMLSpanElement>(null);
  const trigger = useRef<HTMLButtonElement>(null);

  useEffect(() => {
    if (!open) return;

    const onPointer = (event: PointerEvent) => {
      if (!wrap.current?.contains(event.target as Node)) setOpen(false);
    };
    const onKey = (event: globalThis.KeyboardEvent) => {
      if (event.key !== "Escape") return;
      event.preventDefault();
      setOpen(false);
      trigger.current?.focus();
    };

    document.addEventListener("pointerdown", onPointer);
    document.addEventListener("keydown", onKey);
    return () => {
      document.removeEventListener("pointerdown", onPointer);
      document.removeEventListener("keydown", onKey);
    };
  }, [open]);

  return {
    open,
    wrap,
    trigger,
    toggle: () => setOpen((current) => !current),
    close: () => {
      setOpen(false);
      trigger.current?.focus();
    },
  };
}

/** Arrow keys walk a menu's options; up from the first returns to its filter. */
function moveFocus(event: KeyboardEvent<HTMLElement>) {
  if (event.key !== "ArrowDown" && event.key !== "ArrowUp") return;
  event.preventDefault();

  const options = Array.from(event.currentTarget.querySelectorAll<HTMLElement>(".cat"));
  const at = options.indexOf(document.activeElement as HTMLElement);
  const next = options[at + (event.key === "ArrowDown" ? 1 : -1)];

  if (next) next.focus();
  else if (event.key === "ArrowUp") {
    event.currentTarget.parentElement?.querySelector<HTMLElement>(".pop-filter")?.focus();
  }
}
