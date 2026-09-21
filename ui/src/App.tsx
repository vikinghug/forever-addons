import { useCallback, useEffect, useMemo, useRef, useState } from "react";

import {
  api,
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
  InstalledView,
  SourceId,
} from "./types";
import { addonKey } from "./types";
import { bytes } from "./format";
import { Browse, stateOf } from "./components/Browse";
import { DetailDrawer } from "./components/DetailDrawer";
import { Installed } from "./components/Installed";
import { ProgressBar } from "./components/ProgressBar";
import { Rail, type View } from "./components/Rail";
import { Sources } from "./components/Sources";

const EMPTY_INSTALLED: InstalledView = { managed: [], unmanaged: [] };

export default function App() {
  const [view, setView] = useState<View>("browse");
  const [status, setStatus] = useState<AppStatus | null>(null);
  const [catalog, setCatalog] = useState<AddonSummary[]>([]);
  const [categories, setCategories] = useState<string[]>([]);
  const [installed, setInstalled] = useState<InstalledView>(EMPTY_INSTALLED);

  const [text, setText] = useState("");
  const [category, setCategory] = useState("");
  const [searching, setSearching] = useState(true);

  const [selected, setSelected] = useState<AddonSummary | null>(null);
  const [detail, setDetail] = useState<AddonDetail | null>(null);
  const [detailError, setDetailError] = useState<string | null>(null);

  const [busy, setBusy] = useState<Record<string, string>>({});
  const [refreshing, setRefreshing] = useState<SourceId | null>(null);
  const [pull, setPull] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  const canInstall = status?.install_valid ?? false;
  const hasCatalog = (status?.sources ?? []).some((source) => source.fetched_at !== null);

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

  useEffect(() => {
    api.listCategories().then(setCategories).catch(() => setCategories([]));
  }, [status]);

  // Searching is debounced so typing does not re-query the catalog per keypress.
  useEffect(() => {
    setSearching(true);
    const timer = window.setTimeout(async () => {
      try {
        setCatalog(
          await api.searchCatalog({
            text,
            category: category || null,
            sources: null,
          }),
        );
        setError(null);
      } catch (cause) {
        setError(messageOf(cause));
      } finally {
        setSearching(false);
      }
    }, 180);

    return () => window.clearTimeout(timer);
  }, [text, category, status]);

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

  const withBusy = async (id: AddonId, label: string, work: () => Promise<InstalledView>) => {
    const key = addonKey(id);
    setBusy((current) => ({ ...current, [key]: label }));
    setError(null);

    try {
      setInstalled(await work());
    } catch (cause) {
      setError(messageOf(cause));
    } finally {
      setBusy(({ [key]: _dropped, ...rest }) => rest);
    }
  };

  const install = (id: AddonId) => withBusy(id, "Installing…", () => api.installAddon(id));
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

  const title = useMemo(() => {
    if (view === "browse") return "Browse addons";
    if (view === "installed") return "Installed addons";
    return "Addon sources";
  }, [view]);

  return (
    <div className="shell" data-drawer={selected ? "open" : "closed"}>
      <Rail
        view={view}
        status={status}
        catalogCount={catalog.length}
        installedCount={installed.managed.length + installed.unmanaged.length}
        onNavigate={setView}
        onChooseFolder={pickFolder}
      />

      <main className="main">
        <div className="toolbar">
          <h1 className="view-title">{title}</h1>

          {view === "browse" && (
            <>
              <div className="field">
                <span className="field-icon" aria-hidden="true">⌕</span>
                <input
                  type="search"
                  value={text}
                  placeholder="Search by name, author, or description"
                  aria-label="Search addons"
                  onChange={(event) => setText(event.target.value)}
                />
              </div>
              <select
                value={category}
                aria-label="Filter by category"
                onChange={(event) => setCategory(event.target.value)}
              >
                <option value="">All categories</option>
                {categories.map((name) => (
                  <option key={name} value={name}>
                    {name}
                  </option>
                ))}
              </select>
            </>
          )}

          <span className="toolbar-spacer" />

          {view === "browse" && (
            <span className="result-count">
              {searching ? "searching…" : `${catalog.length} shown`}
            </span>
          )}
        </div>

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

        <div className="scroller">
          {view === "browse" && (
            <Browse
              addons={catalog}
              installed={installed}
              selected={selected ? addonKey(selected.id) : null}
              busy={busy}
              canInstall={canInstall}
              loading={searching}
              hasCatalog={hasCatalog}
              onSelect={openDetail}
              onInstall={(addon) => install(addon.id)}
              onRemove={(addon) => remove(addon.id)}
              onGoToSources={() => setView("sources")}
            />
          )}

          {view === "installed" && (
            <Installed
              installed={installed}
              busy={busy}
              onUpdate={(addon) => install(addon.id)}
              onRemove={(addon) => remove(addon.id)}
              onOpen={openUrl}
              onGoToBrowse={() => setView("browse")}
            />
          )}

          {view === "sources" && (
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
          )}
        </div>
      </main>

      {selected && (
        <DetailDrawer
          detail={detail}
          loading={detail === null && detailError === null}
          error={detailError}
          state={stateOf(selected, installed)}
          busy={busy[addonKey(selected.id)] ?? null}
          canInstall={canInstall}
          onClose={() => setSelected(null)}
          onOpen={openUrl}
          onInstall={() => install(selected.id)}
          onRemove={() => remove(selected.id)}
        />
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
