import type { AddonDetail } from "../types";
import { SOURCE_NAMES, isInstallable } from "../types";
import { count } from "../format";
import type { RowState } from "./AddonRow";

interface Props {
  detail: AddonDetail | null;
  loading: boolean;
  error: string | null;
  state: RowState;
  busy: string | null;
  canInstall: boolean;
  onClose: () => void;
  onOpen: (url: string) => void;
  onInstall: () => void;
  onRemove: () => void;
}

export function DetailDrawer({
  detail,
  loading,
  error,
  state,
  busy,
  canInstall,
  onClose,
  onOpen,
  onInstall,
  onRemove,
}: Props) {
  const installed = state === "installed" || state === "outdated";
  const installable = detail === null || isInstallable(detail);
  return (
    <aside className="drawer" aria-label="Addon details">
      <div className="drawer-head">
        <div style={{ minWidth: 0, flex: 1 }}>
          <h2 className="drawer-title">{detail?.name ?? (loading ? "Loading…" : "Details")}</h2>
          {detail?.author && <span className="row-author">by {detail.author}</span>}
        </div>
        <button type="button" className="btn" data-variant="quiet" onClick={onClose} aria-label="Close details">
          ✕
        </button>
      </div>

      <div className="drawer-body">
        {error && (
          <p className="prose">
            <span className="tag" data-tone="rust">Could not load</span> {error}
          </p>
        )}

        {detail && (
          <>
            <p className="prose">{detail.description || detail.summary}</p>

            <dl className="facts">
              <dt>Source</dt>
              <dd>{SOURCE_NAMES[detail.id.source]}</dd>

              <dt>Version</dt>
              <dd>{detail.version ?? "not published"}</dd>

              <dt>Downloads</dt>
              <dd>{count(detail.downloads)}</dd>

              {detail.download.kind === "unsupported" && (
                <>
                  <dt>Archive</dt>
                  <dd>{detail.download.format} — this application cannot open it</dd>
                </>
              )}

              {detail.download.kind === "external" && (
                <>
                  <dt>Download</dt>
                  <dd>only through the source's website, by the author's choice</dd>
                </>
              )}

              <dt>Expansions</dt>
              <dd>
                {detail.expansions
                  .map((expansion) =>
                    expansion.kind === "unknown" ? expansion.label : label(expansion.kind),
                  )
                  .join(", ") || "WoW Forever"}
              </dd>

              {detail.categories.length > 0 && (
                <>
                  <dt>Categories</dt>
                  <dd>{detail.categories.join(", ")}</dd>
                </>
              )}
            </dl>

            <div className="row-actions">
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
            </div>

            <div className="row-actions">
              <button type="button" className="btn" onClick={() => onOpen(detail.page_url)}>
                Open source page
              </button>
              {detail.website_url && (
                <button type="button" className="btn" onClick={() => onOpen(detail.website_url!)}>
                  Author's site
                </button>
              )}
            </div>

            {detail.screenshots.length > 0 && (
              <div className="shots">
                <span className="label">Screenshots</span>
                {detail.screenshots.slice(0, 4).map((shot) => (
                  <img key={shot.url} src={shot.url} alt="" loading="lazy" />
                ))}
              </div>
            )}
          </>
        )}
      </div>
    </aside>
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
