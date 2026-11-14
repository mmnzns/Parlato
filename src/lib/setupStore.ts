// Parlato: the one shared copy of the setup status (see lib/setupStatus.ts and
// hooks/useSetupStatus.ts). Loads are coalesced: asking for a refresh while one
// is running makes exactly one more run after it, so the answer is never older
// than the last request.

import type { TFunction } from "i18next";
import { listen } from "@tauri-apps/api/event";
import i18n from "@/i18n";
import { loadSetupStatus, SETUP_CHANGED_EVENT, type SetupStatus } from "@/lib/setupStatus";

let snapshot: SetupStatus | null = null;
const listeners = new Set<() => void>();
let timer: ReturnType<typeof setTimeout> | null = null;
let running = false;
let again = false;
let subscribers = 0;
let teardown: (() => void) | null = null;

function run() {
  timer = null;
  if (running) {
    again = true;
    return;
  }
  running = true;
  loadSetupStatus(i18n.t.bind(i18n) as unknown as TFunction)
    .then((s) => {
      snapshot = s;
      listeners.forEach((l) => l());
    })
    .catch(console.error)
    .finally(() => {
      running = false;
      if (again) {
        again = false;
        run();
      }
    });
}

/** Asks for a fresh reading. Several asks in the same moment make one load. */
export function refreshSetupStatus() {
  if (timer === null) timer = setTimeout(run, 0);
}

export function getSetupSnapshot(): SetupStatus | null {
  return snapshot;
}

function start() {
  const bump = () => refreshSetupStatus();
  window.addEventListener("focus", bump);
  window.addEventListener(SETUP_CHANGED_EVENT, bump);
  i18n.on("languageChanged", bump);
  const unlisteners = [
    listen("source:changed", bump),
    listen("model:download:complete", bump),
    listen("parakeet_model:download:complete", bump),
  ];
  refreshSetupStatus();
  return () => {
    window.removeEventListener("focus", bump);
    window.removeEventListener(SETUP_CHANGED_EVENT, bump);
    i18n.off("languageChanged", bump);
    unlisteners.forEach((u) => u.then((fn) => fn()));
  };
}

export function subscribeSetup(l: () => void): () => void {
  listeners.add(l);
  if (subscribers++ === 0) teardown = start();
  return () => {
    listeners.delete(l);
    if (--subscribers === 0) {
      teardown?.();
      teardown = null;
    }
  };
}
