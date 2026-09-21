import type { AddonSummary, InstalledView } from "../types";
import { addonKey, needsUpdate } from "../types";
import { AddonRow, type RowState } from "./AddonRow";

interface Props {
  addons: AddonSummary[];
  installed: InstalledView;
  selected: string | null;
  busy: Record<string, string>;
  canInstall: boolean;
  loading: boolean;
  hasCatalog: boolean;
  onSelect: (addon: AddonSummary) => void;
  onInstall: (addon: AddonSummary) => void;
  onRemove: (addon: AddonSummary) => void;
  onGoToSources: () => void;
}

export function Browse({
  addons,
  installed,
  selected,
  busy,
  canInstall,
  loading,
  hasCatalog,
  onSelect,
  onInstall,
  onRemove,
  onGoToSources,
}: Props) {
  if (!hasCatalog) {
    return (
      <div className="state">
        <h2>No addons pulled yet</h2>
        <p>
          Pull a source's catalog once and it is cached on disk until you refresh it.
          CurseForge and Wago need a key first; GitHub just needs a repository to track.
        </p>
        <button type="button" className="btn" data-variant="primary" onClick={onGoToSources}>
          Go to sources
        </button>
      </div>
    );
  }

  if (addons.length === 0) {
    return (
      <div className="state">
        <h2>{loading ? "Searching…" : "Nothing matches that"}</h2>
        <p>
          {loading
            ? "Reading the cached catalogs."
            : "Try a shorter search, or clear the category filter."}
        </p>
      </div>
    );
  }

  return (
    <div className="rows" role="listbox" aria-label="Addons">
      {addons.map((addon) => (
        <AddonRow
          key={addonKey(addon.id)}
          addon={addon}
          state={stateOf(addon, installed)}
          selected={selected === addonKey(addon.id)}
          busy={busy[addonKey(addon.id)] ?? null}
          canInstall={canInstall}
          onSelect={() => onSelect(addon)}
          onInstall={() => onInstall(addon)}
          onRemove={() => onRemove(addon)}
        />
      ))}
    </div>
  );
}

/** What the row stripe and the action button say about an addon. */
export function stateOf(addon: AddonSummary, installed: InstalledView): RowState {
  const record = installed.managed.find((entry) => addonKey(entry.id) === addonKey(addon.id));
  if (!record) return "none";
  if (!record.present) return "broken";
  return needsUpdate(record.update) ? "outdated" : "installed";
}
