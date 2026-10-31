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

**Absolute rule (owner, 2026-10-29): Sonnet does the heavy lifting.** If Sonnet
can handle a piece of development work (writing code, tests, builds, edits,
searches), hand it to a Sonnet subagent. Opus plans, reviews the result before
it is committed, and takes only what Sonnet can't handle. The goal is to save
tokens.

## Stack

- **Shell:** Tauri v2 (Rust backend, WebView2 frontend)
- **Frontend:** React 19, TypeScript, Vite, Tailwind v4, shadcn/ui
- **Transcription:** whisper.cpp and Parakeet/ONNX through the shared engine
  (`voice-engine-stt`, in process; it pulls in `whisper-rs` and `parakeet-rs`,
  which are not direct dependencies of this app)
- **LLM enhancement:** local models through the engine's AI helper (llama.cpp
  runs in the `parlato-llm` sidecar, driven by `voice-engine-client`), Ollama,
  plus cloud providers
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

- **CMake** — `whisper-rs-sys` (through `voice-engine-stt`) compiles C++ via the
  `cmake` crate, and `scripts/fetch-helpers.sh` builds llama.cpp for the helper.
  Not listed upstream.
- **LLVM / libclang** — `whisper-rs-sys` runs `bindgen`, which needs `libclang.dll`.
  Not listed upstream. `LIBCLANG_PATH` is set to `C:\Program Files\LLVM\bin`.
  Without it the build dies at `whisper-rs-sys` with "Unable to find libclang".

Also note: VS Build Tools here are **2026 (v18)**, not the 2022 (v17) upstream
specifies. The build works. CUDA is not used by this app — see Guardrails.

**The local AI helper must exist before the app builds** (engine v0.2.2).
`tauri.conf.json` lists `binaries/parlato-llm` in `bundle.externalBin`, and
`tauri-build` fails if `src-tauri/binaries/parlato-llm-<triple>[.exe]` is
missing. On the Mac run `scripts/fetch-helpers.sh` once (it builds the engine's
helper at the pinned tag; the folder is git-ignored). Releases get the helpers
from the engine's reusable workflow (`release.yml`, job `helpers`); the Windows
x64 job adds `parlato-llm-vulkan` with `src-tauri/tauri.windows-x64.conf.json`.
CI creates empty placeholder files instead.

**Windows release builds of whisper.cpp need `-O2` (fixed 2026-10-15).** With
the Visual Studio generator the `cmake` crate strips every `/O` flag from
`CMAKE_C/CXX_FLAGS_RELEASE`, so `whisper-rs-sys` compiled at `/Od` in release.
`src-tauri/.cargo/config.toml` `[env]` puts `-O2 -DNDEBUG` back (the sys crate
forwards `CMAKE_*` env vars to CMake). To check: `CMAKE_CXX_FLAGS_RELEASE` in
`target/release/build/whisper-rs-sys-*/out/build/CMakeCache.txt`. After
changing it: `cargo clean --release -p whisper-rs-sys`. The block is the
engine's "Build settings an app must copy" and must stay equal to it.
`scripts/cpu-baseline.ps1` fails a release when the cache has no `-O2`.

**Windows releases build ggml for a fixed CPU, not the runner's (fixed
2026-10-16).** whisper.cpp defaults to `GGML_NATIVE=ON`, so `whisper-rs-sys`
compiled for the building machine's CPU (AVX-512 on this Ryzen and on some
GitHub runners), which crashes with "illegal instruction" on most Intel PCs.
`scripts/cpu-baseline.ps1` (whisper.cpp only; the helper's llama.cpp is
checked by the engine's workflow) sets `GGML_NATIVE=OFF` plus x86-64-v3 (AVX2, no
AVX-512) on x64 and plain armv8-a on ARM64; `release.yml` and `ci.yml` call it
and fail if a CMake cache has NATIVE or AVX-512 on. It is Windows only, not
`.cargo/config.toml` (`[env]` can't be scoped to a target and the Mac must stay
native). Local `tauri dev` / `cargo test` still build natively unless you
dot-source the script and call `Set-GgmlBaseline x64` first.

**Debug builds crash when voice detection loads (found 2026-10-28).** A
`tauri dev` / debug `cargo test` build aborts right after
`whisper_vad_init_from_file_with_params` (the app exits with 0xc0000409, the
`vad_threads_smoke` test with exit code 3). The linker warns `LNK4098:
defaultlib 'msvcrt' conflicts`: the C++ sys crates link the release C runtime
while debug Rust links the debug one. Release builds are fine (same test
passes, installed releases load VAD normally). To test dictation by hand,
use a release build (`npm run tauri build`), or turn voice detection off in
the dev build.

## Shared engine (voice-engine)

The local AI helper, the helper protocol, speech model runtimes, the model catalog, the shared model folder and the
downloader come from https://github.com/mmnzns/voice-engine, pinned by tag in src-tauri/Cargo.toml.

- Sonnet does the heavy lifting, no exceptions: hand every piece of development work Sonnet can handle to Sonnet
  subagents. Opus only plans, briefs, reviews and decides.
- Before working on AI, speech or models, read the engine change log on the Notion page "Voice Apps Engine Sync".
- Never edit engine code from this repository. If the engine needs a change, open an issue on mmnzns/voice-engine.
- Move to a new engine tag only on purpose, on a branch, after the tests and a manual check pass. Then update this
  app's column in the change log.
- A local `src-tauri/.cargo/config.toml` [patch] pointing at ~/Developer/voice-engine is for trying changes only.
  Never commit it; releases build against the tag.

Code flows one way only: from the engine into Parlato, never from Parlato (GPL, derived from Parla and VoiceInk)
into the engine, Talaan or Ensayo, which are the owner's own work. Parlato-only plans go on the "Parlato Journey"
page (https://app.notion.com/p/3eb4dc1744e1815ca2d2efa19ab6552f); the engine change log is
https://app.notion.com/p/4074dc1744e18017a92ffe0752e2282d.

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

**Long Parakeet TDT dictations are decoded in windows** (2026-10-22,
`transcription/parakeet_windows.rs`). A single pass over a long recording
made Parakeet drift into English and drop sentences. Over 15 s, the TDT models
(v2, v3, Ultra; not Unified or Nemotron) are cut into 14.96 s windows with a
2 s overlap, starts aligned on silence, and the timed tokens are merged by
matching the overlap, following FluidAudio's `ChunkProcessor` at the commit
VoiceInk pins (762baf67). It is pure functions with unit tests; the module
header lists what is not ported (gap repair, parallel windows). Check it on a
long real recording with the ignored `parakeet_long_audio` test. Checked
2026-10-22 on a 77 s dictation with Parakeet Ultra: 6 windows, nothing lost
or doubled at the seams. That recording did not drift in a single pass
either, and the two texts differ only in a few words (single pass "Parlato",
windows "Parlat"), so it shows no harm, not the fix itself; Florian's case
was a 147 s recording.

**One "Use graphics card" setting, `use_gpu` (engine v0.2.2).** It replaces the 0.14
`parakeet_gpu` setting (commands `get/set_use_gpu`, store key `use_gpu`, migration in
`use_gpu.rs`: `parakeet_gpu == true` becomes `use_gpu = true`, then the old key is removed;
switch in `SpeechThreadsPanel.tsx`). Default: on for the Mac; on Windows on only when
`voice_engine_stt::discrete_card()` finds a discrete card, worked out only while the person
has not chosen. It covers Parakeet TDT (DirectML on Windows), Whisper (Metal, Mac only)
and local AI (Metal on the Mac, the Vulkan helper on Windows x64). ONNX models run on the
processor on the Mac. Every Windows build carries ONNX Runtime with DirectML (through
`voice-engine-stt/directml`) and uses the DirectML.dll of Windows (System32, Windows 10
1903+; not bundled, very old builds fall back to the processor), so there is still one
installer and the auto-updater is untouched. If the graphics card fails (`fell_back`),
Parlato uses the processor, remembers the failure until the app restarts and shows one
recorder-pill notice. The old 2026-10-22 measurements (RTX 5070 Ti, Parakeet Ultra int8,
77 s: processor 1.25 s, GPU encoder 0.52 s) are in the CHANGELOG and the 2026-10-22 spec.

**Whisper and local AI on the graphics card (engine v0.2.2).** On the Mac
Whisper uses Metal and the local AI helper uses Metal. On Windows x64 local AI
uses the Vulkan helper `parlato-llm-vulkan.exe` (next to the app exe, only when
the user's "Use graphics card" setting is on); Whisper stays on the processor
in process (no Vulkan speech helper in this release); Parakeet uses DirectML.
Windows ARM64 has no Vulkan helper. Because llama.cpp lives in the helper, the
Vulkan runtime (`vulkan-1.dll`) is only needed by that helper, never by the app
itself. CUDA stays out of releases.

**Model names and descriptions shown in the UI come from the locale files.**
The Rust catalogs keep upstream's French `display_name` / `notes`; the UI
shows `modelText.<id>.name` / `.notes` instead (`src/lib/modelText.ts`, id with
non-alphanumerics replaced by `_`), falling back to the catalog text. When
porting a new model from upstream, add its en/fr/es `modelText` entry too.

**Model folders (engine v0.2.2).** Downloaded models live in one folder shared
with Talaan and Ensayo, `%LOCALAPPDATA%/Craft Concepts Digital/Models` (Mac:
`~/Library/Application Support/Craft Concepts Digital/Models`), managed by `voice-engine-models`. The old folders
(`Models/`, `ParakeetModels/`, `LlmModels/` under the app's data folder) are only
read once, at first start, when the files are moved into the shared folder;
imports stay in `LlmModels/` and `Models/imported/`. Never delete the shared
folder (the uninstaller does not).

**Do not bump the engine tag casually.** The llama.cpp and whisper.cpp versions
come from the engine's `Cargo.lock` at the tag, so a tag move changes both.
Always run the ignored smoke tests against real model files after a move.
Parakeet uses ONNX Runtime (static, via `voice-engine-stt`).

**Every downloadable model is credited in `THIRD_PARTY_NOTICES.md`.** Adding a
model means adding its author, licence and any required notice there (and the
licence text under `licenses/` if the licence asks for a copy). Parlato never
bundles or hosts model files; they download from the original host. Pin
third-party repositories to a commit (`revision` in the Parakeet catalog).

**i18n is a three-file change.** `src/i18n/locales/` holds en/es/fr, currently
889 keys each (including the `macOverrides` block) and exactly in sync. Any user-facing string means editing all three.
Adding a key to only `en.json` is a silent bug in two languages. French and Spanish
are written with their accents (converted 2026-10-05, commit 3d7925b; Spanish
questions and exclamations take their opening marks). New strings follow suit,
and so do the user-facing strings outside the locale files: the language names in
`src/i18n/index.ts` and the `\u{}` escapes in `tray.rs`. Code comments stay
unaccented ASCII.

**Tailwind scans every file in the repo, docs included.** A backslash path
in a Markdown file (a backslash followed by hex digits) reads as a CSS escape
and fails `npm run build` with "Invalid code point". Write paths in docs
with forward slashes.

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
backed up by the owner in Bitwarden (2026-09-30), and as the
`TAURI_SIGNING_PRIVATE_KEY` repository secret. It signs updates for Windows
and macOS. Never commit it, print it or copy it into the repo. Never replace the
public key: every installed copy only accepts updates signed with the
matching private key, so a new key strands all existing installs. Losing the
private key has the same effect.

**Releasing.** Bump the version in `package.json`, `package-lock.json`,
`src-tauri/Cargo.toml` and `src-tauri/tauri.conf.json`, add a `CHANGELOG.md`
entry, commit, then push a `vX.Y.Z` tag. Every release publishes its
changelog (owner rule, 2026-10-05): the workflow copies the
`## [X.Y.Z] - date` section of `CHANGELOG.md` into the release page under
"What's new", and fails if that section is missing. So rename
`## [Unreleased]` to the version and date before tagging. `.github/workflows/release.yml`
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
- (Historical: `.cargo/config.toml` once handled duplicate ggml symbols with
  `/FORCE:MULTIPLE`. Since engine v0.2.2 llama.cpp is not in the app, so the
  duplicates and the flag are gone.)
- `keyring` only enables `windows-native`. Move it to a Windows target block
  and add `apple-native` under `[target.'cfg(target_os = "macos")'.dependencies]`,
  otherwise API keys are not stored in the Keychain.
- (Historical: the `gpu-detect` feature and `nvml-wrapper` are gone since engine
  v0.2.2; hardware and graphics card detection come from the engine.)
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
- Not done yet then (Metal is done since engine v0.2.2): non-activating
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
- The local AI helper (`parlato-llm`, an `externalBin` sidecar in
  `Parlato.app/Contents/MacOS`) is signed by Tauri's bundler with the same
  identity and hardened runtime, inside out before the app (checked in
  tauri-bundler `macos/app.rs` and `sign.rs`; `release.yml` verifies it with
  `codesign -dv` and fails without `runtime` in its flags). It gets the app's
  `Entitlements.plist` too.
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
  The .p12 backup and its password are in the owner's Bitwarden
  (secure note "Parlato Signing", base64 text; restore with
  `pbpaste | base64 -d > parlato-signing.p12`).
- README + docs/INSTALL.md wait on branch `docs/mac-install` until the first
  release with a .dmg is published; merge it then.

**Mac follow-up from the Windows batch of 2026-10-05: done on the MacBook the
same day, no Mac code change needed.** Measured on an M3 Pro (6 performance +
6 efficiency cores, automatic = 9 threads), best runs, CPU only:
- Whisper ggml-base, 6 s / 29 s: 6 threads 240 / 498 ms, 9 threads 239 /
  540 ms, 12 threads 292 / 710 ms.
- Parakeet Ultra, 6 s / 29 s: 6 threads 105 / 490 ms, 9 threads 104 / 450 ms,
  12 threads 103 / 427 ms.
- Voice detection, 29 s: 2 threads 50 ms, 12 threads 250 ms.
So the shared three-quarters default stays on the Mac: it is the best
compromise (Parakeet likes more threads, Whisper slightly fewer), and every
count is well under a second. `parakeet_threads_smoke` now also reads
`PARLATO_THREADS`. Key names checked: the menu bar test passes on macOS and
the recorder merges `macOverrides.keys`. The original to-do list follows.

**Mac follow-up from the Windows batch of 2026-10-05 (to do on the MacBook).**
These changes landed in shared code from Windows. They build on macOS without
any Mac work, but their defaults were measured on the PC only. Pull first.
- **Speech threads.** `auto_threads()` in `transcription/whisper.rs` (three
  quarters of `available_parallelism()`, 12 on the 16-thread PC) is now the
  automatic thread count for both Whisper and Parakeet
  (`parakeet.rs::ensure_loaded`, new `threads` argument). The user setting is
  store key `speech_threads` (0 = automatic), commands `get/set_speech_threads`,
  UI `SpeechThreadsPanel.tsx` (Speech model page, any local model, with a
  "Use recommended settings" reset). Windows results (re-measured 2026-10-15
  on the -O2 build; the first numbers came from unoptimized code): Whisper ggml-base on 6 s of
  speech, 4 threads 1.0 s, 8 threads 0.60 s, 12 threads 0.48 s, 16 threads
  0.49 s, and on 49 s, 8 threads 2.3 s, 12 threads 1.6 s, 16 threads 5.0 s;
  Parakeet Ultra on 49 s, 4 threads 1.53 s, 12 threads 0.86 s, 16 threads
  0.87 s. 12 (three quarters) still wins.
  **Mac task:** Apple Silicon has performance and efficiency cores and no
  hyperthreading, so three quarters of all cores may not be best. Measure:
  `PARLATO_WHISPER_MODEL=<ggml-base.bin> PARLATO_WAV=<16 kHz wav>
  cargo test --release --lib whisper_threads_smoke -- --ignored --nocapture`
  (`PARLATO_THREADS=4,6,8,10` and `PARLATO_SHORT_ONLY=1` narrow it) and
  `PARLATO_PARAKEET_DIR=<parakeet-ultra dir> PARLATO_WAV=<wav>
  cargo test --release --lib parakeet_threads_smoke -- --ignored --nocapture`
  (that test loads Parakeet TDT, so use Ultra or v3, not Unified). Ask the
  owner before downloading a test model (ggml-base.bin is about 148 MB). If
  another count wins on the Mac (for example the performance-core count,
  `sysctl hw.perflevel0.physicalcpu`), add a `#[cfg(target_os = "macos")]`
  `auto_threads` next to the existing one and leave the Windows one alone.
- **Voice detection threads.** `vad_threads()` in `transcription/vad.rs` is
  now 2 (it used every thread and made Parakeet dictations hang for up to a
  minute on Windows). Re-measured with -O2, 49 s of audio: 1 thread 0.12 s,
  2 threads 0.22 s, 4 threads 0.38 s, 16 threads 2.4 s, so fewer is still
  better and 2 stays. Confirm on the Mac with `vad_threads_smoke` (same
  env vars as above, plus `PARLATO_VAD_MODEL=<ggml-silero.bin>`).
- **Translated key names.** The shortcut recorder reads key names from the
  `keys` locale block, with Mac names in `macOverrides.keys`. The menu bar
  label comes from `tray.rs::shortcut_label`, which has `cfg(macos)` rows.
  Check in French and Spanish that the recorder and the menu bar show Mac
  names (for example "Option droite", "Cmd").
- **Nothing to do on the Mac:** the power-modes backup (`power_mode/config.rs`
  keeps `parla.power_mode.backup.json` and refuses edits if the modes cannot
  be read), the coffee button and the three translated labels.
- **Not a Mac task, for the record:** Whisper's slowness on the CPU is not
  caused by the two ggml copies (a Whisper-only build ran at the same speed).
  The quantized large-v3-turbo model is very slow on the Windows CPU (over
  7 minutes for 6 s of speech at 12 threads); on the Mac, Metal (Phase 3) is
  the real fix. Update 2026-10-15: most of that was the missing `-O2` (see
  "Local environment notes"): 6 s of speech went from 152 s to 8.2 s.

**Phase 3: polish.**
- Mute other audio while recording (`audio/mute.rs`): written 2026-10-14 from
  the PC on branch `mac/pause-other-audio` (`audio/mute_macos.rs`, system
  `osascript` volume commands instead of Core Audio, so no new crate and no
  permission). Compiled and unit-tested by the macOS CI job; the owner tested it
  on a Mac on 2026-10-14 and it works (the check was: Pause other audio on,
  play music, dictate, music goes quiet and comes back). When it shipped, in the same
  release commit remove "pausing other audio" from README.md "Known
  limitations", docs/FAQ.md "What isn't on the Mac yet?" and docs/INSTALL.md
  "Not on Mac yet" (they describe the released version, so they must not
  change earlier).
- Recorder pill visible above full-screen apps and on every Space.
- Menu bar icon (template image) and a "Hide Dock icon" option (the tray
  code already mentions it as macOS-only).
- Metal acceleration for whisper.cpp and llama.cpp: done with engine v0.2.2
  (Whisper in process, local AI in the helper).

**Phase 4: later, only if wanted.**
- Power modes: frontmost app via NSWorkspace (done); installed apps list
  (written 2026-10-14 from the PC on branch `mac/installed-apps`, stacked on
  `mac/pause-other-audio`: `power_mode/installed_apps_macos.rs` scans the
  Applications folders and reads each Info.plist with the `plist` crate, plus
  the running apps from NSWorkspace; the Mac Power modes page now shows app
  pairing, not website pairing). The plist parsing is tested on Windows and on
  the macOS CI job; the owner tested the picker and pairing on a Mac on 2026-10-14
  and they work (Add app lists the apps, open ones first, and the mode
  applies only in the paired app). Browser URL via AppleScript
  (Automation permission) is still to do.
- Screen context: `xcap` capture (Screen Recording permission) plus Apple
  Vision OCR instead of `Media.Ocr`.
- `window_subclass.rs` has no Mac equivalent; leave it Windows-only.

## Guardrails for this fork

- **Do not touch native Windows systems code for now**: WASAPI audio capture
  (`audio/`), keyboard hooks (`hotkeys/`), OCR / UI Automation (`screen_context/`),
  and `window_subclass.rs`. These are the hardest to debug and are off-limits until
  more reps are built on lower-risk changes. The Mac port adds new macOS-only
  files in these folders (see "Mac port plan"); that is allowed, editing the
  Windows code paths is not. One exception, with the owner's OK (2026-10-22):
  `audio/mute.rs` was edited for the mute-restore fix (the mute state machine
  only). Capture code stays off-limits.
- **GPU use is allowed, on conditions** (owner decision, 2026-10-22; it
  replaces the old "CPU-only" rule). It must keep one installer per platform
  and the auto-updater, be a user choice with a fallback to the processor,
  and never stop the app from starting on a PC without that GPU. CUDA is not
  used at all (the `cuda*` features were removed in engine v0.2.2): do not add it. The GPU
  here is Blackwell (needs CUDA 12.8+) and the VS version is v18 while CUDA
  integrates with v17, two independent failure points. CUDA is its own future
  task. The Parakeet DirectML switch (see "Things that are easy to get wrong")
  is the first use.
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
