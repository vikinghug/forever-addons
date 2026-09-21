// The one place the UI talks to the Rust core.
//
// Outside a Tauri window — `npm --prefix ui run dev` — every call is answered by
// the mock backend instead, so the interface can be built and reviewed without
// a WoW client on the machine.

import type {
  AddonDetail,
  AddonSummary,
  AddonId,
  AppStatus,
  CatalogQuery,
  FetchProgress,
  InstallProgress,
  InstalledView,
  SourceId,
} from "./types";
import * as mock from "./mock";

export const isDesktop = "__TAURI_INTERNALS__" in window;

async function call<T>(command: string, args?: Record<string, unknown>): Promise<T> {
  const { invoke } = await import("@tauri-apps/api/core");
  return invoke<T>(command, args);
}

export const api = {
  getStatus: (): Promise<AppStatus> =>
    isDesktop ? call("get_status") : mock.getStatus(),

  detectWowInstall: (): Promise<string | null> =>
    isDesktop ? call("detect_wow_install") : mock.detectWowInstall(),

  setClientPath: (path: string): Promise<AppStatus> =>
    isDesktop ? call("set_client_path", { path }) : mock.setClientPath(path),

  setEnabledSources: (sources: SourceId[]): Promise<AppStatus> =>
    isDesktop ? call("set_enabled_sources", { sources }) : mock.setEnabledSources(sources),

  setCurseforgeApiKey: (key: string | null): Promise<AppStatus> =>
    isDesktop ? call("set_curseforge_api_key", { key }) : mock.setCurseforgeApiKey(key),

  setWagoToken: (token: string | null): Promise<AppStatus> =>
    isDesktop ? call("set_wago_token", { token }) : mock.setWagoToken(token),

  addGithubRepo: (repo: string): Promise<AppStatus> =>
    isDesktop ? call("add_github_repo", { repo }) : mock.addGithubRepo(repo),

  removeGithubRepo: (repo: string): Promise<AppStatus> =>
    isDesktop ? call("remove_github_repo", { repo }) : mock.removeGithubRepo(repo),

  refreshSource: (source: SourceId): Promise<AppStatus> =>
    isDesktop ? call("refresh_source", { source }) : mock.refreshSource(source),

  searchCatalog: (query: CatalogQuery): Promise<AddonSummary[]> =>
    isDesktop ? call("search_catalog", { query }) : mock.searchCatalog(query),

  listCategories: (): Promise<string[]> =>
    isDesktop ? call("list_categories") : mock.listCategories(),

  getAddonDetail: (id: AddonId): Promise<AddonDetail> =>
    isDesktop ? call("get_addon_detail", { id }) : mock.getAddonDetail(id),

  listInstalled: (): Promise<InstalledView> =>
    isDesktop ? call("list_installed") : mock.listInstalled(),

  installAddon: (id: AddonId): Promise<InstalledView> =>
    isDesktop ? call("install_addon", { id }) : mock.installAddon(id),

  uninstallAddon: (id: AddonId): Promise<InstalledView> =>
    isDesktop ? call("uninstall_addon", { id }) : mock.uninstallAddon(id),

  openUrl: (url: string): Promise<void> =>
    isDesktop ? call("open_url", { url }) : mock.openUrl(url),
};

/** Opens the native folder picker, or asks for a typed path in the browser. */
export async function chooseFolder(current: string | null): Promise<string | null> {
  if (!isDesktop) {
    return (
      window.prompt(
        "Path to your WoW Forever folder (e.g. …/World of Warcraft/_classic_beta_)",
        current ?? "",
      ) || null
    );
  }

  const { open } = await import("@tauri-apps/plugin-dialog");
  const chosen = await open({
    directory: true,
    multiple: false,
    title: "Select your WoW Forever folder",
    defaultPath: current ?? undefined,
  });

  return typeof chosen === "string" ? chosen : null;
}

type Unlisten = () => void;

async function listen<T>(event: string, handler: (payload: T) => void): Promise<Unlisten> {
  if (!isDesktop) return mock.listen(event, handler);

  const { listen: tauriListen } = await import("@tauri-apps/api/event");
  return tauriListen<T>(event, (message) => handler(message.payload));
}

export const onCatalogProgress = (handler: (progress: FetchProgress) => void) =>
  listen<FetchProgress>("catalog:progress", handler);

export const onInstallProgress = (handler: (progress: InstallProgress) => void) =>
  listen<InstallProgress>("install:progress", handler);

/** Turns an error from a command into the sentence the Rust side wrote. */
export function messageOf(error: unknown): string {
  if (typeof error === "string") return error;
  if (error instanceof Error) return error.message;
  return "Something went wrong.";
}
