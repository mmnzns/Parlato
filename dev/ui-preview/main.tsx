// DEV ONLY. Boots the real Parlato UI against a mocked Tauri backend.
// The app is imported dynamically AFTER the mocks are installed, because
// src/main.tsx reads the window label and the app calls invoke() on mount.

import { mockIPC, mockWindows } from "@tauri-apps/api/mocks";
import { handle } from "./mocks";

const params = new URLSearchParams(window.location.search);

// ?lang=fr|es : i18n reads this key from localStorage (see src/i18n/index.ts).
const lang = params.get("lang");
if (lang === "fr" || lang === "es" || lang === "en") {
  try {
    localStorage.setItem("parla_language", lang);
  } catch {
    // storage blocked: the browser language is used
  }
}

// The real app picks its screen from the Tauri window label (src/main.tsx).
// In a browser the label comes from the URL hash:
//   #recorder         -> the floating pill
//   #recorder-popover -> the pill's Mode popover
//   (none)            -> the main window
const hash = window.location.hash.replace(/^#/, "");
const label = hash === "recorder" || hash === "recorder-popover" ? hash : "main";
window.addEventListener("hashchange", () => window.location.reload());

mockWindows(label);
mockIPC((cmd, payload) => handle(cmd, payload), { shouldMockEvents: true });

await import("../../src/main");

if (label !== "main") {
  // The real pill windows are transparent over the desktop; show a neutral
  // backdrop here so the (dark) pill is visible.
  document.documentElement.style.background = "linear-gradient(135deg,#d9dde6,#b4bccb)";
  document.body.style.background = "transparent";
  document.body.style.minHeight = "100vh";
}
