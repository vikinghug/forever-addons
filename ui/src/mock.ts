// A stand-in backend for `npm --prefix ui run dev`.
//
// The fixtures are shaped like real CurseForge, Wago, and GitHub metadata, so
// the browse list looks like the one the desktop app renders. CurseForge is
// live in the real app; here its rows sit in the same fixture list and are
// answered as if CurseForge had just returned them. State lives in module
// scope: installing something here changes the mock's world until reload.

import type {
  AddonDetail,
  AddonId,
  AddonSummary,
  AppStatus,
  CatalogQuery,
  Description,
  InstalledView,
  ManagedAddon,
  SearchResults,
  Sort,
  SourceId,
} from "./types";
import { addonKey } from "./types";

const FOREVER = { kind: "forever" } as const;

function addon(
  source: SourceId,
  key: string,
  name: string,
  author: string,
  summary: string,
  categories: string[],
  downloads: number,
  version: string | null,
  updated_at: string | null = "2026-09-18T00:00:00Z",
  download?: AddonSummary["download"],
): AddonSummary {
  return {
    id: { source, key },
    name,
    author,
    summary,
    categories,
    downloads,
    version,
    updated_at,
    icon_url: null,
    page_url: pageUrl(source, key, name),
    expansions: [FOREVER],
    download:
      download ?? (source === "wago" ? { kind: "brokered" } : { kind: "direct", url: "" }),
  };
}

function pageUrl(source: SourceId, key: string, name: string): string {
  const slug = name.toLowerCase().replace(/\W+/g, "-");
  switch (source) {
    case "curseforge":
      return `https://www.curseforge.com/wow/addons/${slug}`;
    case "wago":
      return `https://addons.wago.io/addons/${slug}`;
    case "github":
      return `https://github.com/${key}`;
  }
}

const CATALOG: AddonSummary[] = [
  addon("curseforge", "1032100", "Questie Forever", "Aero",
    "Quest objectives on the map, rebuilt for WoW Forever.",
    ["Quests & Leveling"], 152_340, null, "2026-09-18T20:00:00Z"),
  addon("curseforge", "1032411", "ClassicUI Forever", "Ketho",
    "The classic interface look on Forever's modern client.",
    ["Miscellaneous"], 48_210, null, "2026-09-19T09:00:00Z"),
  addon("curseforge", "1032987", "RestedXP Guide", "RestedXP",
    "In-game leveling routes for the Forever beta.",
    ["Quests & Leveling"], 61_020, null, "2026-09-17T12:00:00Z"),
  addon("curseforge", "1031500", "Auctionator", "plusmouse",
    "A saner auction house, ported to Forever.",
    ["Auction & Economy"], 88_400, null, "2026-09-16T10:00:00Z"),
  // Some authors disable API distribution; the page is the only download.
  addon("curseforge", "1029001", "Details! Damage Meter Forever", "Terciob",
    "The damage meter, ported to Forever.",
    ["Combat"], 201_500, null, "2026-09-16T08:00:00Z",
    { kind: "external", url: "https://www.curseforge.com/wow/addons/details-forever" }),
  addon("wago", "qN5mGYzr", "ForeverAuras", "Stanzilla",
    "A WeakAuras replacement built for Forever's addon rules.",
    [], 48_211, "1.2.0", "2026-09-17T08:00:00Z"),
  addon("wago", "aB3xLQwe", "Baganator", "plusmouse",
    "One-bag inventory with category views.",
    [], 30_150, "690.2", "2026-09-18T16:00:00Z"),
  addon("github", "RevoltLive85/ForeverGuide", "ForeverGuide", "RevoltLive85",
    "A data-driven leveling guide engine for WoW Forever.",
    [], 1_040, "v1.4.0", "2026-09-18T12:00:00Z"),
];

const DESCRIPTIONS: Record<string, Description> = {
  "curseforge:1032100": {
    format: "html",
    text: `<h1>Questie Forever</h1>
<p>&nbsp;</p>
<p>Questie Forever tracks quest <strong>objectives</strong>, <em>availability</em>, and turn-ins
on the world map and minimap, using Forever's own quest database.</p>
<h2>Download</h2>
<p>We suggest the <a href="/linkout?remoteUrl=https%253a%252f%252fcurseforge.overwolf.com%252f">CurseForge Client</a>,
or the <a href="https://github.com/Questie/Questie/releases/latest">latest GitHub release</a>.</p>
<h2>Information</h2>
<ul>
<li><a href="/wow/addons/questie/pages/faq">Frequently Asked Questions</a></li>
<li>Type <code>/questie</code> to open the options.</li>
<li><span style="color:#808080;font-size:18px">Inline styles are stripped.</span></li>
</ul>
<script>alert("never runs")</script>`,
  },
  "wago:qN5mGYzr": {
    format: "plain",
    text:
      "ForeverAuras displays buffs, debuffs, cooldowns, and any other condition " +
      "you can express, as icons, bars, or text anywhere on screen — within " +
      "Forever's addon restrictions.",
  },
};

let status: AppStatus = {
  client_path: "/home/you/Games/Battle.net/World of Warcraft/_classic_beta_",
  install_valid: true,
  install_error: null,
  sources: [
    { id: "curseforge", name: "CurseForge",
      site_url: "https://www.curseforge.com/wow/search?class=addons&gameVersionTypeId=88568",
      enabled: true, listing: "live", search_access: "allowed",
      requires_api_key: true, api_key_configured: true,
      addon_count: 0, fetched_at: null, tracked_repos: [] },
    { id: "wago", name: "Wago Addons", site_url: "https://addons.wago.io/",
      enabled: true, listing: "catalog", search_access: null,
      requires_api_key: true, api_key_configured: false,
      addon_count: 0, fetched_at: null, tracked_repos: [] },
    { id: "github", name: "GitHub", site_url: "https://github.com/",
      enabled: true, listing: "catalog", search_access: null,
      requires_api_key: false, api_key_configured: false,
      addon_count: 1, fetched_at: "2026-09-20T18:00:00Z",
      tracked_repos: ["RevoltLive85/ForeverGuide"] },
  ],
};

let installed: InstalledView = {
  managed: [
    managed(CATALOG[0], ["Questie"], "10.0.1", { status: "available", latest: "10.1.0" }),
    managed(CATALOG[5], ["ForeverAuras", "ForeverAurasOptions"], "1.2.0", {
      status: "up-to-date",
    }),
  ],
  unmanaged: [
    // A retail addon left over in a shared layout; Forever refuses it.
    { folder: "WeakAuras", title: "WeakAuras", version: "5.19.13", author: "The WeakAuras Team",
      notes: null, interface: 110200, loads_on_client: false },
    { folder: "Ace3", title: "Ace3", version: null, author: null, notes: null,
      interface: 16001, loads_on_client: true },
  ],
};

function managed(
  source: AddonSummary,
  folders: string[],
  version: string,
  update: ManagedAddon["update"],
): ManagedAddon {
  return {
    id: source.id,
    name: source.name,
    version,
    page_url: source.page_url,
    installed_at: "2026-09-18T19:41:00Z",
    present: true,
    update,
    folders: folders.map((folder) => ({
      folder,
      title: folder,
      version,
      author: source.author,
      notes: null,
      interface: 16001,
      loads_on_client: true,
    })),
  };
}

const delay = <T,>(value: T, ms = 180) =>
  new Promise<T>((resolve) => setTimeout(() => resolve(value), ms));

export const getStatus = () => delay(status, 60);

export const detectWowInstall = () =>
  delay<string | null>("/home/you/Games/Battle.net/World of Warcraft/_classic_beta_");

export function setClientPath(path: string) {
  status = { ...status, client_path: path, install_valid: true, install_error: null };
  return delay(status);
}

export function setEnabledSources(sources: SourceId[]) {
  status = {
    ...status,
    sources: status.sources.map((source) => ({
      ...source,
      enabled: sources.includes(source.id),
    })),
  };
  return delay(status);
}

export function setCurseforgeApiKey(key: string | null) {
  return setKeyConfigured("curseforge", key);
}

export function setWagoToken(token: string | null) {
  return setKeyConfigured("wago", token);
}

function setKeyConfigured(id: SourceId, key: string | null) {
  const configured = key !== null && key.trim() !== "";
  status = {
    ...status,
    sources: status.sources.map((source) =>
      source.id === id ? { ...source, api_key_configured: configured } : source,
    ),
  };
  return delay(status);
}

export function addGithubRepo(repo: string) {
  const slug = repo.replace(/^https:\/\/github\.com\//, "").replace(/\.git$/, "");
  status = {
    ...status,
    sources: status.sources.map((source) =>
      source.id === "github"
        ? { ...source, tracked_repos: [...new Set([...source.tracked_repos, slug])] }
        : source,
    ),
  };
  return delay(status);
}

export function removeGithubRepo(repo: string) {
  status = {
    ...status,
    sources: status.sources.map((source) =>
      source.id === "github"
        ? { ...source, tracked_repos: source.tracked_repos.filter((entry) => entry !== repo) }
        : source,
    ),
  };
  return delay(status);
}

export function refreshSource(source: SourceId) {
  if (source === "curseforge") {
    return Promise.reject("CurseForge is searched live and has no catalog to pull");
  }

  const pulled = CATALOG.filter((addon) => addon.id.source === source).length;
  status = {
    ...status,
    sources: status.sources.map((entry) =>
      entry.id === source
        ? { ...entry, fetched_at: new Date().toISOString(), addon_count: pulled }
        : entry,
    ),
  };
  return delay(status, 900);
}

/** Mirrors `search_addons`: a live source answers only with a key saved and
 * only when the query includes it, and a bare number is a CurseForge project
 * ID. Facets are counted as `catalog::assemble` counts them. */
export function searchAddons(query: CatalogQuery): Promise<SearchResults> {
  const text = query.text.trim();
  const words = text.toLowerCase().split(/\s+/).filter(Boolean);
  const includes = (source: SourceId) => !query.sources || query.sources.includes(source);
  const searched = status.sources
    .filter((source) => source.enabled)
    .filter((source) =>
      source.listing === "live"
        ? source.api_key_configured && includes(source.id)
        : source.fetched_at !== null,
    )
    .map((source) => source.id);
  const inCategory = (addon: AddonSummary) =>
    !query.category || addon.categories.includes(query.category);

  const matches = CATALOG.filter((addon) => {
    if (!searched.includes(addon.id.source)) return false;
    if (/^\d+$/.test(text)) return addon.id.source === "curseforge" && addon.id.key === text;

    const haystack = `${addon.name} ${addon.summary} ${addon.author ?? ""}`.toLowerCase();
    return words.every((word) => haystack.includes(word));
  });

  const sources = searched.map((source) => ({
    source,
    count: matches.filter((addon) => addon.id.source === source && inCategory(addon)).length,
  }));
  const included = matches.filter((addon) => includes(addon.id.source));
  const categories = [...new Set(included.flatMap((addon) => addon.categories))]
    .sort()
    .map((name) => ({
      name,
      count: included.filter((addon) => addon.categories.includes(name)).length,
    }));
  const addons = included.filter(inCategory).sort(compareBy(query.sort));

  return delay({ addons, text_matches: matches.length, sources, categories, notices: [] });
}

/** Mirrors `Sort::compare`: a missing value goes last in either direction,
 * and ties fall back to name, then id. */
function compareBy(sort: Sort) {
  const sign = sort.direction === "ascending" ? 1 : -1;
  const name = (addon: AddonSummary) => addon.name.toLowerCase();
  const date = (addon: AddonSummary) => {
    const time = addon.updated_at ? Date.parse(addon.updated_at) : NaN;
    return Number.isNaN(time) ? null : time;
  };
  const key = (addon: AddonSummary): number | string | null => {
    switch (sort.field) {
      case "downloads":
        return addon.downloads;
      case "updated":
        return date(addon);
      case "name":
        return name(addon);
      case "author":
        return addon.author?.trim().toLowerCase() || null;
    }
  };
  const order = (a: number | string, b: number | string) => (a < b ? -1 : a > b ? 1 : 0);

  return (a: AddonSummary, b: AddonSummary) => {
    const [left, right] = [key(a), key(b)];
    const byField =
      left === null || right === null
        ? Number(left === null) - Number(right === null)
        : sign * order(left, right);

    return byField || order(name(a), name(b)) || order(addonKey(a.id), addonKey(b.id));
  };
}

export function getAddonDetail(id: AddonId): Promise<AddonDetail> {
  const found = CATALOG.find((addon) => addonKey(addon.id) === addonKey(id));
  if (!found) return Promise.reject(`${addonKey(id)} is not in the catalog.`);

  return delay(
    {
      ...found,
      description: DESCRIPTIONS[addonKey(id)] ?? {
        format: "markdown",
        text: `## Changes\n\n- Fixed the \`/guide\` command\n- Added **Forever** support\n\n> ${found.summary}`,
      },
      website_url: "https://github.com/",
      screenshots: [],
    },
    320,
  );
}

export const listInstalled = () => delay(installed, 80);

export function installAddon(id: AddonId): Promise<InstalledView> {
  const found = CATALOG.find((addon) => addonKey(addon.id) === addonKey(id));
  if (!found) return Promise.reject(`${addonKey(id)} is not in the catalog.`);

  emit("install:progress", { addon_id: id, stage: "resolving" });
  window.setTimeout(
    () => emit("install:progress", { addon_id: id, stage: "downloading", received: 620_000, total: 1_100_000 }),
    200,
  );
  window.setTimeout(() => emit("install:progress", { addon_id: id, stage: "extracting" }), 500);

  const folder = found.name.replace(/\W+/g, "");
  installed = {
    ...installed,
    managed: [
      ...installed.managed.filter((entry) => addonKey(entry.id) !== addonKey(id)),
      managed(found, [folder], found.version ?? "1.0.0", { status: "up-to-date" }),
    ],
  };

  return delay(installed, 800);
}

export function uninstallAddon(id: AddonId) {
  installed = {
    ...installed,
    managed: installed.managed.filter((entry) => addonKey(entry.id) !== addonKey(id)),
  };
  return delay(installed, 300);
}

export function openUrl(url: string) {
  window.open(url, "_blank", "noopener");
  return Promise.resolve();
}

// A tiny event bus so the progress bar has something to render in the browser.
const listeners = new Map<string, Set<(payload: unknown) => void>>();

export function listen<T>(event: string, handler: (payload: T) => void) {
  const set = listeners.get(event) ?? new Set();
  set.add(handler as (payload: unknown) => void);
  listeners.set(event, set);

  return Promise.resolve(() => {
    set.delete(handler as (payload: unknown) => void);
  });
}

function emit(event: string, payload: unknown) {
  listeners.get(event)?.forEach((handler) => handler(payload));
}
