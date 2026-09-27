import { initials } from "../format";
import type { Group, RelationRow } from "../relations";
import { GROUPS, nameOf } from "../relations";
import type { AddonId, AddonSummary } from "../types";
import { isInstallable } from "../types";

interface Props {
  groups: Record<Group, RelationRow[]>;
  addonName: string;
  /** The name of the file an install would fetch. */
  targetName: string;
  canInstall: boolean;
  busyOf: (id: AddonId) => string | null;
  onInstall: (id: AddonId) => void;
  onRemove: (id: AddonId) => void;
  onSelect: (addon: AddonSummary) => void;
  onOpen: (url: string) => void;
}

/**
 * The target file's relations, grouped by what the user has to do: conflicts
 * first, then missing requirements, then optional companions, then the
 * libraries it already carries.
 */
export function RelationsTab(props: Props) {
  const { groups, addonName, targetName } = props;
  const total = GROUPS.reduce((sum, group) => sum + groups[group].length, 0);

  if (total === 0) {
    return <p className="prose">{targetName} doesn't list any other addons.</p>;
  }

  return (
    <div className="relations">
      <p className="relations-note">
        From <b>{targetName}</b>, the file an install would fetch.
      </p>
      {GROUPS.map((group) =>
        groups[group].length === 0 ? null : (
          <section key={group} className="rel-group" aria-label={TITLES[group]}>
            <div className="section-head rel-head">
              <span className="label">{TITLES[group]}</span>
            </div>
            <p className="rel-why">{why(group, addonName)}</p>
            {group === "bundled" ? (
              <div className="libs">
                {groups[group].map((row) => (
                  <span key={row.addon.id.key} className="lib">
                    {nameOf(row.addon)}
                    {row.relation.kind === "included" && <small>own folder</small>}
                  </span>
                ))}
              </div>
            ) : (
              groups[group].map((row) => (
                <Row key={row.addon.id.key} row={row} group={group} {...props} />
              ))
            )}
          </section>
        ),
      )}
    </div>
  );
}

const TITLES: Record<Group, string> = {
  conflicts: "Conflicts",
  needs: "Needs",
  "works-with": "Works with",
  bundled: "Bundled inside",
  unknown: "Other relations",
};

function why(group: Group, name: string): string {
  switch (group) {
    case "conflicts":
      return "Running both makes one of them misbehave. Keep one.";
    case "needs":
      return `${name} won't load without these. Installing it brings them along.`;
    case "works-with":
      return `Optional. ${name} uses them when they're present.`;
    case "bundled":
      return `Shipped inside ${name}'s own files; nothing to install.`;
    case "unknown":
      return "Relations CurseForge describes in a way this app doesn't recognise yet.";
  }
}

function Row({
  row,
  group,
  targetName,
  canInstall,
  busyOf,
  onInstall,
  onRemove,
  onSelect,
  onOpen,
}: Props & { row: RelationRow; group: Group }) {
  const { addon } = row;
  const listed = addon.availability === "listed" ? addon : null;
  const busy = busyOf(addon.id);

  return (
    <div className="rel" data-tone={group === "conflicts" && row.standing !== "absent" ? "rust" : undefined}>
      <span className="row-icon" aria-hidden="true">
        {listed?.icon_url ? <img src={listed.icon_url} alt="" /> : initials(nameOf(addon))}
      </span>
      <div className="rel-body">
        <div className="rel-title">
          <span className="rel-name">{nameOf(addon)}</span>
          {listed?.author && <span className="row-author">by {listed.author}</span>}
          <StandingTag row={row} group={group} />
          {row.isNew && <span className="tag" data-tone="gold">New in {targetName}</span>}
        </div>
        {listed?.summary && <span className="rel-summary">{listed.summary}</span>}
      </div>
      <div className="rel-actions">
        {group === "conflicts" && row.standing === "installed" && (
          <button type="button" className="btn" data-variant="danger" disabled={busy !== null} onClick={() => onRemove(addon.id)}>
            {busy ?? `Remove ${nameOf(addon)}`}
          </button>
        )}
        {group === "conflicts" && row.standing === "by-hand" && (
          <span className="stat">Delete its folder to remove it</span>
        )}
        {group !== "conflicts" && row.standing === "absent" && listed && isInstallable(listed) && (
          <button type="button" className="btn" disabled={busy !== null || !canInstall} onClick={() => onInstall(addon.id)}>
            {busy ?? "Install"}
          </button>
        )}
        {listed && (
          <button type="button" className="btn" data-variant="quiet" onClick={() => onSelect(listed)}>
            Details
          </button>
        )}
        {addon.availability === "no-forever-file" && (
          <button type="button" className="btn" data-variant="quiet" onClick={() => onOpen(addon.page_url)}>
            Open page
          </button>
        )}
      </div>
    </div>
  );
}

function StandingTag({ row, group }: { row: RelationRow; group: Group }) {
  const conflict = group === "conflicts";

  if (row.standing === "installed") {
    return <span className="tag" data-tone={conflict ? "rust" : "moss"}>Installed</span>;
  }
  if (row.standing === "by-hand") {
    return <span className="tag" data-tone={conflict ? "rust" : "moss"}>Installed by hand</span>;
  }
  switch (row.addon.availability) {
    case "listed":
      return <span className="tag" data-tone="mute">Not installed</span>;
    case "no-forever-file":
      return <span className="tag" data-tone="mute">No Forever file</span>;
    case "unlisted":
      return <span className="tag" data-tone="mute">Not on CurseForge</span>;
  }
}
