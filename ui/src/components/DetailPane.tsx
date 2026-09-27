import { useEffect, useState, type ReactNode } from "react";

import { api, messageOf } from "../api";
import { count, initials } from "../format";
import type { Group, RelationRow } from "../relations";
import { GROUPS, fileById, missingRequired, nameOf, needsAttention, relationsOf } from "../relations";
import type { AddonDetail, AddonId, AddonSummary, FileListing, InstalledView } from "../types";
import { SOURCE_NAMES, addonKey, isInstallable } from "../types";
import type { RowState } from "./AddonRow";
import { DescriptionView } from "./Description";
import { FilesTab } from "./FilesTab";
import { Gallery } from "./Gallery";
import { RelationsTab } from "./RelationsTab";

interface Props {
  /** The row picked in the list; its fields fill the header while the detail loads. */
  addon: AddonSummary | null;
  detail: AddonDetail | null;
  error: string | null;
  state: RowState;
  installed: InstalledView;
  /** What each addon's install or removal is doing, by `addonKey`. */
  busyByKey: Record<string, string>;
  canInstall: boolean;
  onClose: () => void;
  onOpen: (url: string) => void;
  /** Installs the addon, then each of `with` — the requirements it lacks. */
  onInstall: (withAddons: AddonId[]) => void;
  onInstallOther: (id: AddonId) => void;
  onRemoveOther: (id: AddonId) => void;
  onSelect: (addon: AddonSummary) => void;
  /** Installs from a zip the user downloaded, for a site-only addon. */
  onInstallFile: () => void;
  onRemove: () => void;
}

export function DetailPane(props: Props) {
  const { addon } = props;
  if (!addon) {
    return (
      <section className="detail" aria-label="Addon details">
        <div className="state">
          <h2>Pick an addon</h2>
          <p>Its description, screenshots, files, and related addons show up here.</p>
        </div>
      </section>
    );
  }

  // Keyed so tabs and loaded files reset when another addon is picked.
  return <AddonPane key={addonKey(addon.id)} {...props} addon={addon} />;
}

type Tab = "overview" | "files" | "relations";

type Files =
  | { state: "loading" }
  | { state: "loaded"; listing: FileListing }
  | { state: "failed"; message: string };

function AddonPane({
  addon,
  detail,
  error,
  state,
  installed,
  busyByKey,
  canInstall,
  onClose,
  onOpen,
  onInstall,
  onInstallFile,
  onInstallOther,
  onRemoveOther,
  onSelect,
  onRemove,
}: Props & { addon: AddonSummary }) {
  const [tab, setTab] = useState<Tab>("overview");
  const files = useFiles(addon.id, installed);
  const shown = detail ?? addon;
  const busy = busyByKey[addonKey(addon.id)] ?? null;
  const busyOf = (id: AddonId) => busyByKey[addonKey(id)] ?? null;

  const history = files.state === "loaded" && files.listing.kind === "listed" ? files.listing : null;
  const groups = history ? relationsOf(history, installed) : null;
  const missing = groups ? missingRequired(groups) : [];
  const target = history ? fileById(history, history.target) : undefined;
  const relationCount = groups
    ? GROUPS.filter((group) => group !== "bundled").reduce((sum, group) => sum + groups[group].length, 0)
    : 0;

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
          missing={missing}
          onInstall={() => onInstall(missing.map((entry) => entry.id))}
          onInstallFile={onInstallFile}
          onRemove={onRemove}
        />
        <button type="button" className="btn" data-variant="quiet" onClick={onClose} aria-label="Close details">
          ✕
        </button>
      </div>

      {history && (
        <div className="detail-tabs" role="tablist" aria-label="Addon sections">
          <TabButton tab="overview" current={tab} onPick={setTab}>Overview</TabButton>
          <TabButton tab="files" current={tab} onPick={setTab}>
            Files <span className="tab-count">{history.files.length}</span>
          </TabButton>
          <TabButton tab="relations" current={tab} onPick={setTab}>
            Relations
            {groups && needsAttention(groups) ? (
              <span className="tab-alert" title="Needs attention" />
            ) : (
              <span className="tab-count">{relationCount}</span>
            )}
          </TabButton>
        </div>
      )}

      {tab === "files" && history ? (
        <div className="detail-body" data-single role="tabpanel">
          <FilesTab addonId={addon.id} pageUrl={shown.page_url} history={history} onOpen={onOpen} />
        </div>
      ) : tab === "relations" && groups ? (
        <div className="detail-body" data-single role="tabpanel">
          <RelationsTab
            groups={groups}
            addonName={shown.name}
            targetName={target?.name ?? shown.name}
            canInstall={canInstall}
            busyOf={busyOf}
            onInstall={onInstallOther}
            onRemove={onRemoveOther}
            onSelect={onSelect}
            onOpen={onOpen}
          />
        </div>
      ) : (
        <div className="detail-body" role={history ? "tabpanel" : undefined}>
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
            {detail && <Gallery shots={detail.screenshots} />}
            {!detail && !error && <p className="prose">{addon.summary || "Loading…"}</p>}
            {detail && <DescriptionView html={detail.description} onOpen={onOpen} />}
          </div>

          <aside className="detail-side">
            {groups && (
              <Preflight
                groups={groups}
                updating={state === "installed" || state === "outdated"}
                onReview={() => setTab("relations")}
              />
            )}
            {files.state === "failed" && (
              <p className="side-note">Files and related addons could not load: {files.message}</p>
            )}
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
              {addon.id.source === "curseforge" && (
                <button
                  type="button"
                  className="btn"
                  onClick={() => onOpen(`${shown.page_url.replace(/\/+$/, "")}/comments`)}
                >
                  Comments ↗
                </button>
              )}
            </div>
          </aside>
        </div>
      )}
    </section>
  );
}

function TabButton({
  tab,
  current,
  onPick,
  children,
}: {
  tab: Tab;
  current: Tab;
  onPick: (tab: Tab) => void;
  children: ReactNode;
}) {
  return (
    <button
      type="button"
      role="tab"
      className="tab"
      aria-selected={tab === current}
      onClick={() => onPick(tab)}
    >
      {children}
    </button>
  );
}

/**
 * The file listing for this addon, fetched again when its install changes so
 * the installed file and the relations' standings stay current.
 */
function useFiles(id: AddonId, installed: InstalledView): Files {
  const [files, setFiles] = useState<Files>({ state: "loading" });
  const record = installed.managed.find((entry) => addonKey(entry.id) === addonKey(id));
  const installedAt = record?.installed_at ?? null;

  const { source, key } = id;

  useEffect(() => {
    let current = true;
    api
      .getAddonFiles({ source, key })
      .then((listing) => current && setFiles({ state: "loaded", listing }))
      .catch((cause) => current && setFiles({ state: "failed", message: messageOf(cause) }));
    return () => {
      current = false;
    };
  }, [source, key, installedAt]);

  return files;
}

/** What to sort out before installing: the one boxed element in the side column. */
function Preflight({
  groups,
  updating,
  onReview,
}: {
  groups: Record<Group, RelationRow[]>;
  updating: boolean;
  onReview: () => void;
}) {
  const conflicts = groups.conflicts.filter((row) => row.standing !== "absent");
  const missing = groups.needs.filter((row) => row.standing === "absent");
  const companions = groups["works-with"].filter((row) => row.standing !== "absent");
  if (conflicts.length + missing.length + companions.length === 0) return null;

  const names = (rows: RelationRow[]) => rows.map((row) => nameOf(row.addon)).join(", ");

  return (
    <div className="preflight">
      <span className="label">Before you {updating ? "update" : "install"}</span>
      {conflicts.length > 0 && (
        <p className="preflight-item" data-tone="rust">
          <span>
            <b>Conflicts with {names(conflicts)}</b>, which you have installed.{" "}
            <button type="button" className="link" onClick={onReview}>Review</button>
          </span>
        </p>
      )}
      {missing.length > 0 && (
        <p className="preflight-item" data-tone="frost">
          <span>
            <b>Needs {names(missing)}</b>. {updating ? "Updating" : "Installing"} brings{" "}
            {missing.length === 1 ? "it" : "them"} along when this app can.
          </span>
        </p>
      )}
      {companions.length > 0 && (
        <p className="preflight-item" data-tone="moss">
          <span>
            Works with {names(companions)}, which you have.
          </span>
        </p>
      )}
    </div>
  );
}

function Actions({
  addon,
  state,
  busy,
  canInstall,
  missing,
  onInstall,
  onInstallFile,
  onRemove,
}: {
  addon: AddonSummary;
  state: RowState;
  busy: string | null;
  canInstall: boolean;
  /** Requirements the install brings along. */
  missing: AddonSummary[];
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
        {!busy && !siteOnly && missing.length > 0 && (
          <span className="btn-sub">
            + {missing.length === 1 ? missing[0].name : `${missing.length} more`}
          </span>
        )}
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
