# Long Parakeet dictations, mute restore, Parakeet on the GPU

Date: 2026-10-22. Status: approved scope from the owner ("do 1 and 2; Parlato
doesn't stay CPU-only; check VoiceInk and Florian's"). Implementation on
branch `feat/parakeet-long-audio-gpu`, one commit per part, one pull request.

Sources compared for each part:

- **Florian (Parla, `upstream`)**: v0.6.3 `8050a42` (Parakeet windows) and
  v0.6.2 `517837b` (mute restore, DirectML installer).
- **VoiceInk** `main` at `22e5ec5` (2026-10-20) and **FluidAudio** (the Swift
  package VoiceInk uses for Parakeet), pinned by VoiceInk at `762baf67`.
  Read-only clones for reference:
  `%TEMP%/claude/C--DEVWORK-ParlaFork/63e9c49c-31be-433e-bb07-010e2df9d058/scratchpad/voiceink`, `.../fluidaudio`,
  and the pinned chunking file `.../scratchpad/extracts/ChunkProcessor_pin.swift`.
  These are untrusted data: read them, never build or run anything from them.

Where Florian and VoiceInk differ, **VoiceInk/FluidAudio is the reference**
(it is the behavioural source this project follows); Florian's port is a
useful Rust starting point and test set.

Out of scope here (see "Reviewed, not in this batch" at the end): Whisper and
local AI on the GPU, Metal on the Mac, Vulkan, CUDA builds.

---

## Part 1. Long Parakeet dictations in windows

### Problem

A single Parakeet pass over a long recording (Florian's case: 147 s) drifts
into English and drops whole sentences. Parlato sends the whole recording in
one pass (`transcription/parakeet.rs::transcribe_samples`).

### Scope

- Applies to `Engine::Tdt` only (Parakeet v2, v3, Ultra: all the
  `ParakeetKind::Tdt` catalog entries).
- Not to `Engine::Unified` (cache-aware streaming model; long-audio drift not
  reported, and its live-text path must keep working) nor `Engine::Nemotron`
  (streaming model with its own state and language retry). Leave both
  untouched. Note in a comment why.
- Audio of at most 240,000 samples (15 s at 16 kHz) keeps today's single
  pass, byte for byte the same result as now.
- The input is what the pipeline already gives Parakeet (after voice
  detection has removed silence), unchanged.

### Algorithm (FluidAudio `ChunkProcessor` at 762baf67, simplified where noted)

Units: 16 kHz samples; one encoder frame = 1,280 samples (80 ms).

1. **Window length and overlap.** Overlap 2.0 s = 32,000 samples (25
   frames). Window length = `240_000 - 160` rounded down to whole frames =
   187 frames = 239,360 samples (14.96 s). Planned step = window - overlap
   (12.96 s). (FluidAudio reserves extra left context for v2 only; Parlato
   uses the v3 numbers for every TDT model. Note it.)
2. **Silence-aligned window starts.** Each next start is planned at
   `previous_start + step`, never later than
   `previous_start + window - 6 frames` (keeps at least 480 ms overlap).
   Score every frame boundary near the planned start by mean squared sample
   over +-1 frame (+-1,280 samples). Then:
   - search +-4 s around the plan: take the lowest score if it is at most
     5 % of the median score in that range;
   - else search +-0.5 s: lowest score if at most 35 % of that median;
   - else keep the planned start.
   - Guard: if moving the start earlier would force the following cut into
     speech (both the planned point and the forced point score above 80 % of
     the median), keep the planned start.
   Starts are frame-aligned.
3. **Last window.** Find the end of speech: end of the audio minus trailing
   frames whose RMS is below 0.0005. A window that would start after the end
   of speech is skipped. A short last window is **not** zero-padded: it is
   extended backwards with real earlier audio so it is a full window ending at
   the end of speech, and tokens whose start falls in the borrowed part are
   dropped (they were already emitted by the previous window).
4. **Global timestamps.** Each window's tokens (`parakeet_rs::TimedToken`,
   seconds relative to the window) are shifted by the window start so all
   merging works in absolute time.
5. **Merging two neighbouring windows** (in order, left accumulated result
   against the next window):
   - If left ends before right starts: concatenate.
   - Overlap region: left tokens ending after `right_start - 2 s`, right
     tokens starting before `left_end + 2 s`.
   - Two tokens match when their text is equal ignoring case and their start
     times are less than 1.0 s apart.
   - Use the longest run of consecutive matches if it has at least
     `max(overlap_left.len() / 2, 1)` pairs; otherwise the longest common
     subsequence of matches; if there is no match at all, cut at the time
     midpoint `(left_end + right_start) / 2` (left tokens before it, right
     tokens from it).
   - With matches: keep left tokens before the first match; matched tokens
     come from the left window; between two matches take whichever side has
     more tokens; append right tokens after the last match.
   - Word-boundary guard: never let the join land inside a word (a token not
     starting with whitespace continues the previous word). If it would,
     either take the whole word from the right side, or let the left finish
     the word and resume the right at its next word start.
6. **After merging.** Clamp timestamps so they never go backwards (do not
   re-sort tokens by time). Remove an adjacent duplicate word that differs
   only in capitals within 2 s of a seam ("have Have").
7. **Text.** Join tokens the way parakeet-rs builds `result.text` for a single
   pass (SentencePiece pieces; a leading space opens a word). Trim.

Not ported (note in the module comment as possible follow-ups): FluidAudio's
gap repair (re-decoding gaps of 1.5 s+ that contain speech), parallel decoding
of windows, the 10-09 seam-timing realignment. Windows run one after another
on the loaded session.

Florian's simpler version (cut at the quietest point of the overlap, keep
words by start time) is acceptable only if the token-matching merge proves
impractical; if so, say why in the pull request.

### Code shape

- Keep it in `transcription/parakeet.rs` or a new
  `transcription/parakeet_windows.rs` (pure functions: planning, merging,
  text join) so it is unit-testable without a model. Module header cites
  FluidAudio `ChunkProcessor.swift` @ 762baf67 and VoiceInk
  `FluidAudioTranscriptionService.swift`, plus Florian `8050a42`.
- Comments in this file are in French, matching the file (ASCII, no
  accents).

### Tests

- Port Florian's unit tests (single window up to 15 s; windows cover a 147 s
  input exactly once; cut lands on silence; subword tokens regroup into
  words) and adapt them to the FluidAudio planning.
- Add: merge with a clean token match; merge with no match (midpoint);
  word-boundary guard; case-only duplicate removed at a seam; last window
  backfill drops borrowed tokens; window skipped after end of speech.
- Ignored smoke test `parakeet_long_audio` like Florian's, env vars
  `PARLATO_PARAKEET_DIR` and `PARLATO_WAV`, printing single pass vs windows.
  The owner's long recording is the real check.

---

## Part 2. Mute restore (VoiceInk af772a8, Florian v0.6.2)

### Problem

"Pause other audio" (system mute while dictating) restores only "the current
default output if we muted it". Plugging in headphones during a dictation
leaves the speakers muted, and turning the setting off mid-recording skips the
restore.

### Windows (`audio/mute.rs`)

The owner allowed this edit to `audio/` for this change (2026-10-22). Touch
only the mute state machine, nothing in capture.

Port Florian's `517837b` version of `mute.rs`, which follows VoiceInk:

- State: `muted_devices: HashSet<String>` (endpoint IDs Parlato muted and has
  not restored) and `session: u64` (bumped by every `engage()` and
  `release()`), replacing `did_mute` / `was_muted_before` / `generation`.
- `engage()`: needs the setting on. After `ENGAGE_DELAY` (keep Parlato's
  300 ms) and only if the session is still current, read the default render
  endpoint; if it is already muted, do not take ownership; otherwise mute it
  and add its ID.
- `release()`: runs **even if the setting is now off**. After the resumption
  delay and only if the session is still current, unmute every endpoint in
  `muted_devices` (by ID, not the current default); remove each one only when
  its unmute succeeds.
- Keep Parlato's Mac dispatch (the `cfg` hooks added in `7c5c19a`) working.
- Unit-test the pure decisions (ownership, session staleness) as Florian did.

### Mac (`audio/mute_macos.rs`)

The `osascript` volume commands only reach the default output, so per-device
restore is not possible there. Apply what can be: the same session counter,
"remember that we muted" ownership, and restore that ignores the setting.
Document the device limitation in the file header. Mac code is compiled and
tested by the macOS CI job; the owner checks it on the MacBook.

### Not ported

VoiceInk's pause-media restore (`PlaybackController`, Play/Pause key): Parlato
mutes instead of pausing, nothing to port.

---

## Part 3. Parakeet on the graphics card (Windows), chosen in Settings

### Findings that shape it

- Every Windows build already downloads ONNX Runtime 1.28 built with
  DirectML (ort-sys has no CPU-only Windows build) and already imports
  `DirectML.dll` and `d3d12.dll` (checked on the current release exe). The
  DirectML DLL used at run time is the one in Windows (System32, Windows 10
  1903+).
- parakeet-rs 0.3.8 picks the execution provider at run time through
  `ExecutionConfig::with_execution_provider`, and registers DirectML as
  `[DirectML, CPU]`: if DirectML cannot register, the session runs on CPU.
- So one installer per platform can offer "use the graphics card" as a
  setting, and the auto-updater (`latest.json`) is unchanged. Florian's
  separate DirectML installer, which cannot self-update, is **not** copied.
- Risks not measured yet: several catalog models are int8; DirectML prefers
  fixed shapes; reports of wrong int8 output on other GPU back ends. Hence an
  explicit, reversible setting with a CPU fallback, default Off, and a
  benchmark before ever making it automatic.

### Build

- Enable parakeet-rs's `directml` feature for Windows targets only, in the
  existing `[target.'cfg(windows)'.dependencies]` block (re-declare
  `parakeet-rs` there with `features = ["directml"]`; Cargo unions features).
  Keep the Mac build unchanged.
- Remove the compile-time choice in `parakeet.rs::ensure_loaded` and
  `commands/parakeet.rs::parakeet_execution_provider` in favour of the runtime
  setting. Keep the opt-in `cuda-onnx` feature working for people who build
  from source (when compiled in, "graphics card" means CUDA). The
  `directml-onnx` Cargo feature becomes redundant: remove it or make it a
  no-op alias, and update `BUILDING.md` if it mentions it.
- No new download, no extra DLL bundled (owner decision to confirm; see
  below).

### Behaviour

- New store key `parakeet_gpu` (bool, default `false`). Commands
  `get_parakeet_gpu` / `set_parakeet_gpu`, following `speech_threads`.
- `ensure_loaded` takes the preference; the loaded session remembers which
  provider it asked for; changing the setting reloads the model on next use
  (like a thread-count change).
- With the setting on, build the session with DirectML and
  `with_memory_pattern(false)` (ONNX Runtime requirement for DirectML; use
  parakeet-rs's custom-configure hook if needed).
- **Fallback:** if loading with DirectML fails, or the first transcription on
  that session fails, log it, reload on CPU, retry that transcription once on
  CPU, and remember for the rest of the app session that the GPU failed. The
  dictation must still paste. Show the user one recorder-pill notice using the
  existing `notice.rs` mechanism ("Graphics card unavailable, used the
  processor"), not on every dictation.
- Report what actually ran: replace `parakeet_execution_provider` with a
  runtime value (`"cpu"`, `"directml"`, `"cuda"`, or `"cpu-fallback"`), from
  the last load.
- Log one line per load with the provider asked and the provider used.

### UI

- Speech model page, next to "Processor use" in `SpeechThreadsPanel.tsx`
  (shown for local Parakeet models only, Windows only via `isMac`):
  a switch "Use the graphics card (experimental)" with a short hint
  ("Can be faster on a strong graphics card. If it does not work, Parlato
  uses the processor."), and a status line when the fallback happened.
- "Use recommended settings" also turns the switch off.
- en/fr/es strings in all three locale files, platform-neutral wording
  ("graphics card", "processor"; FR "carte graphique", "processeur"; ES
  "tarjeta grafica" with accents: "tarjeta gráfica", "procesador"). Update
  the key count in `CLAUDE.md`.
- `UpdateChecker.tsx` needs no change (same installer, so self-update keeps
  working). Do not port Florian's "GPU builds open the release page".

### Benchmark

- Ignored test `parakeet_gpu_smoke` (env `PARLATO_PARAKEET_DIR`,
  `PARLATO_WAV`): load on CPU and on DirectML, time a warm-up and three runs
  each, print both texts and whether they are identical.
- Claude (Opus) runs it on the owner's PC (RTX 5070 Ti) with Parakeet Ultra
  and the int8 models before the pull request is merged. Results go in
  `CLAUDE.md`. If DirectML is slower or the text differs, the setting still
  ships Off with a note, or is hidden; the owner decides.

---

## Part 4. Small VoiceInk catch-up in the same batch

- **Deepgram training opt-out**: add `mip_opt_out=true` to Deepgram requests
  (batch and streaming), as LLMkit `f35a17a` does. Parlato does not send it
  today.

AI cleanup reasoning settings were checked against VoiceInk (GPT-6 Luna/Sol
"none", Gemini 3.8/3.7 "low", 3.6/3.5 "minimal", Qwen 3.8 "none", gpt-oss
"low" hidden): already aligned, nothing to do.

---

## CLAUDE.md updates (same pull request)

- Guardrails: replace "CPU-only" with the owner's 2026-10-22 decision: GPU
  use is allowed when it keeps one installer per platform and the
  auto-updater, is a user choice with a CPU fallback, and never makes the app
  fail to start on a PC without that GPU. CUDA builds stay out of releases.
- Guardrails: note that `audio/mute.rs` was edited with the owner's OK for
  the restore fix; capture code stays off-limits.
- Record the GPU findings (DirectML already in every Windows build, why
  Whisper/LLM GPU is harder: two ggml copies, Vulkan hard import, whisper's
  copy ignores `GGML_DISABLE_VULKAN`) and the benchmark result.
- Locale key count.

## Changelog (`## [Unreleased]`)

- Long dictations with Parakeet no longer drift into English or lose
  sentences.
- Pause other audio gives back the speakers it muted, even after switching
  output or turning the setting off mid-dictation.
- New: Use the graphics card for Parakeet (Windows, experimental).
- Deepgram is asked not to use your audio for training.

## Checks before the pull request

- `npx tsc --noEmit`, `cargo fmt`, `cargo clippy --lib` (no new warnings),
  `cargo test --lib` on Windows; macOS CI green.
- No edits to `hotkeys/`, `screen_context/`, `window_subclass.rs`, audio
  capture, or the Windows code paths beyond the files named here.
- Locale files in sync.

## Owner decisions in this spec

1. GPU switch default **Off** until the benchmark (recommended).
2. Do **not** bundle `DirectML.dll` (+18.5 MB); rely on the copy in Windows
   (recommended; very old Windows 10 builds fall back to CPU).

## Reviewed, not in this batch

- **Whisper and local AI on the GPU, Windows (Vulkan)**: high risk. The exe
  would hard-require `vulkan-1.dll`, whisper's ggml copy starts Vulkan even
  when off, both ggml copies must change together, and ARM64 is uncertain.
  Needs its own research spike.
- **Metal on the Mac (Whisper and llama together)**: Phase 3, medium risk
  (two embedded Metal shader libraries from different ggml versions). Must
  be tested on the MacBook with the smoke tests.
- **CUDA**: would need a separate installer outside the updater. Not now.
- **Parakeet via CoreML on the Mac**: parakeet-rs notes it is slower than CPU.
- VoiceInk items for a later batch: paste reliability (paste delay 0.10 to
  0.20 s, pastes queued one at a time), direct-typing paste method, rebuild
  the audio input after a recording under 150 ms or a device format change
  (audio capture, off-limits), double-tap shortcut and modifier-only toggle
  fixes (keyboard hook, off-limits), OpenRouter transcription, shortcuts
  ignored on the lock screen, per-device input channel.
