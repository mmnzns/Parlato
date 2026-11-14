# UI preview (dev only, mock data)

Runs the real Parlato React screens in a normal browser with a **mocked Tauri
backend**, so every screen can be clicked through without the Tauri app. No
real settings, models, Keychain, microphone or history are ever touched.

**Everything shown is mock data. Never wire this to real data.** It is not part
of the app build (`tsconfig.json` only includes `src/`, and the Vite build only
uses the root `index.html`).

## Run

```
npm run ui-preview      # http://localhost:1430 (strict port)
```

## Query switches

| URL | Effect |
| --- | --- |
| `/?onboarding=1` | First-run state: shows the Onboarding flow |
| `/?perm=missing` | Microphone and Accessibility not granted |
| `/?lang=fr` or `/?lang=es` | UI language (`en` also works) |
| `/#recorder` | The floating recording pill (click-through: the pill's record button runs a fake dictation) |
| `/#recorder-popover` | The pill's Mode popover |

Switches combine, e.g. `/?onboarding=1&lang=es`. Changing the hash reloads the page.

## How it works

- `main.tsx` installs `mockWindows(label)` and `mockIPC(handler, { shouldMockEvents: true })`
  before dynamically importing `src/main.tsx`. The label comes from the URL hash
  (the real app picks its view from the Tauri window label).
- `mocks.ts` has a handler for every command in `src/lib/tauri.ts` plus the
  store, updater, dialog, opener, autostart, process, app and window plugin commands.
  State is in memory and resets on reload. Unknown commands log
  `console.warn("[ui-preview] unmocked command", ...)` and return `null`.
- Model downloads emit progress and completion events; recording and file
  transcription emit `recording:*` / `pipeline:state` events.
- Platform is whatever your browser reports: use a Mac browser to get the macOS strings.
