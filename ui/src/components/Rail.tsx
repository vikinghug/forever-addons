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

export function Rail({
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
    <nav className="rail">
      <div className="brand">
        <span className="brand-mark" aria-hidden="true">∞</span>
        <span className="brand-name">Forever Addons</span>
      </div>

      <div className="nav">
        <NavItem
          label="Browse"
          count={catalogCount}
          current={view === "browse"}
          onClick={() => onNavigate("browse")}
        />
        <NavItem
          label="Installed"
          count={installedCount}
          current={view === "installed"}
          onClick={() => onNavigate("installed")}
        />
        <NavItem
          label="Sources"
          count={enabledSources}
          current={view === "sources"}
          onClick={() => onNavigate("sources")}
        />
      </div>

      <div className="rail-foot">
        <span className="label">Client</span>
        <div className="client-card">
          <span className="brand-client">World of Warcraft: Forever</span>
          <span className="client-path" data-state={path ? "set" : "unset"}>
            {path ?? "No folder chosen yet"}
          </span>
          {status && !status.install_valid && path && (
            <span className="tag" data-tone="rust">Not a client folder</span>
          )}
          <button type="button" className="btn" onClick={onChooseFolder}>
            {path ? "Change folder" : "Choose folder"}
          </button>
        </div>
      </div>
    </nav>
  );
}

function NavItem({
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
    <button type="button" className="nav-item" aria-current={current} onClick={onClick}>
      <span>{label}</span>
      <span className="nav-count">{count}</span>
    </button>
  );
}
