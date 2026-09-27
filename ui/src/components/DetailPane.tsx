import type { AddonDetail, AddonSummary } from "../types";
import { SOURCE_NAMES, isInstallable } from "../types";
import { count, initials } from "../format";
import type { RowState } from "./AddonRow";
import { DescriptionView } from "./Description";

interface Props {
  /** The row picked in the list; its fields fill the header while the detail loads. */
  addon: AddonSummary | null;
  detail: AddonDetail | null;
  error: string | null;
  state: RowState;
  busy: string | null;
  canInstall: boolean;
  onClose: () => void;
  onOpen: (url: string) => void;
  onInstall: () => void;
  onRemove: () => void;
}

export function DetailPane({
  addon,
  detail,
  error,
  state,
  busy,
  canInstall,
  onClose,
  onOpen,
  onInstall,
  onRemove,
}: Props) {
  if (!addon) {
    return (
      <section className="detail" aria-label="Addon details">
        <div className="state">
          <h2>Pick an addon</h2>
          <p>Its description, screenshots, and install options show up here.</p>
        </div>
      </section>
    );
  }

  const shown = detail ?? addon;

  return (
    <section className="detail" aria-label="Addon details">
      <div className="detail-head">
        <span className="row-icon detail-icon" aria-hidden="true">
          {shown.icon_url ? <img src={shown.icon_url} alt="" /> : initials(shown.name)}
        </span>
        <div className="detail-heading">
          <h2 className="detail-title">{shown.name}</h2>
          {shown.author && <span className="row-author">by {shown.author}</span>}
        </div>
        <Actions
          addon={shown}
          state={state}
          busy={busy}
          canInstall={canInstall}
          onInstall={onInstall}
          onRemove={onRemove}
        />
        <button type="button" className="btn" data-variant="quiet" onClick={onClose} aria-label="Close details">
          ✕
        </button>
      </div>

      <div className="detail-body">
        <div className="detail-main">
          {error && (
            <p className="prose">
              <span className="tag" data-tone="rust">Could not load</span> {error}
            </p>
          )}
          {!detail && !error && <p className="prose">{addon.summary || "Loading…"}</p>}
          {detail && (
            <DescriptionView
              description={detail.description}
              pageUrl={detail.page_url}
              title={detail.name}
              onOpen={onOpen}
            />
          )}

          {detail && detail.screenshots.length > 0 && (
            <div className="shots">
              <span className="label">Screenshots</span>
              <div className="shots-grid">
                {detail.screenshots.slice(0, 6).map((shot) => (
                  <img key={shot.url} src={shot.url} alt="" loading="lazy" />
                ))}
              </div>
            </div>
          )}
        </div>

        <aside className="detail-side">
          <Facts addon={shown} />
          <div className="detail-links">
            <button type="button" className="btn" onClick={() => onOpen(shown.page_url)}>
              Open source page
            </button>
            {detail?.website_url && (
              <button type="button" className="btn" onClick={() => onOpen(detail.website_url!)}>
                Author's site
              </button>
            )}
          </div>
        </aside>
      </div>
    </section>
  );
}

function Actions({
  addon,
  state,
  busy,
  canInstall,
  onInstall,
  onRemove,
}: {
  addon: AddonSummary;
  state: RowState;
  busy: string | null;
  canInstall: boolean;
  onInstall: () => void;
  onRemove: () => void;
}) {
  const installed = state === "installed" || state === "outdated";
  const installable = isInstallable(addon);

  return (
    <div className="row-actions">
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
        data-variant="primary"
        disabled={busy !== null || !canInstall || !installable}
        onClick={onInstall}
        title={
          installable
            ? canInstall
              ? undefined
              : "Choose your World of Warcraft folder first"
            : "Download it from the source's page instead"
        }
      >
        {busy ?? (state === "outdated" ? "Update" : installed ? "Reinstall" : "Install")}
      </button>
    </div>
  );
}

function Facts({ addon }: { addon: AddonSummary }) {
  return (
    <dl className="facts">
      <dt>Source</dt>
      <dd>{SOURCE_NAMES[addon.id.source]}</dd>

      <dt>Version</dt>
      <dd>{addon.version ?? "not published"}</dd>

      <dt>Downloads</dt>
      <dd>{count(addon.downloads)}</dd>

      {addon.download.kind === "unsupported" && (
        <>
          <dt>Archive</dt>
          <dd>{addon.download.format} — this application cannot open it</dd>
        </>
      )}

      {addon.download.kind === "external" && (
        <>
          <dt>Download</dt>
          <dd>only through the source's website, by the author's choice</dd>
        </>
      )}

      <dt>Expansions</dt>
      <dd>
        {addon.expansions
          .map((expansion) =>
            expansion.kind === "unknown" ? expansion.label : label(expansion.kind),
          )
          .join(", ") || "WoW Forever"}
      </dd>

      {addon.categories.length > 0 && (
        <>
          <dt>Categories</dt>
          <dd>{addon.categories.join(", ")}</dd>
        </>
      )}
    </dl>
  );
}

function label(kind: string): string {
  switch (kind) {
    case "forever":
      return "WoW Forever";
    default:
      return kind;
  }
}
