import type { InstalledView, ManagedAddon } from "../types";
import { SOURCE_NAMES, addonKey, needsUpdate } from "../types";
import { day, initials, when } from "../format";

interface Props {
  installed: InstalledView;
  busy: Record<string, string>;
  onUpdate: (addon: ManagedAddon) => void;
  onRemove: (addon: ManagedAddon) => void;
  onOpen: (url: string) => void;
  onGoToBrowse: () => void;
}

export function Installed({
  installed,
  busy,
  onUpdate,
  onRemove,
  onOpen,
  onGoToBrowse,
}: Props) {
  const { managed, unmanaged } = installed;

  if (managed.length === 0 && unmanaged.length === 0) {
    return (
      <div className="state">
        <h2>Interface/AddOns is empty</h2>
        <p>Install something from the browse list and it will show up here.</p>
        <button type="button" className="btn" data-variant="primary" onClick={onGoToBrowse}>
          Browse addons
        </button>
      </div>
    );
  }

  return (
    <>
      {managed.length > 0 && (
        <>
          <div className="section-head">
            <span className="label">Managed here · {managed.length}</span>
          </div>
          <div className="rows">
            {managed.map((addon) => (
              <ManagedRow
                key={addonKey(addon.id)}
                addon={addon}
                busy={busy[addonKey(addon.id)] ?? null}
                onUpdate={() => onUpdate(addon)}
                onRemove={() => onRemove(addon)}
                onOpen={() => onOpen(addon.page_url)}
              />
            ))}
          </div>
        </>
      )}

      {unmanaged.length > 0 && (
        <>
          <div className="section-head">
            <span className="label">Installed by hand · {unmanaged.length}</span>
          </div>
          <div className="rows">
            {unmanaged.map((folder) => (
              <div className="row" data-state="none" key={folder.folder}>
                <span className="row-icon" aria-hidden="true">·</span>
                <span className="row-body">
                  <span className="row-head">
                    <span className="row-name">{folder.title ?? folder.folder}</span>
                    {folder.author && <span className="row-author">by {folder.author}</span>}
                  </span>
                  <span className="row-meta">
                    <span className="stat">{folder.folder}</span>
                    {folder.version && <span className="stat">v{folder.version}</span>}
                    {folder.interface && <span className="stat">iface {folder.interface}</span>}
                    {!folder.loads_on_client && (
                      <span className="tag" data-tone="rust">
                        Will not load on this client
                      </span>
                    )}
                  </span>
                </span>
                <span className="row-actions">
                  <span className="tag" data-tone="mute">Not managed</span>
                </span>
              </div>
            ))}
          </div>
        </>
      )}
    </>
  );
}

function ManagedRow({
  addon,
  busy,
  onUpdate,
  onRemove,
  onOpen,
}: {
  addon: ManagedAddon;
  busy: string | null;
  onUpdate: () => void;
  onRemove: () => void;
  onOpen: () => void;
}) {
  const outdated = needsUpdate(addon.update);
  const state = !addon.present ? "broken" : outdated ? "outdated" : "installed";

  return (
    <div className="row" data-state={state}>
      <span className="row-icon" aria-hidden="true">
        {initials(addon.name)}
      </span>

      <span className="row-body">
        <span className="row-head">
          <span className="row-name">{addon.name}</span>
          <span className="row-author">installed {when(addon.installed_at)}</span>
        </span>
        <span className="row-meta">
          <span className="chip">{SOURCE_NAMES[addon.id.source]}</span>
          {addon.version && <span className="stat">v{addon.version.replace(/^v/i, "")}</span>}
          {addon.folders.map((folder) => (
            <span className="chip" key={folder.folder}>
              {folder.folder}
            </span>
          ))}
        </span>
      </span>

      <span className="row-actions">
        {!addon.present && <span className="tag" data-tone="rust">Folders missing</span>}
        {addon.update.status === "available" && (
          <span className="tag" data-tone="frost">
            v{addon.update.latest} available
          </span>
        )}
        {addon.update.status === "source-changed" && (
          <span className="tag" data-tone="frost">
            Changed {day(addon.update.updated_at)}
          </span>
        )}
        {addon.present && !outdated && <span className="tag" data-tone="moss">Current</span>}

        <button type="button" className="btn" data-variant="quiet" onClick={onOpen}>
          Page
        </button>
        <button
          type="button"
          className="btn"
          data-variant="danger"
          disabled={busy !== null}
          onClick={onRemove}
        >
          Remove
        </button>
        <button
          type="button"
          className="btn"
          data-variant={outdated || !addon.present ? "primary" : undefined}
          disabled={busy !== null}
          onClick={onUpdate}
        >
          {busy ?? (outdated ? "Update" : addon.present ? "Reinstall" : "Restore")}
        </button>
      </span>
    </div>
  );
}
