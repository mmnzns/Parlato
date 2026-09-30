// Parlato: "Transcribe a file" jobs, kept at module level so the list and
// its progress survive navigating away from the page while a long file is
// still being transcribed.
//
// One file at a time: the backend pipeline tracks a single in-flight
// history row, like a dictation. Progress comes from the same
// pipeline:state events the recorder pill uses.

import { useSyncExternalStore } from "react";
import { listen } from "@tauri-apps/api/event";
import { api } from "@/lib/tauri";

export type FileJobStatus = "converting" | "transcribing" | "cleaning" | "done" | "failed";

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
};

type Snapshot = { jobs: FileJob[]; busy: boolean };

let snapshot: Snapshot = { jobs: [], busy: false };
let activeId: string | null = null;
let listening = false;
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
}

function ensureListener() {
  if (listening) return;
  listening = true;
  listen<PipelineEvent>("pipeline:state", (e) => {
    const id = activeId;
    if (!id) return;
    const p = e.payload;
    if (p.state === "transcribing") patch(id, { status: "transcribing" });
    else if (p.state === "enhancing") patch(id, { status: "cleaning" });
    else if (p.state === "done") finish(id, { status: "done", text: p.text ?? "" });
    else if (p.state === "failed") finish(id, { status: "failed", error: p.message ?? "" });
  });
}

export async function startFileJob(path: string) {
  if (activeId) return;
  ensureListener();
  const id = crypto.randomUUID();
  const name = path.split(/[\\/]/).pop() ?? path;
  activeId = id;
  publish([{ id, name, status: "converting" }, ...snapshot.jobs]);
  try {
    const res = await api.transcribeFile(path);
    patch(id, { durationSec: res.duration_sec });
  } catch (e) {
    finish(id, { status: "failed", error: String(e) });
  }
}

export function removeFileJob(id: string) {
  if (id === activeId) return;
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
