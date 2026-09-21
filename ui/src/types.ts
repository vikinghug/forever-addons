// Mirrors the serde shapes in src-tauri. Anything added on the Rust side has to
// be added here too; there is no generated binding.

export type SourceId = "curseforge" | "wago" | "github";

export type Expansion = { kind: "forever" } | { kind: "unknown"; label: string };

export interface AddonId {
  source: SourceId;
  key: string;
}

export type Download =
  | { kind: "direct"; url: string }
  | { kind: "brokered" }
  /** An archive format this build cannot open. */
  | { kind: "unsupported"; url: string; format: string }
  /** The author distributes only through the source's website. */
  | { kind: "external"; url: string };

export interface AddonSummary {
  id: AddonId;
  name: string;
  summary: string;
  author: string | null;
  version: string | null;
  /** When the source last changed this addon, RFC 3339 in UTC. */
  updated_at: string | null;
  icon_url: string | null;
  page_url: string;
  categories: string[];
  downloads: number | null;
  expansions: Expansion[];
  download: Download;
}

export interface AddonDetail extends AddonSummary {
  description: string;
  website_url: string | null;
  screenshots: { url: string }[];
}

export interface InstalledFolder {
  folder: string;
  title: string | null;
  version: string | null;
  author: string | null;
  notes: string | null;
  interface: number | null;
  loads_on_client: boolean;
}

export type UpdateStatus =
  | { status: "up-to-date" }
  | { status: "available"; latest: string }
  /** No versions to compare, but the source changed after the install. */
  | { status: "source-changed"; updated_at: string }
  | { status: "unknown" };

export interface ManagedAddon {
  id: AddonId;
  name: string;
  version: string | null;
  folders: InstalledFolder[];
  page_url: string;
  installed_at: string;
  present: boolean;
  update: UpdateStatus;
}

export interface InstalledView {
  managed: ManagedAddon[];
  unmanaged: InstalledFolder[];
}

export interface SourceStatus {
  id: SourceId;
  name: string;
  site_url: string;
  enabled: boolean;
  requires_api_key: boolean;
  api_key_configured: boolean;
  addon_count: number;
  fetched_at: string | null;
  /** The repositories the user tracks — only ever non-empty for GitHub. */
  tracked_repos: string[];
}

export interface AppStatus {
  client_path: string | null;
  install_valid: boolean;
  install_error: string | null;
  sources: SourceStatus[];
}

export interface CatalogQuery {
  text: string;
  category: string | null;
  sources: SourceId[] | null;
}

export interface FetchProgress {
  source: SourceId;
  page: number;
  total_pages: number | null;
  addons_so_far: number;
}

export type InstallProgress = { addon_id: AddonId } & (
  | { stage: "resolving" }
  | { stage: "downloading"; received: number; total: number | null }
  | { stage: "extracting" }
  | { stage: "installed"; folders: string[] }
);

export const addonKey = (id: AddonId) => `${id.source}:${id.key}`;

export const SOURCE_NAMES: Record<SourceId, string> = {
  curseforge: "CurseForge",
  wago: "Wago Addons",
  github: "GitHub",
};

/** False when the archive cannot be obtained by this build. */
export const isInstallable = (addon: AddonSummary) =>
  addon.download.kind !== "unsupported" && addon.download.kind !== "external";

/** True for both a confirmed version bump and a post-install source change. */
export const needsUpdate = (update: UpdateStatus) =>
  update.status === "available" || update.status === "source-changed";
