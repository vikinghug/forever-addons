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
  /** The author allows downloads only on the source's website; the user
   * downloads the zip there and installs it with `install_addon_file`. */
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
  /** Sanitized HTML (`SafeHtml`); the summary as a paragraph when the source has none. */
  description: string;
  website_url: string | null;
  screenshots: Screenshot[];
}

export interface Screenshot {
  url: string;
  /** A smaller rendition for strips; `url` serves when there is none. */
  thumbnail_url: string | null;
  /** Plain text, never markup. */
  title: string | null;
  description: string | null;
}

/** How stable the author says a file is. */
export type Channel =
  | { kind: "release" }
  | { kind: "beta" }
  | { kind: "alpha" }
  | { kind: "unknown"; code: number };

/** What a file says about another addon. */
export type Relation =
  | { kind: "required" }
  | { kind: "optional" }
  | { kind: "tool" }
  | { kind: "incompatible" }
  /** Shipped inside the file's own folders. */
  | { kind: "embedded" }
  /** Packaged into the file's archive as folders of its own. */
  | { kind: "included" }
  | { kind: "unknown"; code: number };

export interface Dependency {
  addon: AddonId;
  relation: Relation;
}

export interface PublishedFile {
  /** Source-local file id (`FileKey`). */
  id: string;
  /** The author's name for the build; usually its version. */
  name: string;
  file_name: string;
  channel: Channel;
  /** RFC 3339, UTC. */
  published_at: string;
  size: number | null;
  downloads: number | null;
  /** Folders the archive places under Interface/AddOns. */
  folders: string[];
  dependencies: Dependency[];
}

export type RelatedAddon =
  | ({ availability: "listed" } & AddonSummary)
  /** Listed with no Forever file, typically an embed-only library. */
  | { availability: "no-forever-file"; id: AddonId; name: string; page_url: string }
  /** Missing from the source: deleted, private, or moderated. */
  | { availability: "unlisted"; id: AddonId };

export interface FileHistory {
  /** Newest first. */
  files: PublishedFile[];
  /** The file an install would fetch now. */
  target: string | null;
  /** The file installed, inferred from the install time. */
  installed: string | null;
  related: RelatedAddon[];
}

export type FileListing =
  | ({ kind: "listed" } & FileHistory)
  /** The source publishes one current download and nothing older. */
  | { kind: "not-offered" };

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
  /** The source will not serve the archive to this app; updates come from a
   * zip the user downloads from the addon's page. */
  site_download_only: boolean;
}

export interface InstalledView {
  managed: ManagedAddon[];
  unmanaged: InstalledFolder[];
}

/** Whether a live source's key may search, as learned this session. */
export type SearchAccess = "untested" | "allowed" | "forbidden";

export interface SourceStatus {
  id: SourceId;
  name: string;
  site_url: string;
  enabled: boolean;
  /** Catalog sources are pulled and cached; live ones are queried per search. */
  listing: "catalog" | "live";
  /** Only set for live sources. */
  search_access: SearchAccess | null;
  requires_api_key: boolean;
  api_key_configured: boolean;
  /** Always 0 for a live source, which keeps no catalog. */
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

/** Mirrors `domain::SortField`: only fields every source can fill. */
export type SortField = "downloads" | "updated" | "name" | "author";
export type SortDirection = "ascending" | "descending";

export interface Sort {
  field: SortField;
  direction: SortDirection;
}

/** One direction of a sort field, named in the field's own terms. */
export interface SortChoice {
  direction: SortDirection;
  /** On the menu's segmented button: "Most", "A–Z". */
  short: string;
  /** On the menu's trigger: "Most downloaded", "Name, A–Z". */
  long: string;
}

/** Every browse order. Each field lists its natural direction first:
 * biggest and newest first, names A to Z. */
export const SORT_OPTIONS: { field: SortField; label: string; choices: [SortChoice, SortChoice] }[] = [
  {
    field: "downloads",
    label: "Downloads",
    choices: [
      { direction: "descending", short: "Most", long: "Most downloaded" },
      { direction: "ascending", short: "Fewest", long: "Fewest downloads" },
    ],
  },
  {
    field: "updated",
    label: "Last updated",
    choices: [
      { direction: "descending", short: "Newest", long: "Recently updated" },
      { direction: "ascending", short: "Oldest", long: "Least recently updated" },
    ],
  },
  {
    field: "name",
    label: "Name",
    choices: [
      { direction: "ascending", short: "A–Z", long: "Name, A–Z" },
      { direction: "descending", short: "Z–A", long: "Name, Z–A" },
    ],
  },
  {
    field: "author",
    label: "Author",
    choices: [
      { direction: "ascending", short: "A–Z", long: "Author, A–Z" },
      { direction: "descending", short: "Z–A", long: "Author, Z–A" },
    ],
  },
];

/** "Most downloaded" for the default sort. */
export function sortName(sort: Sort): string {
  const option = SORT_OPTIONS.find((entry) => entry.field === sort.field);
  const choice = option?.choices.find((entry) => entry.direction === sort.direction);
  return choice?.long ?? sort.field;
}

export const DEFAULT_SORT: Sort = { field: "downloads", direction: "descending" };

export interface CatalogQuery {
  text: string;
  category: string | null;
  sources: SourceId[] | null;
  sort: Sort;
}

export interface SourceNotice {
  source: SourceId;
  message: string;
}

/** What the browse panel narrows by. `sources: null` means every source;
 * hiding installed addons happens in the UI, not in the query. */
export interface BrowseQuery {
  text: string;
  category: string | null;
  sources: SourceId[] | null;
  sort: Sort;
  hideInstalled: boolean;
}

export interface SourceCount {
  source: SourceId;
  count: number;
}

export interface CategoryCount {
  name: string;
  count: number;
}

export interface SearchResults {
  addons: AddonSummary[];
  /** Matches for the text before the source and category filters. */
  text_matches: number;
  /** Matches per searched source, before the source filter. A live source
   * the query left out was never asked and has no entry. */
  sources: SourceCount[];
  /** Categories among the matches before the category filter. */
  categories: CategoryCount[];
  /** Why a live source's results are partial or missing. */
  notices: SourceNotice[];
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
