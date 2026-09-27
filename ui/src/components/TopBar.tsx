import type { AppStatus } from "../types";

export type View = "browse" | "installed" | "sources";

interface Props {
  view: View;
  status: AppStatus | null;
  catalogCount: number;
  installedCount: number;
  onNavigate: (view: View) => void;
  onChooseFolder: () => void;
}

export function TopBar({
  view,
  status,
  catalogCount,
  installedCount,
  onNavigate,
  onChooseFolder,
}: Props) {
  const enabledSources = status?.sources.filter((source) => source.enabled).length ?? 0;
  const path = status?.client_path ?? null;

  return (
    <header className="topbar">
      <div className="brand">
        <span className="brand-mark" aria-hidden="true">∞</span>
        <span className="brand-name">Forever Addons</span>
      </div>

      <nav className="tabs">
        <Tab
          label="Browse"
          count={catalogCount}
          current={view === "browse"}
          onClick={() => onNavigate("browse")}
        />
        <Tab
          label="Installed"
          count={installedCount}
          current={view === "installed"}
          onClick={() => onNavigate("installed")}
        />
        <Tab
          label="Sources"
          count={enabledSources}
          current={view === "sources"}
          onClick={() => onNavigate("sources")}
        />
      </nav>

      <div className="client">
        <span className="label">Client</span>
        {/* Long paths are cut from the left: the folder name is the part that tells installs apart. */}
        <span className="client-path" data-state={path ? "set" : "unset"} title={path ?? undefined}>
          <bdi>{path ?? "No folder chosen yet"}</bdi>
        </span>
        {status && !status.install_valid && path && (
          <span className="tag" data-tone="rust">Not a client folder</span>
        )}
        <button type="button" className="btn" onClick={onChooseFolder}>
          {path ? "Change folder" : "Choose folder"}
        </button>
      </div>
    </header>
  );
}

function Tab({
  label,
  count,
  current,
  onClick,
}: {
  label: string;
  count: number;
  current: boolean;
  onClick: () => void;
}) {
  return (
    <button type="button" className="tab" aria-current={current} onClick={onClick}>
      <span>{label}</span>
      <span className="tab-count">{count}</span>
    </button>
  );
}
