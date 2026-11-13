// Parlato: the model downloads that are running, in one place.
//
// A download keeps running in the backend when the person leaves the page that
// started it, but that page's progress state is gone. This store listens to
// the progress events for as long as the app window lives and asks
// `list_active_downloads` once at start (and for any download it first hears
// of by event), so the sidebar chip and every card can show the truth.

import { useSyncExternalStore } from "react";
import { listen } from "@tauri-apps/api/event";
import { api, type ActiveDownload } from "@/lib/tauri";

type Kind = ActiveDownload["kind"];
type Entry = ActiveDownload & { at: number };

const EVENTS: { kind: Kind; prefix: string }[] = [
  { kind: "whisper", prefix: "model" },
  { kind: "parakeet", prefix: "parakeet_model" },
  { kind: "llm", prefix: "llm_model" },
  { kind: "vad", prefix: "vad" },
];

/** The voice detection download has no error or cancel event, so a vad entry
 *  that stops reporting is dropped after this long. */
const VAD_STALE_MS = 20_000;

const entries = new Map<string, Entry>();
let snapshot: ActiveDownload[] = [];
const listeners = new Set<() => void>();
let started = 0;
let stop: (() => void) | null = null;

const keyOf = (kind: Kind, id: string) => `${kind}:${id}`;

function publish() {
  snapshot = [...entries.values()].map(({ at: _at, ...d }) => d);
  listeners.forEach((l) => l());
}

function upsert(d: ActiveDownload) {
  entries.set(keyOf(d.kind, d.id), { ...d, at: Date.now() });
}

async function reload() {
  try {
    const list = await api.listActiveDownloads();
    // The list is the truth for what exists; keep a fresher progress reading
    // from an event when it is further along.
    const next = new Map<string, Entry>();
    for (const d of list) {
      const old = entries.get(keyOf(d.kind, d.id));
      next.set(keyOf(d.kind, d.id), {
        ...d,
        downloaded: old && old.downloaded > d.downloaded ? old.downloaded : d.downloaded,
        total: d.total || old?.total || 0,
        at: Date.now(),
      });
    }
    entries.clear();
    next.forEach((v, k) => entries.set(k, v));
    publish();
  } catch (e) {
    // Older backend without the command: events alone still work.
    console.error(e);
  }
}

function begin() {
  const unlisteners: Promise<() => void>[] = [];
  for (const { kind, prefix } of EVENTS) {
    unlisteners.push(
      listen<{ id?: string; downloaded: number; total: number }>(`${prefix}:download:progress`, (e) => {
        const id = e.payload.id ?? kind;
        const known = entries.get(keyOf(kind, id));
        upsert({ id, kind, name: known?.name ?? id, downloaded: e.payload.downloaded, total: e.payload.total });
        publish();
        // First time we hear of it: the list knows its proper name.
        if (!known) void reload();
      }),
    );
    const done = (e: { payload: { id?: string } | null }) => {
      entries.delete(keyOf(kind, e.payload?.id ?? kind));
      publish();
    };
    unlisteners.push(listen(`${prefix}:download:complete`, done));
    if (kind !== "vad") unlisteners.push(listen(`${prefix}:download:error`, done));
  }
  const timer = window.setInterval(() => {
    const now = Date.now();
    let changed = false;
    for (const [k, v] of entries) {
      if (v.kind === "vad" && now - v.at > VAD_STALE_MS) {
        entries.delete(k);
        changed = true;
      }
    }
    if (changed) publish();
  }, 5_000);
  void reload();
  return () => {
    window.clearInterval(timer);
    unlisteners.forEach((u) => u.then((fn) => fn()));
    entries.clear();
    publish();
  };
}

function subscribe(l: () => void) {
  listeners.add(l);
  if (started++ === 0) stop = begin();
  return () => {
    listeners.delete(l);
    if (--started === 0) {
      stop?.();
      stop = null;
    }
  };
}

/** The downloads running now, in the order they started. */
export function useActiveDownloads(): ActiveDownload[] {
  return useSyncExternalStore(subscribe, () => snapshot);
}

/** One reading of the running downloads, for a card that has just opened. */
export async function activeDownloadsOnce(kind: Kind): Promise<ActiveDownload[]> {
  try {
    return (await api.listActiveDownloads()).filter((d) => d.kind === kind);
  } catch (e) {
    console.error(e);
    return [];
  }
}

/** Cancels a download through the command of its kind. Voice detection has no
 *  cancel command (it is under 1 MB). */
export async function cancelActiveDownload(d: ActiveDownload): Promise<void> {
  if (d.kind === "whisper") await api.cancelDownloadWhisperModel(d.id);
  else if (d.kind === "parakeet") await api.cancelDownloadParakeetModel(d.id);
  else if (d.kind === "llm") await api.cancelDownloadGgufModel(d.id);
}

/** Page that shows a download's card. */
export function downloadView(kind: Kind): "models" | "enhancement" | "audio" {
  return kind === "llm" ? "enhancement" : kind === "vad" ? "audio" : "models";
}
