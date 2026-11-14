import React from "react";
import ReactDOM from "react-dom/client";
import { getCurrentWindow } from "@tauri-apps/api/window";
import App from "./App";
import { MiniRecorderView } from "./components/MiniRecorderView";
import { RecorderPopoverView } from "./components/RecorderPopoverView";
import { ReviewView } from "./components/ReviewView";
import "./i18n";
// Parlato fonts, bundled locally so the app works offline.
import "@fontsource-variable/bricolage-grotesque";
import "@fontsource/ibm-plex-mono/400.css";
import "@fontsource/ibm-plex-mono/500.css";
import "@fontsource/ibm-plex-mono/600.css";
import "./index.css";
import { initTheme } from "./lib/theme";

// Selection de la vue en fonction du label de fenetre :
// - "main" : app principale
// - "recorder" : mini-recorder flottant
// - "recorder-popover" : popover du bouton Mode de la bulle
// - "recorder-review" : editeur "Review before pasting"
const label = getCurrentWindow().label;
const isRecorder = label === "recorder";
const isPopover = label === "recorder-popover";
const isReview = label === "recorder-review";
const Root = isRecorder ? MiniRecorderView : isPopover ? RecorderPopoverView : isReview ? ReviewView : App;

// Les fenetres bulle et popover sont transparentes au niveau Tauri
// (decorations off, transparent: true). On force le body / #root en
// transparent pour eviter que le bg blanc tailwind par defaut colle un
// carre blanc derriere la pill.
if (isRecorder || isPopover || isReview) {
  document.documentElement.style.background = "transparent";
  document.body.style.background = "transparent";
}
if (isRecorder || isReview) {
  document.body.style.colorScheme = "dark";
}

// Parlato: the main window follows the computer's light/dark theme,
// unless overridden in Settings > Appearance.
if (!isRecorder && !isPopover && !isReview) {
  initTheme();
}

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    <Root />
  </React.StrictMode>,
);
