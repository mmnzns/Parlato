// Parlato: light/dark appearance for the main window.
// "system" follows the Windows app theme; "light"/"dark" override it.
// Stored per machine in localStorage (a UI preference, not app data).

export type ThemePref = "system" | "light" | "dark";

const KEY = "parlato_theme";
const darkQuery = window.matchMedia("(prefers-color-scheme: dark)");

export function getThemePref(): ThemePref {
  try {
    const v = localStorage.getItem(KEY);
    return v === "light" || v === "dark" ? v : "system";
  } catch {
    return "system";
  }
}

function apply() {
  const pref = getThemePref();
  const dark = pref === "dark" || (pref === "system" && darkQuery.matches);
  document.documentElement.classList.toggle("dark", dark);
}

export function setThemePref(pref: ThemePref) {
  try {
    if (pref === "system") localStorage.removeItem(KEY);
    else localStorage.setItem(KEY, pref);
  } catch {
    // Storage unavailable: the choice still applies for this session.
  }
  apply();
}

export function initTheme() {
  apply();
  darkQuery.addEventListener("change", apply);
}
