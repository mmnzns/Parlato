# CLAUDE.md

Guidance for Claude Code when working in this repository.

## What this is

Parla is a native Windows voice-to-text app: it transcribes speech and pastes the
result at the cursor. It is a fork of
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

**i18n is a three-file change.** `src/i18n/locales/` holds en/es/fr, currently
587 keys each and exactly in sync. Any user-facing string means editing all three.
Adding a key to only `en.json` is a silent bug in two languages.

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

**App identity is still upstream's.** `src-tauri/tauri.conf.json` has
`identifier: "com.litterabbit.parla"`. Changing it is a deliberate branding task,
not a drive-by edit — it affects installer upgrade paths and stored settings
location.

## Guardrails for this fork

- **Do not touch native Windows systems code for now**: WASAPI audio capture
  (`audio/`), keyboard hooks (`hotkeys/`), OCR / UI Automation (`screen_context/`),
  and `window_subclass.rs`. These are the hardest to debug and are off-limits until
  more reps are built on lower-risk changes.
- **CPU-only.** Do not add CUDA features to the build. The GPU here is Blackwell
  (needs CUDA 12.8+) and the VS version is v18 while CUDA integrates with v17 —
  two independent failure points. CUDA is its own future task.
- **Preserve GPL-3.0.** Keep the LICENSE and all copyright/license headers. Note
  modifications if anything is ever distributed.
- **Commit only what builds and works.** One logical change per commit.
- `upstream` remote points at LitteRabbit-37/Parla; `origin` is this fork. Pull
  upstream periodically rather than drifting.
