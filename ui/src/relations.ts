// How the file an install would fetch relates to other addons, judged against
// what is installed right now.

import type {
  AddonId,
  AddonSummary,
  FileHistory,
  InstalledView,
  PublishedFile,
  RelatedAddon,
  Relation,
} from "./types";
import { addonKey, isInstallable } from "./types";

/** Ordered by what the user has to do about them. */
export type Group = "conflicts" | "needs" | "works-with" | "bundled" | "unknown";

export const GROUPS: Group[] = ["conflicts", "needs", "works-with", "bundled", "unknown"];

/** Installed by this app, a folder of the same name the user added, or neither. */
export type Standing = "installed" | "by-hand" | "absent";

export interface RelationRow {
  addon: RelatedAddon;
  relation: Relation;
  standing: Standing;
  /** The installed file did not relate to this addon the same way. */
  isNew: boolean;
}

export function groupOf(relation: Relation): Group {
  switch (relation.kind) {
    case "incompatible":
      return "conflicts";
    case "required":
      return "needs";
    case "optional":
    case "tool":
      return "works-with";
    case "embedded":
    case "included":
      return "bundled";
    case "unknown":
      return "unknown";
  }
}

export const fileById = (history: FileHistory, id: string | null) =>
  id === null ? undefined : history.files.find((file) => file.id === id);

/** The target file's relations, grouped. Empty when there is no target. */
export function relationsOf(
  history: FileHistory,
  installed: InstalledView,
): Record<Group, RelationRow[]> {
  const groups: Record<Group, RelationRow[]> = {
    conflicts: [],
    needs: [],
    "works-with": [],
    bundled: [],
    unknown: [],
  };

  const target = fileById(history, history.target);
  if (!target) return groups;

  const before = fileById(history, history.installed);
  for (const dependency of target.dependencies) {
    const addon = relatedAddon(history, dependency.addon);
    const group = groupOf(dependency.relation);
    groups[group].push({
      addon,
      relation: dependency.relation,
      standing: standingOf(addon, installed),
      isNew: before !== undefined && before.id !== target.id && !relatesAs(before, dependency.addon, group),
    });
  }

  return groups;
}

function relatesAs(file: PublishedFile, id: AddonId, group: Group): boolean {
  return file.dependencies.some(
    (dependency) =>
      addonKey(dependency.addon) === addonKey(id) && groupOf(dependency.relation) === group,
  );
}

function relatedAddon(history: FileHistory, id: AddonId): RelatedAddon {
  return (
    history.related.find((addon) => addonKey(addon.id) === addonKey(id)) ?? {
      availability: "unlisted",
      id,
    }
  );
}

export function nameOf(addon: RelatedAddon): string {
  switch (addon.availability) {
    case "listed":
    case "no-forever-file":
      return addon.name;
    case "unlisted":
      return `Unlisted addon ${addon.id.key}`;
  }
}

export function standingOf(addon: RelatedAddon, installed: InstalledView): Standing {
  const key = addonKey(addon.id);
  if (installed.managed.some((entry) => addonKey(entry.id) === key)) return "installed";
  if (addon.availability === "unlisted") return "absent";

  // A library installed by hand usually sits in a folder named after it.
  const name = folderish(addon.name);
  return installed.unmanaged.some((folder) => folderish(folder.folder) === name)
    ? "by-hand"
    : "absent";
}

const folderish = (name: string) => name.toLowerCase().replace(/[^a-z0-9]/g, "");

/** Required addons this app can install that are not there yet. */
export function missingRequired(groups: Record<Group, RelationRow[]>): AddonSummary[] {
  return groups.needs.flatMap((row) =>
    row.standing === "absent" && row.addon.availability === "listed" && isInstallable(row.addon)
      ? [row.addon]
      : [],
  );
}

/** Whether anything in the relations stands in the way of a clean install. */
export function needsAttention(groups: Record<Group, RelationRow[]>): boolean {
  return (
    groups.conflicts.some((row) => row.standing !== "absent") ||
    groups.needs.some((row) => row.standing === "absent")
  );
}
