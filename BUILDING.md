# Building Parla

This document describes how to build Parla from source on Windows.

## Prerequisites

### Required

- **Windows 10 22H2 or Windows 11**
- **[Rust](https://rustup.rs/)** stable toolchain (tested with 1.95+)
- **[Node.js 20+](https://nodejs.org/)** and `npm`
- **Microsoft Visual Studio 2022 Build Tools** with the "Desktop development with C++" workload (or Visual Studio Community/Professional/Enterprise)
- **WebView2 Runtime** (preinstalled on Windows 11, installed automatically by the Parla installer)
- **CMake** and **LLVM** (`LIBCLANG_PATH` set to its `bin` folder): whisper.cpp is compiled from source and `whisper-rs-sys` runs `bindgen`.
- **Git**, for the shared engine (see below).

No CUDA and no NVIDIA toolkit is needed or supported. The graphics card is used through Vulkan (local AI helper, Windows x64), DirectML (Parakeet, Windows) and Metal (Mac), all in the normal build.

## The shared engine and the local AI helper

Speech models, the model catalog and downloader, and the local AI helper come from [voice-engine](https://github.com/mmnzns/voice-engine), pinned by tag (`v0.2.2`) in `src-tauri/Cargo.toml`. Cargo downloads the engine crates itself; nothing to do for the speech part.

Local AI runs in a separate program, the `parlato-llm` helper, which Tauri bundles as a sidecar (`bundle.externalBin` in `tauri.conf.json`). **Tauri refuses to build until `src-tauri/binaries/parlato-llm-<target triple>[.exe]` exists.** The folder is git-ignored.

- **Mac (Apple silicon):** run `scripts/fetch-helpers.sh` once. It clones the engine at the pinned tag into `~/.cache/parlato/` and builds the helper there (several minutes the first time because llama.cpp is compiled; needs `jq` and `cmake`). Set `ENGINE_REF` to use another tag.
- **Windows:** the engine's `scripts/build-helpers.ps1 -Target x86_64-pc-windows-msvc -OutDir <folder> -EngineRef v0.2.2 -Helpers llm -LlmName parlato-llm` (from a clone of the engine at that tag) builds it; copy the result into `src-tauri/binaries/`. Releases do this in CI. For `cargo test --lib` alone an empty placeholder file with the right name is enough.
- **Releases:** `.github/workflows/release.yml` calls the engine's reusable workflow (`build-helpers.yml@v0.2.2`, `helpers: llm,llm-vulkan`, `llm-name: parlato-llm`), downloads `helpers-<triple>` into `src-tauri/binaries`, and the Windows x64 job passes `src-tauri/tauri.windows-x64.conf.json` (`--config`) so `parlato-llm-vulkan` is bundled too. Windows ARM64 and the Mac ship `parlato-llm` only.

`src-tauri/.cargo/config.toml` carries the engine's build settings (fixed CPU baseline, macOS 11 minimum, `-O2` for whisper.cpp). Keep it equal to the engine's "Build settings an app must copy"; Cargo does not read the engine's own file when it builds Parlato.

## Clone the repo

```bash
git clone https://github.com/LitteRabbit-37/Parla.git
cd Parla
```

## Install dependencies

```bash
npm install
```

## Development mode

Run the app in hot-reload dev mode (Vite dev server + Tauri dev):

```bash
npm run tauri dev
```

The first launch builds the Rust backend which can take a few minutes (whisper.cpp is compiled from source through `whisper-rs-sys`). Build the local AI helper first (previous section).

## Release build

```bash
npm run tauri build
```

This produces:

- `src-tauri/target/release/parla.exe` - the standalone binary
- `src-tauri/target/release/bundle/nsis/Parla_x.y.z_x64-setup.exe` - NSIS installer (the only bundle target we ship; it prompts for data removal on uninstall and lets the user opt out of the desktop shortcut)

`src-tauri/.cargo/config.toml` already sets the CPU baseline and `-O2`. The CI jobs also set the ggml baseline in the environment (x86-64-v3: AVX2, no AVX-512) and check the result. If you will share the build, do the same locally:

```powershell
. .\scripts\cpu-baseline.ps1
Set-GgmlBaseline x64      # arm64 for an ARM64 build
Reset-StaleGgml src-tauri/Cargo.toml x64 release
npm run tauri build
Assert-GgmlBaseline src-tauri/Cargo.toml x64 release
```

For an ARM64 build pass `arm64` and add `aarch64-pc-windows-msvc` as a last argument to the three functions. `Assert-GgmlBaseline` also fails a release build when the whisper.cpp cache has no `-O2`. (The helper's llama.cpp baseline is checked by the engine's workflow.) For a Windows x64 release build also pass `--config src-tauri/tauri.windows-x64.conf.json` so the Vulkan helper is bundled.

## ARM64 build

To build Parla natively for Windows on ARM (Snapdragon X, etc.):

### Prerequisites for ARM64

- **Clang/LLVM tools**: In the Visual Studio Installer, under \"Desktop development with C++\", ensure \"C++ Clang Compiler for Windows\" and \"MSBuild support for LLVM toolset\" are checked. The ggml library (used by whisper.cpp, and by llama.cpp in the helper) does not support MSVC on ARM64 — it requires Clang.
- **Ninja**: Install via `choco install ninja`, or ensure it is available from the Visual Studio installation.

### Build

```powershell
rustup target add aarch64-pc-windows-msvc
$env:CC = "clang-cl"
$env:CXX = "clang-cl"
$env:CMAKE_GENERATOR = "Ninja"
npm run tauri build -- --target aarch64-pc-windows-msvc
```

This produces:

- `src-tauri/target/aarch64-pc-windows-msvc/release/parla.exe` - the standalone binary
- `src-tauri/target/aarch64-pc-windows-msvc/release/bundle/nsis/Parla_x.y.z_arm64-setup.exe` - NSIS installer

All functionality (Whisper, Parakeet, local AI, audio capture, etc.) works natively on ARM64. There is no Vulkan helper for ARM64, so local AI runs on the processor there.

The CI workflow `.github/workflows/release.yml` builds the ARM64 variant automatically on GitHub's `windows-11-arm` runner.

## Graphics card

There is no `[features]` table any more: one build serves every machine. There are no CUDA features. On Windows the ONNX Runtime DirectML provider is always compiled in (through `voice-engine-stt/directml`); Parakeet uses it when "Use graphics card" is on. Whisper and local AI use Metal on the Mac, and local AI uses the Vulkan helper on Windows x64.

The CI workflow `.github/workflows/release.yml` builds x64 and ARM64 Windows installers and the Mac `.dmg`.

## Tests

```bash
cd src-tauri
cargo test --lib
```

The Rust test suite covers hotkeys state machine, power mode matching, text filters, URL validator, prompt detection, cloud catalog and enhancement helpers.

## Auto-updater signing keys

If you plan to distribute builds via GitHub Releases with the Tauri auto-updater, you need a signing key pair:

```bash
npm run tauri signer generate
```

This prints a public key and a private key. Set the public key in `src-tauri/tauri.conf.json` under `plugins.updater.pubkey`, and store the private key in your repo secrets as `TAURI_SIGNING_PRIVATE_KEY` (with `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` if you set a passphrase). The `.github/workflows/release.yml` workflow picks these up automatically.

## Troubleshooting

- **rustc STATUS_ACCESS_VIOLATION on tokenizers**: already mitigated via `[profile.release.package.tokenizers] opt-level = 1` in `Cargo.toml`. If you still hit it, update your rustc version.
- **`tauri-build` says a binary is missing** (`binaries/parlato-llm-<triple>`): build the local AI helper, see "The shared engine and the local AI helper".
- **Slow whisper.cpp in a release build:** check `CMAKE_CXX_FLAGS_RELEASE` in `target/release/build/whisper-rs-sys-*/out/build/CMakeCache.txt` has `-O2`; if you changed `.cargo/config.toml`, run `cargo clean --release -p whisper-rs-sys`.

## Project layout

See the [README.md](README.md) "Architecture" section for a full module tour.
