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
  /** Installs from a zip the user downloaded, for a site-only addon. */
  onInstallFile: () => void;
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
  onInstallFile,
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
          onInstallFile={onInstallFile}
          onRemove={onRemove}
        />
        <button type="button" className="btn" data-variant="quiet" onClick={onClose} aria-label="Close details">
          ✕
        </button>
      </div>

      <div className="detail-body">
        <div className="detail-main">
          <DownloadNotice
            addon={shown}
            busy={busy}
            canInstall={canInstall}
            onOpen={onOpen}
            onInstallFile={onInstallFile}
          />
          {error && (
            <p className="prose">
              <span className="tag" data-tone="rust">Could not load</span> {error}
            </p>
          )}
          {!detail && !error && <p className="prose">{addon.summary || "Loading…"}</p>}
          {detail && (
            <DescriptionView html={detail.description} onOpen={onOpen} />
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
  onInstallFile,
  onRemove,
}: {
  addon: AddonSummary;
  state: RowState;
  busy: string | null;
  canInstall: boolean;
  onInstall: () => void;
  onInstallFile: () => void;
  onRemove: () => void;
}) {
  const installed = state === "installed" || state === "outdated";
  const installable = isInstallable(addon);
  const siteOnly = addon.download.kind === "external";

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
        disabled={busy !== null || !canInstall || !(installable || siteOnly)}
        onClick={siteOnly ? onInstallFile : onInstall}
        title={
          !canInstall
            ? "Choose your World of Warcraft folder first"
            : siteOnly
              ? "Pick the zip you downloaded from the addon's page"
              : installable
                ? undefined
                : "This app cannot open the archive; see the note below"
        }
      >
        {busy ?? `${state === "outdated" ? "Update" : installed ? "Reinstall" : "Install"}${siteOnly ? " from zip…" : ""}`}
      </button>
    </div>
  );
}

/** Why this app cannot fetch the archive itself, and what to do instead. */
function DownloadNotice({
  addon,
  busy,
  canInstall,
  onOpen,
  onInstallFile,
}: {
  addon: AddonSummary;
  busy: string | null;
  canInstall: boolean;
  onOpen: (url: string) => void;
  onInstallFile: () => void;
}) {
  const source = SOURCE_NAMES[addon.id.source];

  switch (addon.download.kind) {
    case "direct":
    case "brokered":
      return null;

    case "external": {
      const { url } = addon.download;
      return (
        <div className="notice" role="note">
          <h3 className="notice-title">Download this one from {source}</h3>
          <p>
            The author of {addon.name} has turned off downloads through third-party apps, so{" "}
            {source} won't send the file to Forever Addons. Download the zip from the addon's
            page, then pick it here. It installs and shows under Installed like any other addon,
            and updates are still flagged when the author publishes a new file.
          </p>
          <div className="notice-actions">
            <button type="button" className="btn" onClick={() => onOpen(url)}>
              1 · Open download page
            </button>
            <button
              type="button"
              className="btn"
              data-variant="primary"
              disabled={busy !== null || !canInstall}
              onClick={onInstallFile}
            >
              2 · Install downloaded zip…
            </button>
          </div>
        </div>
      );
    }

    case "unsupported": {
      const { url, format } = addon.download;
      return (
        <div className="notice" role="note">
          <h3 className="notice-title">Published as a .{format} archive</h3>
          <p>
            Forever Addons can only unpack .zip files. Download {addon.name} from its page and
            unpack it into Interface/AddOns yourself; it will then show under Installed as an
            addon installed by hand.
          </p>
          <div className="notice-actions">
            <button type="button" className="btn" onClick={() => onOpen(url)}>
              Open download page
            </button>
          </div>
        </div>
      );
    }
  }
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
          <dd>from the addon's page only — the author turned off app downloads</dd>
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
