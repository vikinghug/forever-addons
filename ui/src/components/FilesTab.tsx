import { useCallback, useEffect, useRef, useState } from "react";

import { api, messageOf } from "../api";
import { ago, bytes, count, day } from "../format";
import { fileById, groupOf, nameOf } from "../relations";
import type { AddonId, FileHistory, PublishedFile } from "../types";
import { DescriptionView } from "./Description";

interface Props {
  addonId: AddonId;
  pageUrl: string;
  history: FileHistory;
  onOpen: (url: string) => void;
}

/** Channels are cumulative: showing betas means releases and betas. */
const RANK = { release: 0, beta: 1, alpha: 2, unknown: 2 } as const;
const CHANNELS = [
  { rank: 0, label: "Releases" },
  { rank: 1, label: "+ Beta" },
  { rank: 2, label: "+ Alpha" },
];
/** Changelogs are one request each; the digest stops here. */
const DIGEST_LIMIT = 5;

const rankOf = (file: PublishedFile) => RANK[file.channel.kind];

/**
 * The file history, anchored on what is installed: newer files carry the
 * frost stripe, the installed one moss, and the changelogs between them read
 * as one "what changed" digest above the list.
 */
export function FilesTab({ addonId, pageUrl, history, onOpen }: Props) {
  const target = fileById(history, history.target);
  const installed = fileById(history, history.installed);
  const [shown, setShown] = useState(() => initialRank(history));
  const [open, setOpen] = useState<string | null>(null);
  const changelogs = useChangelogs(addonId);

  const installedAt = installed ? history.files.indexOf(installed) : history.files.length;
  const incoming = incomingFiles(history, target, installed);
  const visible = history.files.filter((file) => rankOf(file) <= shown);
  const load = changelogs.load;
  const digest = incoming.slice(0, DIGEST_LIMIT).map((file) => file.id).join(",");

  useEffect(() => {
    if (digest) load(digest.split(","));
  }, [digest, load]);

  const toggle = (file: PublishedFile) => {
    const next = open === file.id ? null : file.id;
    setOpen(next);
    if (next) load([next]);
  };

  if (history.files.length === 0) {
    return <p className="prose">This addon has no files for WoW Forever.</p>;
  }

  return (
    <div className="files">
      <div className="files-summary">
        <Headline history={history} target={target} installed={installed} newer={incoming.length} />
        <div className="seg" role="group" aria-label="Channels shown">
          {CHANNELS.map((channel) => (
            <button
              key={channel.rank}
              type="button"
              aria-pressed={shown === channel.rank}
              onClick={() => setShown(channel.rank)}
            >
              {channel.label}
            </button>
          ))}
        </div>
      </div>

      {installed && incoming.length > 0 && (
        <section className="digest" aria-label={`Changes since ${installed.name}`}>
          <h3>What changed since {installed.name}</h3>
          <ol>
            {incoming.slice(0, DIGEST_LIMIT).map((file) => (
              <li key={file.id}>
                <span className="digest-version">{file.name}</span>
                <Changelog state={changelogs.get(file.id)} onOpen={onOpen} />
              </li>
            ))}
          </ol>
          {incoming.length > DIGEST_LIMIT && (
            <p className="digest-more">
              and {incoming.length - DIGEST_LIMIT} older builds; open one below for its changelog.
            </p>
          )}
        </section>
      )}

      <div className="timeline">
        {visible.map((file) => {
          const at = history.files.indexOf(file);
          const mark = file === installed ? "installed" : at < installedAt && installed ? "newer" : undefined;
          const isOpen = open === file.id;

          return (
            <div key={file.id} className="file" data-mark={mark} data-open={isOpen || undefined}>
              <button type="button" className="file-row" aria-expanded={isOpen} onClick={() => toggle(file)}>
                <span className="file-caret" aria-hidden="true">▶</span>
                <span className="file-main">
                  <span className="file-name">{file.name}</span>
                  {file.channel.kind !== "release" && (
                    <span className="channel">
                      {file.channel.kind === "unknown" ? `type ${file.channel.code}` : file.channel.kind}
                    </span>
                  )}
                  {file === installed && <span className="tag" data-tone="moss">Installed</span>}
                  {file === target && file !== installed && (
                    <span className="tag" data-tone={installed ? "frost" : "mute"}>Install gets this</span>
                  )}
                </span>
                <span className="file-meta">
                  <span className="stat" data-wide>{count(file.downloads)} ↓</span>
                  {file.size !== null && <span className="stat" data-wide>{bytes(file.size)}</span>}
                  <span className="stat" title={day(file.published_at)}>{ago(file.published_at)}</span>
                </span>
              </button>
              {isOpen && (
                <FileDetail
                  file={file}
                  history={history}
                  changelog={changelogs.get(file.id)}
                  pageUrl={pageUrl}
                  onOpen={onOpen}
                />
              )}
            </div>
          );
        })}
      </div>
    </div>
  );
}

function Headline({
  history,
  target,
  installed,
  newer,
}: {
  history: FileHistory;
  target: PublishedFile | undefined;
  installed: PublishedFile | undefined;
  newer: number;
}) {
  if (installed && newer > 0) {
    return (
      <span className="files-headline">
        You have {installed.name} · <em>{newer} newer {newer === 1 ? "build" : "builds"}</em>
      </span>
    );
  }
  if (installed) {
    return <span className="files-headline">You have {installed.name}, the newest build an install would pick</span>;
  }
  if (history.installed === null && target) {
    return <span className="files-headline">Install gets {target.name}</span>;
  }
  return <span className="files-headline">{history.files.length} files</span>;
}

function FileDetail({
  file,
  history,
  changelog,
  pageUrl,
  onOpen,
}: {
  file: PublishedFile;
  history: FileHistory;
  changelog: ChangelogState | undefined;
  pageUrl: string;
  onOpen: (url: string) => void;
}) {
  const needs = file.dependencies
    .filter((dependency) => groupOf(dependency.relation) === "needs")
    .map((dependency) => {
      const found = history.related.find(
        (addon) => addon.id.source === dependency.addon.source && addon.id.key === dependency.addon.key,
      );
      return found ? nameOf(found) : dependency.addon.key;
    });

  return (
    <div className="file-detail">
      <div>
        <span className="label">Changelog</span>
        <Changelog state={changelog} onOpen={onOpen} />
      </div>
      <dl className="facts">
        <dt>Installs</dt>
        <dd>{file.folders.join(", ") || "not listed"}</dd>
        <dt>File</dt>
        <dd>{file.file_name}</dd>
        <dt>Needs</dt>
        <dd>{needs.join(", ") || "nothing extra"}</dd>
        <dt>Published</dt>
        <dd>{day(file.published_at)}</dd>
      </dl>
      <div className="file-actions">
        <button
          type="button"
          className="btn"
          onClick={() => onOpen(`${pageUrl.replace(/\/+$/, "")}/files/${file.id}`)}
        >
          Open this file's page
        </button>
      </div>
    </div>
  );
}

type ChangelogState =
  | { state: "loading" }
  | { state: "loaded"; html: string }
  | { state: "failed"; message: string };

function Changelog({ state, onOpen }: { state: ChangelogState | undefined; onOpen: (url: string) => void }) {
  if (!state || state.state === "loading") return <p className="prose changelog">Loading…</p>;
  if (state.state === "failed") return <p className="prose changelog">Could not load the changelog: {state.message}</p>;
  if (!state.html) return <p className="prose changelog">No changelog published.</p>;
  return (
    <div className="changelog">
      <DescriptionView html={state.html} onOpen={onOpen} />
    </div>
  );
}

/** Changelogs fetched on demand, each once. */
function useChangelogs(addonId: AddonId) {
  const [entries, setEntries] = useState<Record<string, ChangelogState>>({});
  const requested = useRef(new Set<string>());

  const load = useCallback(
    (ids: string[]) => {
      const set = (id: string, state: ChangelogState) =>
        setEntries((current) => ({ ...current, [id]: state }));

      for (const id of ids) {
        if (requested.current.has(id)) continue;
        requested.current.add(id);
        set(id, { state: "loading" });
        api
          .getFileChangelog(addonId, id)
          .then((html) => set(id, { state: "loaded", html }))
          .catch((cause) => set(id, { state: "failed", message: messageOf(cause) }));
      }
    },
    [addonId],
  );

  return { get: (id: string) => entries[id], load };
}

/**
 * Files an update would move past: newer than the installed one, up to the
 * target, and no less stable than the target.
 */
function incomingFiles(
  history: FileHistory,
  target: PublishedFile | undefined,
  installed: PublishedFile | undefined,
): PublishedFile[] {
  if (!target || !installed) return [];

  const from = history.files.indexOf(target);
  const to = history.files.indexOf(installed);
  return history.files
    .slice(from, Math.max(from, to))
    .filter((file) => rankOf(file) <= rankOf(target));
}

/** Wide enough to show the installed and target files. */
function initialRank(history: FileHistory): number {
  const anchors = [fileById(history, history.target), fileById(history, history.installed)];
  return Math.max(0, ...anchors.flatMap((file) => (file ? [rankOf(file)] : [])));
}
