// Parlato: "Transcribe a file" jobs, kept at module level so the list and
// its progress survive navigating away from the page while a long file is
// still being transcribed.
//
// One file at a time; Parlato queues the others ("queued") and starts the
// next one when a file ends. Progress comes from the pipeline:state events tagged
// job "file" (a dictation made meanwhile sends "dictation" events, which
// this list ignores).

import { useSyncExternalStore } from "react";
import { listen } from "@tauri-apps/api/event";
import { api } from "@/lib/tauri";

export type FileJobStatus = "queued" | "converting" | "transcribing" | "cleaning" | "done" | "failed";

/** Extensions symphonia can decode (see Cargo.toml). Opus/webm is not supported. */
export const FILE_EXTENSIONS = ["mp3", "wav", "m4a", "aac", "flac", "ogg", "oga", "aif", "aiff", "caf", "mp4", "m4v", "mov", "mkv"];

export type FileJob = {
  id: string;
  name: string;
  status: FileJobStatus;
  durationSec?: number;
  text?: string;
  error?: string;
};

type PipelineEvent = {
  state: "transcribing" | "enhancing" | "pasting" | "done" | "failed";
  message: string | null;
  text: string | null;
  job: "dictation" | "file" | "retry";
};

type Snapshot = { jobs: FileJob[]; busy: boolean };

let snapshot: Snapshot = { jobs: [], busy: false };
let activeId: string | null = null;
let listening = false;
/** Paths waiting their turn, by job id. */
const queuedPaths = new Map<string, string>();
const subscribers = new Set<() => void>();

function publish(jobs: FileJob[]) {
  snapshot = { jobs, busy: activeId !== null };
  subscribers.forEach((fn) => fn());
}

function patch(id: string, p: Partial<FileJob>) {
  publish(snapshot.jobs.map((j) => (j.id === id ? { ...j, ...p } : j)));
}

function finish(id: string, p: Partial<FileJob>) {
  activeId = null;
  patch(id, p);
  runNext();
}

/** Starts the oldest queued file, if nothing is running. */
function runNext() {
  if (activeId) return;
  const next = [...snapshot.jobs].reverse().find((j) => j.status === "queued");
  if (!next) return;
  const path = queuedPaths.get(next.id);
  queuedPaths.delete(next.id);
  if (path) void run(next.id, path);
}

function ensureListener() {
  if (listening) return;
  listening = true;
  listen<PipelineEvent>("pipeline:state", (e) => {
    const id = activeId;
    const p = e.payload;
    if (!id || p.job !== "file") return;
    if (p.state === "transcribing") patch(id, { status: "transcribing" });
    else if (p.state === "enhancing") patch(id, { status: "cleaning" });
    else if (p.state === "done") finish(id, { status: "done", text: p.text ?? "" });
    else if (p.state === "failed") finish(id, { status: "failed", error: p.message ?? "" });
  });
}

/** Adds files to the list. Unsupported ones fail at once; the others
 *  run one after another. */
export function startFileJobs(paths: string[]) {
  ensureListener();
  const added: FileJob[] = paths.map((path) => {
    const id = crypto.randomUUID();
    const name = path.split(/[\\/]/).pop() ?? path;
    const ext = name.includes(".") ? (name.split(".").pop() ?? "").toLowerCase() : "";
    if (!FILE_EXTENSIONS.includes(ext)) {
      return { id, name, status: "failed", error: "PARLA_ERR:fileUnsupported" };
    }
    queuedPaths.set(id, path);
    return { id, name, status: "queued" };
  });
  publish([...added.reverse(), ...snapshot.jobs]);
  runNext();
}

async function run(id: string, path: string) {
  activeId = id;
  patch(id, { status: "converting" });
  try {
    const res = await api.transcribeFile(path);
    patch(id, { durationSec: res.duration_sec });
  } catch (e) {
    finish(id, { status: "failed", error: String(e) });
  }
}

export function removeFileJob(id: string) {
  if (id === activeId) return;
  queuedPaths.delete(id);
  publish(snapshot.jobs.filter((j) => j.id !== id));
}

export function useFileJobs(): Snapshot {
  return useSyncExternalStore(
    (fn) => {
      subscribers.add(fn);
      return () => subscribers.delete(fn);
    },
    () => snapshot,
  );
}
