import { useState } from "react";

import type { AppStatus, SourceId, SourceStatus } from "../types";
import { when } from "../format";

interface Props {
  status: AppStatus | null;
  refreshing: SourceId | null;
  onRefresh: (source: SourceId) => void;
  onToggle: (source: SourceId, enabled: boolean) => void;
  onOpen: (url: string) => void;
  onSaveApiKey: (source: SourceId, key: string | null) => void;
  onAddRepo: (repo: string) => void;
  onRemoveRepo: (repo: string) => void;
}

export function Sources({
  status,
  refreshing,
  onRefresh,
  onToggle,
  onOpen,
  onSaveApiKey,
  onAddRepo,
  onRemoveRepo,
}: Props) {
  if (!status) return null;

  return (
    <div className="source-list">
      <p className="prose">
        Pulling a source caches its whole catalog on disk; nothing is fetched again until you
        refresh it here.
      </p>

      {status.sources.map((source) => (
        <SourceCard
          key={source.id}
          source={source}
          refreshing={refreshing}
          onRefresh={onRefresh}
          onToggle={onToggle}
          onOpen={onOpen}
          onSaveApiKey={onSaveApiKey}
          onAddRepo={onAddRepo}
          onRemoveRepo={onRemoveRepo}
        />
      ))}
    </div>
  );
}

function SourceCard({
  source,
  refreshing,
  onRefresh,
  onToggle,
  onOpen,
  onSaveApiKey,
  onAddRepo,
  onRemoveRepo,
}: {
  source: SourceStatus;
  refreshing: SourceId | null;
  onRefresh: (source: SourceId) => void;
  onToggle: (source: SourceId, enabled: boolean) => void;
  onOpen: (url: string) => void;
  onSaveApiKey: (source: SourceId, key: string | null) => void;
  onAddRepo: (repo: string) => void;
  onRemoveRepo: (repo: string) => void;
}) {
  const needsKey = source.requires_api_key && !source.api_key_configured;
  const isGithub = source.id === "github";
  const nothingToPull = isGithub && source.tracked_repos.length === 0;

  return (
    <div className="source-card">
      <div>
        <h3 className="source-name">{source.name}</h3>
        <div className="source-meta">
          <span>{source.addon_count.toLocaleString()} addons</span>
          <span>pulled {when(source.fetched_at)}</span>
          <button type="button" className="link" onClick={() => onOpen(source.site_url)}>
            {new URL(source.site_url).host}
          </button>
        </div>
        {source.requires_api_key && (
          <ApiKeyField source={source} onSave={onSaveApiKey} />
        )}
        {isGithub && (
          <RepoList repos={source.tracked_repos} onAdd={onAddRepo} onRemove={onRemoveRepo} />
        )}
      </div>

      <div className="row-actions">
        <label className="switch">
          <input
            type="checkbox"
            checked={source.enabled}
            onChange={(event) => onToggle(source.id, event.target.checked)}
          />
          Show in browse
        </label>
        <button
          type="button"
          className="btn"
          data-variant={source.fetched_at ? undefined : "primary"}
          disabled={refreshing !== null || needsKey || nothingToPull}
          title={
            needsKey
              ? `Add your ${source.name} key first`
              : nothingToPull
                ? "Track a repository first"
                : undefined
          }
          onClick={() => onRefresh(source.id)}
        >
          {refreshing === source.id ? "Pulling…" : source.fetched_at ? "Refresh" : "Pull catalog"}
        </button>
      </div>
    </div>
  );
}

/** CurseForge and Wago only answer with a personal key/token. */
function ApiKeyField({
  source,
  onSave,
}: {
  source: SourceStatus;
  onSave: (source: SourceId, key: string | null) => void;
}) {
  const [draft, setDraft] = useState("");
  const noun = source.id === "wago" ? "access token" : "API key";
  const from =
    source.id === "wago" ? "addons.wago.io account settings" : "console.curseforge.com";

  const save = () => {
    const trimmed = draft.trim();
    if (!trimmed) return;
    onSave(source.id, trimmed);
    setDraft("");
  };

  return (
    <div className="source-meta api-key">
      {source.api_key_configured ? (
        <>
          <span className="tag" data-tone="moss">{noun} saved</span>
          <button type="button" className="link" onClick={() => onSave(source.id, null)}>
            Clear
          </button>
        </>
      ) : (
        <span className="tag" data-tone="rust">{noun} required</span>
      )}
      <input
        type="password"
        value={draft}
        placeholder={
          source.api_key_configured ? `Replace ${noun}` : `Paste your ${noun} (${from})`
        }
        aria-label={`${source.name} ${noun}`}
        onChange={(event) => setDraft(event.target.value)}
        onKeyDown={(event) => {
          if (event.key === "Enter") save();
        }}
      />
      <button type="button" className="btn" disabled={!draft.trim()} onClick={save}>
        Save
      </button>
    </div>
  );
}

/** The repositories the GitHub source watches for releases. */
function RepoList({
  repos,
  onAdd,
  onRemove,
}: {
  repos: string[];
  onAdd: (repo: string) => void;
  onRemove: (repo: string) => void;
}) {
  const [draft, setDraft] = useState("");

  const add = () => {
    const trimmed = draft.trim();
    if (!trimmed) return;
    onAdd(trimmed);
    setDraft("");
  };

  return (
    <>
      {repos.length > 0 && (
        <div className="source-meta repo-list">
          {repos.map((repo) => (
            <span className="chip" key={repo}>
              {repo}
              <button
                type="button"
                className="link"
                aria-label={`Stop tracking ${repo}`}
                onClick={() => onRemove(repo)}
              >
                ✕
              </button>
            </span>
          ))}
        </div>
      )}
      <div className="source-meta api-key">
        <input
          type="text"
          value={draft}
          placeholder="owner/repo or a github.com URL"
          aria-label="Track a GitHub repository"
          onChange={(event) => setDraft(event.target.value)}
          onKeyDown={(event) => {
            if (event.key === "Enter") add();
          }}
        />
        <button type="button" className="btn" disabled={!draft.trim()} onClick={add}>
          Track
        </button>
      </div>
    </>
  );
}
