# CLAUDE.md

Guidance for Claude Code when working in this repository.

## What this is

Parla is a native Windows voice-to-text app: it transcribes speech and pastes the
result at the cursor. A Mac build is in progress (see "Mac port plan"). It is a fork of
[LitteRabbit-37/Parla](https://github.com/LitteRabbit-37/Parla), itself a Windows
re-implementation of [VoiceInk](https://github.com/Beingpax/VoiceInk) (macOS/Swift).
Licensed GPL-3.0.

This fork is a learning project as much as a product. Changes should be small,
reviewable, and explainable. Prefer the boring, pattern-following change over the
clever one.

## Stack

- **Shell:** Tauri v2 (Rust backend, WebView2 frontend)
- **Frontend:** React 19, TypeScript, Vite, Tailwind v4, shadcn/ui
- **Transcription:** whisper.cpp (`whisper-rs`), Parakeet/ONNX (`parakeet-rs`)
- **LLM enhancement:** llama.cpp (`llama-cpp-2`), Ollama, plus cloud providers
- **Audio:** WASAPI via `cpal`, resampling via `rubato`
- **Storage:** SQLite (history), Tauri store (`parla.settings.json`) for settings,
  Windows Credential Manager for API keys — never write keys to disk

## Layout

```
src/                      React frontend
  components/             Feature panels (one per settings screen)
  components/ui/          shadcn primitives
  hooks/, lib/            Shared hooks and the Tauri IPC bindings
  i18n/locales/           en.json, es.json, fr.json
  index.css               ALL global design tokens
src-tauri/src/            Rust backend
  commands/               Tauri IPC command handlers (the frontend's entry points)
  audio/                  WASAPI capture
  transcription/          Whisper / Parakeet / cloud engines
  enhancement/            LLM orchestration
    providers/            One file per provider, each impl LLMProvider
  hotkeys/, paste/        Global shortcuts, clipboard
  power_mode/             Foreground-window profile switching
  screen_context/         Window capture + OCR
  history/, db/           SQLite history
```

Frontend and backend communicate only through Tauri IPC commands in
`src-tauri/src/commands/`. That is the boundary — respect it.

## Commands

```bash
npm install                       # install frontend deps
npm run tauri dev                 # dev build + run (first build: 20-40 min)
npm run tauri build               # release build
npx tsc --noEmit                  # frontend type-check
cd src-tauri && cargo test --lib  # Rust tests
cd src-tauri && cargo fmt         # format
cd src-tauri && cargo clippy --lib
```

Run the type-check and Rust tests before committing anything non-trivial.

## Local environment notes

The upstream `BUILDING.md` is incomplete. This machine additionally needed:

- **CMake** — `whisper-rs`, `llama-cpp-2` and `parakeet-rs` compile C++ via the
  `cmake` crate. Not listed upstream.
- **LLVM / libclang** — `whisper-rs-sys` runs `bindgen`, which needs `libclang.dll`.
  Not listed upstream. `LIBCLANG_PATH` is set to `C:\Program Files\LLVM\bin`.
  Without it the build dies at `whisper-rs-sys` with "Unable to find libclang".

Also note: VS Build Tools here are **2026 (v18)**, not the 2022 (v17) upstream
specifies. The CPU build works. CUDA is a separate matter — see Guardrails.

## Conventions (from CONTRIBUTING.md — follow these)

- **No emojis** in code, commit messages, or documentation.
- Commit messages: imperative mood, English, focused.
- Rust: run `cargo fmt` and `cargo clippy --lib`. Keep the module-header comments
  that cite the corresponding VoiceInk reference file — they are the project's
  map back to its behavioural source.
- TypeScript: default Vite/TSX conventions; Tailwind classes grouped by concern
  (layout, spacing, colors).
- Existing Rust comments are largely in French (upstream author's language).
  Match the surrounding language when editing a file rather than converting it.

## Things that are easy to get wrong

**Model names and descriptions shown in the UI come from the locale files.**
The Rust catalogs keep upstream's French `display_name` / `notes`; the UI
shows `modelText.<id>.name` / `.notes` instead (`src/lib/modelText.ts`, id with
non-alphanumerics replaced by `_`), falling back to the catalog text. When
porting a new model from upstream, add its en/fr/es `modelText` entry too.

**Do not bump `llama-cpp-2` on its own.** `whisper-rs-sys` and
`llama-cpp-sys-2` each bundle their own copy of ggml, linked together with
`/FORCE:MULTIPLE` (`src-tauri/.cargo/config.toml`), so only one copy of each
symbol survives. Tested 2026-09-29: llama-cpp-2 0.1.157 makes every GGUF abort
in ggml (`GGML_ASSERT ... FLASH_ATTN_EXT`); 0.1.143 works. Newer LLM families
(Qwen 3.5, Gemma 4) need a newer llama.cpp and so wait on a matching
whisper.cpp / ggml. Always run the ignored smoke tests
(`llamacpp_smoke`, `parakeet_smoke`) against real model files after touching
an inference crate. Parakeet uses ONNX Runtime and is not affected.

**Every downloadable model is credited in `THIRD_PARTY_NOTICES.md`.** Adding a
model means adding its author, licence and any required notice there (and the
licence text under `licenses/` if the licence asks for a copy). Parlato never
bundles or hosts model files; they download from the original host. Pin
third-party repositories to a commit (`revision` in the Parakeet catalog).

**i18n is a three-file change.** `src/i18n/locales/` holds en/es/fr, currently
1163 keys each (including the `macOverrides` block) and exactly in sync. Any user-facing string means editing all three.
Adding a key to only `en.json` is a silent bug in two languages. French and Spanish
are written without accents throughout (upstream convention); match it unless the
whole set is converted at once.

**Vite on Windows sometimes misses a change to a locale JSON file** and keeps
serving the stale module (new keys render as raw `section.key` text). `touch` the
file to force a reload.

**Design tokens live in exactly one file.** `src/index.css` (92 lines) defines every
color, radius and the font stack, currently the stock shadcn defaults — fully
achromatic (`oklch(x 0 0)`, zero chroma) in both light and dark. This is the
highest-leverage file for visual identity: change the tokens before touching
component layout.

**LLM model lists live in Rust, not the frontend.** Each provider in
`src-tauri/src/enhancement/providers/` declares a `MODELS` const plus
`default_model()`. Updating a model list is not a one-line array edit — check
`reasoning_for()` and `temperature_for()` in `src-tauri/src/enhancement/service.rs`,
which map reasoning effort by `(provider_id, model)` and temperature by model
prefix. A new model added to `MODELS` without a matching `reasoning_for` arm
silently falls through to the default config.

**The "Custom OpenAI-compat" provider takes a free-text model ID** (frontend input
in `EnhancementPanel.tsx`; `CustomProvider::default_models()` is empty by design).
It is the zero-code way to test a model ID. Caveat: `reasoning_for()` keys on
provider `"custom"`, which matches no arm, so reasoning-effort tuning does not
apply on that path.

**The fork is branded "Parlato".** Upstream's name is Parla; the rename was done
with the upstream author's permission. User-visible identity lives in:
`tauri.conf.json` (productName, identifier `com.craftconceptsdigital.parlato`,
window title, publisher, updater endpoint), `index.html`, `tray.rs` (tooltip and
menu labels), `mini_recorder.rs` (window titles), `Sidebar.tsx`/`Onboarding.tsx`,
the i18n files, `services/api_keys.rs` (Credential Manager service name) and
`installer-hooks.nsh` (uninstall data paths and autostart key). When porting
from upstream, new user-facing strings will say "Parla": rename them.

Deliberately NOT renamed (internal, invisible to users, renaming only adds merge
conflicts): the Rust crate `parla` / `parla_lib` (dev exe is `parla.exe`), store
files `parla.*.json`, thread names, log filters, and code comments.

**Never change the identifier again once the app is distributed.** It determines
the data folders, the single-instance lock and installer upgrades; changing it
strands every user's settings and API keys.

**The auto-updater uses Parlato's own signing key** (generated 2026-09-29).
The public key is `plugins.updater.pubkey` in `tauri.conf.json`. The private
key lives only at `C:\Users\mikey\.tauri\parlato-updater.key` (no password),
backed up by the owner, and as the `TAURI_SIGNING_PRIVATE_KEY` repository
secret. Never commit it, print it or copy it into the repo. Never replace the
public key: every installed copy only accepts updates signed with the
matching private key, so a new key strands all existing installs. Losing the
private key has the same effect.

**Releasing.** Bump the version in `package.json`, `package-lock.json`,
`src-tauri/Cargo.toml` and `src-tauri/tauri.conf.json`, add a `CHANGELOG.md`
entry, commit, then push a `vX.Y.Z` tag. `.github/workflows/release.yml`
builds x64 and ARM64, signs them, uploads `latest.json` and creates a
**draft** release. Publishing the draft is the owner's call. Local
`npm run tauri build` keeps `createUpdaterArtifacts` off, so it needs no key.
Releases are not code-signed (owner decision, no paid certificate).

## Mac port plan (started 2026-09-30)

Goal: a free Mac build of Parlato for the owner's friends and family, installed
from GitHub Releases. Not a public product launch. The Windows build stays the
main product and must never regress because of Mac work.

### Decisions (owner's, do not relitigate)

- **No Apple Developer membership, no notarization.** Users accept a one-time
  Gatekeeper warning. The README explains how to get past it.
- **No Homebrew distribution.** The only channel is the `.dmg` on GitHub
  Releases. (Homebrew on the developer's own machine for CMake etc. is fine.)
- **Same app identifier** `com.craftconceptsdigital.parlato`, same updater
  signing key, same `latest.json`. The Mac build is another platform in the
  same release, not a separate app.
- **Apple Silicon (arm64) only.** No Intel Mac build (owner decision,
  2026-09-30: too small a group).

### How Mac code is added

- Mac code goes in new macOS-only files next to the Windows ones (for example
  `hotkeys/keyboard_hook_macos.rs`), gated with `#[cfg(target_os = "macos")]`.
  Shared files are only touched to add the `cfg` dispatch to the new file.
- The existing `#[cfg(not(windows))]` stubs are where the dispatch goes. Do not
  rewrite or reformat the `#[cfg(windows)]` code paths.
- Keep the module-header comment convention: each Mac file cites the VoiceInk
  Swift file it follows. VoiceInk (GPL-3.0, macOS-native) is the reference
  implementation for every Mac API below.
- Every Mac change must still pass the Windows checks: the Windows release
  workflow builds, `cargo test --lib` passes on Windows.
- **User-facing wording is platform-neutral** (owner decision, 2026-09-30):
  "Same as computer", "this computer", "your computer's settings", never
  "Windows" or "PC", so one string works on both platforms. French uses
  "l'ordinateur" / "cet ordinateur", Spanish "el ordenador" / "este
  ordenador". Done for the general UI and the tray. Still Windows-specific,
  on purpose: the step-by-step permission and OCR instructions (Settings
  paths, language packs, the keyboard hook tip). Phase 1 gives those a
  per-platform version, because the Mac steps are different, not just
  differently named. Internal key names like `voice.sameAsWindows` stay.

### Unsigned app on macOS: the two user-facing costs

1. **First launch is blocked by Gatekeeper.** The user opens the app, gets
   blocked, then goes to System Settings > Privacy & Security > Open Anyway
   (admin password). Terminal fallback:
   `xattr -dr com.apple.quarantine /Applications/Parlato.app`. Right-click >
   Open no longer bypasses it on current macOS. Updates installed by the
   in-app updater are not quarantined, so this happens once.
2. **Permissions are tied to the code signature.** Accessibility, Input
   Monitoring, Screen Recording and Keychain access are granted to a signing
   identity. With the default ad-hoc signature every build is a new identity,
   so after each update the hotkey and paste silently stop working and the
   Keychain asks again. **Fix: sign every Mac build with one self-signed
   code-signing certificate** (free, created once in Keychain Access). It
   gives a stable identity so permissions survive updates. It does not remove
   the Gatekeeper warning. Treat its `.p12` like the updater key: never
   commit it, store it only as repository secrets (`APPLE_CERTIFICATE`,
   `APPLE_CERTIFICATE_PASSWORD`, `APPLE_SIGNING_IDENTITY`, which Tauri reads).
   Losing it means every Mac user re-grants permissions once, which is
   recoverable, unlike losing the updater key.

### Phases

**Phase 0: build it on the Mac as-is.** No features. Mac prerequisites: Xcode
Command Line Tools, Rust, Node, CMake (libclang ships with the Xcode tools).
Expected issues to solve and record under "Local environment notes":
- `.cargo/config.toml` only handles the duplicate ggml symbols
  (`/FORCE:MULTIPLE`) for MSVC. Check whether Apple's linker rejects the same
  duplicates between `whisper-rs-sys` and `llama-cpp-sys-2`.
- `keyring` only enables `windows-native`. Move it to a Windows target block
  and add `apple-native` under `[target.'cfg(target_os = "macos")'.dependencies]`,
  otherwise API keys are not stored in the Keychain.
- The default `gpu-detect` feature (NVIDIA `nvml-wrapper`) is meaningless on
  Mac; build with it off.
- `tauri.conf.json` bundles `nsis` only. Add a `tauri.macos.conf.json`
  override (Tauri merges it on macOS) with `app` + `dmg` targets and
  `macOSPrivateApi: true` if the recorder pill needs transparency.
Done when: `npm run tauri dev` opens the Workbench UI on the Mac, and
transcribing a file (Transcribe a file screen) works with a local model.

Phase 0 findings (2026-09-30, MacBook, Apple Silicon, macOS 27, Rust 1.98):
- Mac working copy: `~/Developer/Parlato`. Prerequisites installed: Rust
  via rustup, CMake via Homebrew. Xcode Command Line Tools and Node were
  already there; libclang came with the Xcode tools, no `LIBCLANG_PATH`
  needed. Full debug build: about 30 seconds after the first one.
- `uiautomation` was an all-platform dependency and pulled the `windows`
  crates into the Mac build. It now sits in the `cfg(windows)` block, as
  does `keyring`'s `windows-native` feature; macOS gets `apple-native`.
- Duplicate ggml symbols: Apple's linker only warns (`ld: duplicate
  symbol`) and keeps the first copy, same outcome as `/FORCE:MULTIPLE` on
  Windows. No config needed. The warnings are expected.
- `gpu-detect` compiles on macOS; NVML just fails at runtime and the app
  runs CPU-only. It logs a long harmless `nvml.dll` debug line.
- Placeholders that compile but do nothing yet: `active_window_macos.rs`,
  `installed_apps_macos.rs`, `ocr_macos.rs` (swapped in by `#[cfg_attr(...,
  path = ...)]` in each `mod.rs`), the non-Windows `try_extract` in
  `browser_url.rs`, and the OCR check in `commands/permissions.rs`. The
  keyboard hook, paste and mute already had non-Windows stubs.
- The recorder pill needs `macOSPrivateApi: true` (in `tauri.conf.json`,
  ignored on Windows) and the `macos-private-api` Tauri feature for
  `.transparent(true)` to compile.
- `cargo test --lib`: 126 passed on macOS.

**Phase 1: dictation works (the point where the owner's girlfriend can use it).**
- `Info.plist` usage strings: `NSMicrophoneUsageDescription` (without it the
  app crashes on first recording), `NSAppleEventsUsageDescription`.
- Hotkey (`hotkeys/`): a CGEventTap, needs Input Monitoring. The Tauri
  global-shortcut plugin cannot do modifier-only keys, so it is not enough.
  Most Mac keyboards have no Right Ctrl: default to Right Option. Fn works too
  but macOS may bind it to emoji or dictation (System Settings > Keyboard >
  "Press globe key to" > Do Nothing); say so in the UI if Fn is chosen.
- Paste (`paste/`): post Cmd+V with CGEvent, needs Accessibility. Keep the
  clipboard backup/restore behaviour.
- Permissions (`commands/permissions.rs`, Onboarding, Settings): real checks
  for Microphone, Accessibility and Input Monitoring, each with a button that
  opens the right System Settings pane. VoiceInk's permission screen is the
  reference.
Done when: hold the key, speak, release, and the text lands in Notes, Safari
and Messages, after a full quit and relaunch.

Phase 1 status (2026-09-30, after a full review):
- Hotkey: `hotkeys/keyboard_hook_macos.rs`, a CGEventTap that maps macOS key
  codes to Windows VK codes and feeds the shared `handle_key`. A character
  typed while a modifier-only trigger is held (Option+e) cancels that press.
  Stale modifiers are re-synced from each event's flags. Events Parlato
  posts itself carry `PARLATO_EVENT_TAG` and are ignored by its own tap.
  The Accessibility prompt waits for onboarding (`allow_accessibility_prompt`).
- Paste: `paste/macos.rs` (Cmd+V, re-activates the app that was frontmost
  at record start), `paste/clipboard_backup_macos.rs` (every pasteboard
  type). If the paste fails, the dictated text stays on the clipboard.
- Permissions: `commands/permissions_macos.rs` (AVCaptureDevice mic status,
  AXIsProcessTrusted, System Settings panes). Onboarding has a first
  "Permissions" step on Mac.
- The shortcut recorder derives VKs from `e.code` on Mac (physical keys, like
  the tap) and refuses Command-only combos.
- Mac-only UI: `src/lib/platform.ts` `isMac`; `macOverrides` in the locale
  files for Mac key names and Mac instructions. Hidden on Mac until they
  work: pause other audio, screen context, command-line AI engine, pairing
  power modes with apps/websites. "Use everywhere else" works (frontmost app
  via NSWorkspace).
- Menu bar: template icon `icons/tray-template.png` (generated from the logo,
  bright shapes only), left click opens the menu, Dock click reopens the
  window (`RunEvent::Reopen`). Pill: work area of the screen under the
  mouse, on every Space.
- Test as a bundle: `npm run tauri build -- --debug --bundles app,dmg`.
  `tauri.macos.conf.json` + `Entitlements.plist` (audio input) are ready for
  Phase 2 signing.
- Logs (both platforms): `logging.rs`, parlato.log in
  `~/Library/Logs/com.craftconceptsdigital.parlato/` or
  `%LOCALAPPDATA%\com.craftconceptsdigital.parlato\logs\`; Settings > Log file
  opens the folder. Logs hold lengths and timings, never dictated text.
- Not done yet: Metal (Phase 3, whisper and llama together), non-activating
  NSPanel pill above full-screen apps, Fn/Globe key, OCR and app pairing
  (Phase 4). On non-QWERTY Macs a custom letter combo fires correctly but its
  label shows the US key name.

**Phase 2: shareable release.**
- Self-signed certificate created, secrets added by the owner in the GitHub
  web UI (Claude never handles the `.p12` or its password).
- `release.yml`: add a `macos-latest` job (arm64) that builds the `.dmg`,
  signs with the self-signed identity, signs the updater artifact, and adds
  `darwin-aarch64` to the same `latest.json` and draft release.
- README: "Install on Mac" section with the Open Anyway steps (with
  screenshots), the Terminal fallback, and the first-run permission checklist.
- Test the update path Mac to Mac, the same way the Windows updater was
  tested: an older-numbered build must update and keep its permissions.

Phase 2 status (2026-09-30):
- Certificate: "Parlato Code Signing", self-signed root, Code Signing
  extended key usage, valid to 2036-09-27, in the owner's login keychain on
  the MacBook. Created by the owner in Keychain Access (Certificate
  Assistant > Create a Certificate, override defaults, 3650 days).
- Verified locally: signing works although macOS calls the certificate
  untrusted. Designated requirement is `identifier
  "com.craftconceptsdigital.parlato" and certificate root = H"3ebc6a96..."`,
  so a rebuilt and re-signed app keeps its Accessibility permission
  (tested: new CDHash, tap installed with no re-grant). Hardened runtime on,
  entitlements embedded.
- Local signed build: `APPLE_SIGNING_IDENTITY="Parlato Code Signing" npm run
  tauri build -- --debug --bundles app`. The first signing asks the owner to
  let codesign use the key (Always Allow).
- `release.yml` has a `macos` job (after the Windows `cpu` jobs): imports
  the .p12 into a temporary keychain, builds, signs, verifies with
  `codesign --verify`, adds the .dmg and `darwin-aarch64` updater entry to
  the same draft release. Needs repository secrets APPLE_CERTIFICATE,
  APPLE_CERTIFICATE_PASSWORD, APPLE_SIGNING_IDENTITY (added by the owner).
- README + docs/INSTALL.md wait on branch `docs/mac-install` until the first
  release with a .dmg is published; merge it then.

**Phase 3: polish.**
- Mute other audio while recording (`audio/mute.rs`): Core Audio default
  output device.
- Recorder pill visible above full-screen apps and on every Space.
- Menu bar icon (template image) and a "Hide Dock icon" option (the tray
  code already mentions it as macOS-only).
- Metal acceleration for whisper.cpp and llama.cpp. Rerun the ignored
  `llamacpp_smoke` / `parakeet_smoke` tests afterwards.

**Phase 4: later, only if wanted.**
- Power modes: frontmost app via NSWorkspace (bundle IDs, not exe names, so
  the matcher and "Add app" picker need Mac-aware matching), installed apps
  from `/Applications`, browser URL via AppleScript (Automation permission).
- Screen context: `xcap` capture (Screen Recording permission) plus Apple
  Vision OCR instead of `Media.Ocr`.
- `window_subclass.rs` has no Mac equivalent; leave it Windows-only.

## Guardrails for this fork

- **Do not touch native Windows systems code for now**: WASAPI audio capture
  (`audio/`), keyboard hooks (`hotkeys/`), OCR / UI Automation (`screen_context/`),
  and `window_subclass.rs`. These are the hardest to debug and are off-limits until
  more reps are built on lower-risk changes. The Mac port adds new macOS-only
  files in these folders (see "Mac port plan"); that is allowed, editing the
  Windows code paths is not.
- **CPU-only.** Do not add CUDA features to the build. The GPU here is Blackwell
  (needs CUDA 12.8+) and the VS version is v18 while CUDA integrates with v17 —
  two independent failure points. CUDA is its own future task.
- **Preserve GPL-3.0.** Keep the LICENSE and all copyright/license headers. Note
  modifications if anything is ever distributed.
- **Commit only what builds and works.** One logical change per commit.
- `upstream` remote points at LitteRabbit-37/Parla; `origin` is this fork.
  **We port from upstream, we do not merge it.** Review upstream's new commits
  periodically and bring over technical changes (bug fixes, engine / provider /
  model catalog updates, security, performance) one by one, by hand or with a
  cherry-pick. Never let upstream overwrite what this fork owns: the Workbench
  UI (components, layout, design tokens), all user-facing wording in the
  locale files, the Parlato branding, and the `modelText.*` model names and
  descriptions. If an upstream commit mixes both, take the technical part only.
