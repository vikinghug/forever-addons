import { useCallback, useEffect, useRef, useState } from "react";

import {
  api,
  chooseArchive,
  chooseFolder,
  messageOf,
  onCatalogProgress,
  onInstallProgress,
} from "./api";
import type {
  AddonDetail,
  AddonId,
  AddonSummary,
  AppStatus,
  BrowseQuery,
  InstalledView,
  SearchResults,
  SourceId,
  SourceStatus,
} from "./types";
import { DEFAULT_SORT, addonKey } from "./types";
import { bytes } from "./format";
import { Browse, stateOf } from "./components/Browse";
import { DetailPane } from "./components/DetailPane";
import { Installed } from "./components/Installed";
import { ProgressBar } from "./components/ProgressBar";
import { Sources } from "./components/Sources";
import { TopBar, type View } from "./components/TopBar";

const EMPTY_INSTALLED: InstalledView = { managed: [], unmanaged: [] };

const EMPTY_RESULTS: SearchResults = {
  addons: [],
  text_matches: 0,
  sources: [],
  categories: [],
  notices: [],
};

const DEFAULT_QUERY: BrowseQuery = {
  text: "",
  category: null,
  sources: null,
  sort: DEFAULT_SORT,
  hideInstalled: false,
};

/** A live source answers once its key is saved; a catalog source once pulled. */
const isBrowsable = (source: SourceStatus) =>
  source.listing === "live"
    ? source.enabled && source.api_key_configured
    : source.fetched_at !== null;

export default function App() {
  const [view, setView] = useState<View>("browse");
  const [status, setStatus] = useState<AppStatus | null>(null);
  const [results, setResults] = useState<SearchResults>(EMPTY_RESULTS);
  const [installed, setInstalled] = useState<InstalledView>(EMPTY_INSTALLED);

  const [query, setQuery] = useState<BrowseQuery>(DEFAULT_QUERY);
  const [searching, setSearching] = useState(true);

  const [selected, setSelected] = useState<AddonSummary | null>(null);
  const [detail, setDetail] = useState<AddonDetail | null>(null);
  const [detailError, setDetailError] = useState<string | null>(null);

  const [busy, setBusy] = useState<Record<string, string>>({});
  const [refreshing, setRefreshing] = useState<SourceId | null>(null);
  const [pull, setPull] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  const canInstall = status?.install_valid ?? false;
  const browsable = (status?.sources ?? []).filter(isBrowsable);

  // What a search depends on. A status change that only records a live
  // source's search access must not trigger another search.
  const searchInputs = (status?.sources ?? [])
    .map((source) =>
      [source.id, source.enabled, source.api_key_configured, source.fetched_at].join(":"),
    )
    .join("|");

  // --- loading ------------------------------------------------------------

  const reloadInstalled = useCallback(async () => {
    if (!canInstall) {
      setInstalled(EMPTY_INSTALLED);
      return;
    }
    try {
      setInstalled(await api.listInstalled());
    } catch (cause) {
      setError(messageOf(cause));
    }
  }, [canInstall]);

  useEffect(() => {
    api.getStatus().then(setStatus).catch((cause) => setError(messageOf(cause)));
  }, []);

  useEffect(() => {
    void reloadInstalled();
  }, [reloadInstalled]);

  // The picked sources that can still answer, as a string so an unchanged
  // pick does not search again. Empty means every source.
  const sourceFilter = (query.sources ?? [])
    .filter((id) => browsable.some((source) => source.id === id))
    .join(",");

  // Searching is debounced so typing does not query CurseForge per keypress,
  // and numbered so a slow answer to an older query cannot replace a newer one.
  const searchSeq = useRef(0);

  useEffect(() => {
    setSearching(true);
    const seq = ++searchSeq.current;
    const timer = window.setTimeout(async () => {
      try {
        const found = await api.searchAddons({
          text: query.text,
          category: query.category,
          sources: sourceFilter ? (sourceFilter.split(",") as SourceId[]) : null,
          sort: query.sort,
        });
        if (seq !== searchSeq.current) return;
        setResults(found);
        setError(null);
      } catch (cause) {
        if (seq === searchSeq.current) setError(messageOf(cause));
      } finally {
        if (seq === searchSeq.current) setSearching(false);
      }
    }, 350);

    return () => window.clearTimeout(timer);
  }, [query.text, query.category, sourceFilter, query.sort, searchInputs]);

  // The Sources screen shows what browsing learned about search access.
  const navigate = (next: View) => {
    setView(next);
    if (next === "sources") {
      api.getStatus().then(setStatus).catch((cause) => setError(messageOf(cause)));
    }
  };

  // --- live progress ------------------------------------------------------

  const unlisten = useRef<Array<() => void>>([]);

  useEffect(() => {
    const subscriptions = [
      onCatalogProgress((progress) => {
        const total = progress.total_pages ? ` of ${progress.total_pages}` : "";
        setPull(`Page ${progress.page}${total} · ${progress.addons_so_far} addons`);
      }),
      onInstallProgress((progress) => {
        setBusy((current) => ({
          ...current,
          [addonKey(progress.addon_id)]: describe(progress),
        }));
      }),
    ];

    Promise.all(subscriptions).then((offs) => {
      unlisten.current = offs;
    });

    return () => {
      unlisten.current.forEach((off) => off());
      unlisten.current = [];
    };
  }, []);

  // --- actions ------------------------------------------------------------

  /** Runs one install or removal; false when it failed and said why. */
  const withBusy = async (
    id: AddonId,
    label: string,
    work: () => Promise<InstalledView>,
  ): Promise<boolean> => {
    const key = addonKey(id);
    setBusy((current) => ({ ...current, [key]: label }));
    setError(null);

    try {
      setInstalled(await work());
      return true;
    } catch (cause) {
      setError(messageOf(cause));
      return false;
    } finally {
      setBusy(({ [key]: _dropped, ...rest }) => rest);
    }
  };

  /** Installs `id`, then each requirement it lacks, stopping at a failure. */
  const install = async (id: AddonId, withAddons: AddonId[] = []) => {
    for (const next of [id, ...withAddons]) {
      if (!(await withBusy(next, "Installing…", () => api.installAddon(next)))) return;
    }
  };
  const installFile = async (id: AddonId, name: string) => {
    const path = await chooseArchive(name);
    if (!path) return;
    await withBusy(id, "Installing…", () => api.installAddonFile(id, path));
  };
  const remove = (id: AddonId) => withBusy(id, "Removing…", () => api.uninstallAddon(id));

  const pickFolder = async () => {
    const chosen = await chooseFolder(status?.client_path ?? null);
    if (!chosen) return;

    try {
      setStatus(await api.setClientPath(chosen));
      setError(null);
    } catch (cause) {
      setError(messageOf(cause));
    }
  };

  const refresh = async (source: SourceId) => {
    setRefreshing(source);
    setPull("Starting…");
    setError(null);

    try {
      setStatus(await api.refreshSource(source));
    } catch (cause) {
      setError(messageOf(cause));
    } finally {
      setRefreshing(null);
      setPull(null);
    }
  };

  const toggleSource = async (source: SourceId, enabled: boolean) => {
    const next = (status?.sources ?? [])
      .filter((entry) => (entry.id === source ? enabled : entry.enabled))
      .map((entry) => entry.id);

    try {
      setStatus(await api.setEnabledSources(next));
    } catch (cause) {
      setError(messageOf(cause));
    }
  };

  const saveApiKey = async (source: SourceId, key: string | null) => {
    try {
      setStatus(
        source === "wago" ? await api.setWagoToken(key) : await api.setCurseforgeApiKey(key),
      );
      setError(null);
    } catch (cause) {
      setError(messageOf(cause));
    }
  };

  const addRepo = async (repo: string) => {
    try {
      setStatus(await api.addGithubRepo(repo));
      setError(null);
      // A fresh repo is only browsable once its release is cataloged.
      await refresh("github");
    } catch (cause) {
      setError(messageOf(cause));
    }
  };

  const removeRepo = async (repo: string) => {
    try {
      setStatus(await api.removeGithubRepo(repo));
      setError(null);
    } catch (cause) {
      setError(messageOf(cause));
    }
  };

  const openDetail = async (addon: AddonSummary) => {
    setSelected(addon);
    setDetail(null);
    setDetailError(null);

    try {
      const found = await api.getAddonDetail(addon.id);
      setDetail(found);
    } catch (cause) {
      setDetailError(messageOf(cause));
    }
  };

  const openUrl = (url: string) => {
    api.openUrl(url).catch((cause) => setError(messageOf(cause)));
  };

  return (
    <div className="shell">
      <TopBar
        view={view}
        status={status}
        catalogCount={results.addons.length}
        installedCount={installed.managed.length + installed.unmanaged.length}
        onNavigate={navigate}
        onChooseFolder={pickFolder}
      />

      {status && !status.install_valid && (
        <div className="banner">
          <span className="banner-dot" aria-hidden="true">●</span>
          <span>
            {status.install_error ??
              "Choose your WoW Forever folder to install anything."}
          </span>
          <span className="toolbar-spacer" />
          <button type="button" className="btn" onClick={pickFolder}>
            Choose folder
          </button>
        </div>
      )}

      {error && (
        <div className="banner">
          <span className="banner-dot" aria-hidden="true">●</span>
          <span>{error}</span>
          <span className="toolbar-spacer" />
          <button type="button" className="btn" data-variant="quiet" onClick={() => setError(null)}>
            Dismiss
          </button>
        </div>
      )}

      {pull && <ProgressBar label={`Pulling catalog · ${pull}`} fraction={null} />}

      {view === "browse" && (
        <main className="browse">
          <Browse
            results={results}
            browsable={browsable}
            query={query}
            installed={installed}
            selected={selected ? addonKey(selected.id) : null}
            busy={busy}
            loading={searching}
            onQuery={(change) => setQuery((current) => ({ ...current, ...change }))}
            onSelect={openDetail}
            onGoToSources={() => navigate("sources")}
          />
          <DetailPane
            addon={selected}
            detail={detail}
            error={detailError}
            state={selected ? stateOf(selected, installed) : "none"}
            installed={installed}
            busyByKey={busy}
            canInstall={canInstall}
            onClose={() => setSelected(null)}
            onOpen={openUrl}
            onInstall={(withAddons) => selected && install(selected.id, withAddons)}
            onInstallOther={(id) => install(id)}
            onRemoveOther={remove}
            onSelect={openDetail}
            onInstallFile={() => selected && installFile(selected.id, selected.name)}
            onRemove={() => selected && remove(selected.id)}
          />
        </main>
      )}

      {view === "installed" && (
        <main className="scroller">
          <Installed
            installed={installed}
            busy={busy}
            onUpdate={(addon) => install(addon.id)}
            onInstallFile={(addon) => installFile(addon.id, addon.name)}
            onRemove={(addon) => remove(addon.id)}
            onOpen={openUrl}
            onGoToBrowse={() => navigate("browse")}
          />
        </main>
      )}

      {view === "sources" && (
        <main className="scroller">
          <Sources
            status={status}
            refreshing={refreshing}
            onRefresh={refresh}
            onToggle={toggleSource}
            onOpen={openUrl}
            onSaveApiKey={saveApiKey}
            onAddRepo={addRepo}
            onRemoveRepo={removeRepo}
          />
        </main>
      )}
    </div>
  );
}

function describe(progress: Parameters<Parameters<typeof onInstallProgress>[0]>[0]): string {
  switch (progress.stage) {
    case "resolving":
      return "Resolving…";
    case "downloading":
      return progress.total
        ? `${Math.round((progress.received / progress.total) * 100)}%`
        : bytes(progress.received);
    case "extracting":
      return "Extracting…";
    case "installed":
      return "Done";
  }
}
