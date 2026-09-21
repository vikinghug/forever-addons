import type { AddonSummary } from "../types";
import { SOURCE_NAMES, isInstallable } from "../types";
import { count, initials } from "../format";

/** What the left stripe on a row encodes. */
export type RowState = "none" | "installed" | "outdated" | "broken";

interface Props {
  addon: AddonSummary;
  state: RowState;
  selected: boolean;
  busy: string | null;
  canInstall: boolean;
  onSelect: () => void;
  onInstall: () => void;
  onRemove: () => void;
}

export function AddonRow({
  addon,
  state,
  selected,
  busy,
  canInstall,
  onSelect,
  onInstall,
  onRemove,
}: Props) {
  const installed = state === "installed" || state === "outdated";
  const installable = isInstallable(addon);

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
          <span className="row-name">{addon.name}</span>
          {addon.author && <span className="row-author">by {addon.author}</span>}
        </span>
        <span className="row-summary">{addon.summary || "No description published."}</span>
        <span className="row-meta">
          <span className="chip">{SOURCE_NAMES[addon.id.source]}</span>
          {addon.categories.slice(0, 2).map((category) => (
            <span className="chip" key={category}>
              {category}
            </span>
          ))}
          <span className="stat">{count(addon.downloads)} downloads</span>
          {addon.version && <span className="stat">v{addon.version.replace(/^v/i, "")}</span>}
          {addon.download.kind === "unsupported" && (
            <span className="chip">.{addon.download.format}</span>
          )}
        </span>
      </span>

      <span className="row-actions" onClick={(event) => event.stopPropagation()}>
        {state === "outdated" && <span className="tag" data-tone="frost">Update</span>}
        {state === "installed" && <span className="tag" data-tone="moss">Installed</span>}
        {!installable && <span className="tag" data-tone="mute">Can't install</span>}

        {installed && (
          <button
            type="button"
            className="btn"
            data-variant="danger"
            disabled={busy !== null}
            onClick={onRemove}
          >
            Remove
          </button>
        )}
        <button
          type="button"
          className="btn"
          data-variant={state === "outdated" || !installed ? "primary" : undefined}
          disabled={busy !== null || !canInstall || !installable}
          onClick={onInstall}
          title={
            addon.download.kind === "unsupported"
              ? `Published as a .${addon.download.format} archive, which this application cannot open`
              : addon.download.kind === "external"
                ? "The author distributes only through the source's website"
                : canInstall
                  ? undefined
                  : "Choose your client folder first"
          }
        >
          {busy ?? (state === "outdated" ? "Update" : installed ? "Reinstall" : "Install")}
        </button>
      </span>
    </div>
  );
}
