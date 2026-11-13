// MOCK DATA ONLY. Nothing here talks to the real Parlato backend, settings,
// models or Keychain. Every value is made up for QA screenshots.
//
// `handle(cmd, args)` is the mockIPC callback: one entry per Tauri command the
// frontend invokes (see src/lib/tauri.ts), plus the Tauri plugin commands
// (store, updater, dialog, opener, autostart, process, window, app).

import { emit } from "@tauri-apps/api/event";

type Args = Record<string, unknown> | undefined;
type Handler = (a: Args) => unknown | Promise<unknown>;

const params = new URLSearchParams(window.location.search);
export const FLAGS = {
  onboarding: params.get("onboarding") === "1",
  permMissing: params.get("perm") === "missing",
};

const GB = 1024 * 1024 * 1024;
const MB = 1024 * 1024;
const sleep = (ms: number) => new Promise((r) => setTimeout(r, ms));
const uid = () => Math.random().toString(36).slice(2, 10) + "-" + Date.now().toString(36);
const localDay = (d: Date) =>
  `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, "0")}-${String(d.getDate()).padStart(2, "0")}`;
const isoDay = (back: number) => {
  const d = new Date();
  d.setDate(d.getDate() - back);
  return localDay(d);
};
const minutesAgo = (m: number) => new Date(Date.now() - m * 60_000).toISOString();

// -- Mutable state ----------------------------------------------------------

const settings: Record<string, unknown> = {
  onboarding_completed: !FLAGS.onboarding,
  dictation_language: "auto",
  speech_threads: { value: 4, auto: 4, max: 10 },
  use_gpu: true,
  model_keep_alive: { value: 300, recommended: 300 },
  close_to_tray: true,
  hide_dock_icon: false,
  system_mute: false,
  audio_resumption_delay: 0,
  sound_feedback: true,
  enhancement_enabled: true,
  active_prompt_id: "00000000-0000-0000-0000-000000000001",
  llm_selection: { provider_id: "llamacpp", model: "granite-4.2-3b" },
  ollama_base_url: "http://localhost:11434",
  custom_base_url: null,
  localcli_cmd: null,
  localcli_timeout: 30,
  selected_gguf: "granite-4.2-3b",
  llamacpp_settings: { context_size: 4096, max_tokens: 1024 },
  power_auto_restore: true,
  screen_context: false,
  vad_enabled: true,
  recorder_style: "mini",
  ui_language: "en",
  cloud_timeout: 30,
  input_device: null,
  selected_whisper: "whisper-turbo",
  retention: {
    transcription_cleanup: false,
    transcription_retention_minutes: 1440,
    audio_cleanup: true,
    audio_retention_days: 7,
  },
  text_processing: {
    text_formatting_enabled: true,
    spoken_commands: true,
    spoken_punctuation: false,
    review_before_paste: false,
    remove_filler_words: true,
    filler_words: ["uh", "um", "hmm", "you know"],
    append_trailing_space: true,
    restore_clipboard_after_paste: true,
  },
  source: {
    kind: "local",
    whisper_model_id: "whisper-turbo",
    cloud_provider: null,
    cloud_model: null,
    parakeet_model_id: null,
  },
  hotkeys: {
    primary: { trigger: { kind: "modifier", option: "rightAlt" }, mode: "hybrid" },
    secondary: { trigger: { kind: "none" }, mode: "hybrid" },
    actions: {
      copy_last_transcription: { kind: "none" },
      paste_last_transcription: { kind: "none" },
      paste_last_enhancement: { kind: "none" },
      retry_last_transcription: { kind: "none" },
      open_history: { kind: "none" },
      cancel_recording: { kind: "none" },
    },
    double_tap: true,
  },
  vad: { downloaded: true, path: "/mock/Models/vad/ggml-silero-v6.2.0.bin", on_disk_bytes: 885_000, other_apps: [] as string[] },
};

const DEFAULT_HOTKEYS = JSON.parse(JSON.stringify(settings.hotkeys));

// Cloud speech API keys: only Groq has one.
const apiKeys: Record<string, boolean> = { groq: true };

// -- Speech models ----------------------------------------------------------

type Whisper = {
  id: string; display_name: string; size_bytes: number; multilingual: boolean; notes: string;
  downloaded: boolean; on_disk_bytes: number | null; path: string | null; imported: boolean;
  speed: number; accuracy: number; language_codes: string[];
  downloading_elsewhere?: boolean; other_apps?: string[];
};
const WHISPER_LANGS = ["auto", "en", "fr", "es", "de", "it", "pt", "nl", "ja", "zh"];
function whisper(id: string, name: string, mb: number, multi: boolean, speed: number, acc: number, notes: string): Whisper {
  return {
    id, display_name: name, size_bytes: mb * MB, multilingual: multi, notes,
    downloaded: false, on_disk_bytes: null, path: null, imported: false,
    speed, accuracy: acc, language_codes: multi ? WHISPER_LANGS : ["en"],
    downloading_elsewhere: false, other_apps: [],
  };
}
const whisperModels: Whisper[] = [
  whisper("whisper-tiny", "Whisper Tiny", 75, true, 0.95, 0.3, "Fastest, lowest accuracy. Fine for quick notes."),
  whisper("whisper-base-en", "Whisper Base (English)", 142, false, 0.85, 0.45, "Small English-only model."),
  whisper("whisper-small", "Whisper Small", 466, true, 0.65, 0.65, "Good balance for most computers."),
  whisper("whisper-medium-en", "Whisper Medium (English)", 1500, false, 0.4, 0.8, "Accurate English model."),
  whisper("whisper-large-v3", "Whisper Large v3 (compressed)", 1100, true, 0.3, 0.92, "Very accurate, many languages."),
  whisper("whisper-turbo", "Whisper Large v3 Turbo (compressed)", 574, true, 0.7, 0.9, "Near-large accuracy at much higher speed."),
];
function markDownloaded(m: { downloaded: boolean; on_disk_bytes: number | null; path: string | null; size_bytes: number; id: string }) {
  m.downloaded = true;
  m.on_disk_bytes = m.size_bytes;
  m.path = `/mock/Models/${m.id}`;
}
markDownloaded(whisperModels.find((m) => m.id === "whisper-turbo")!);
// Shared with Talaan in the preview, so Delete shows the two-choice dialog.
whisperModels.find((m) => m.id === "whisper-turbo")!.other_apps = ["Talaan"];

type Parakeet = {
  id: string; display_name: string; multilingual: boolean; is_quantized: boolean; size_bytes: number;
  notes: string; downloaded: boolean; missing_files: string[]; on_disk_bytes: number | null; path: string | null;
  speed: number; accuracy: number; language_codes: string[]; downloading_elsewhere?: boolean; other_apps?: string[];
};
const EU_LANGS = ["auto", "en", "fr", "es", "de", "it", "pt", "nl", "pl", "ru", "uk"];
function parakeet(id: string, name: string, multi: boolean, quant: boolean, mb: number, speed: number, acc: number, notes: string): Parakeet {
  return {
    id, display_name: name, multilingual: multi, is_quantized: quant, size_bytes: mb * MB, notes,
    downloaded: false, missing_files: [], on_disk_bytes: null, path: null,
    speed, accuracy: acc, language_codes: multi ? EU_LANGS : ["en"],
    downloading_elsewhere: false, other_apps: [],
  };
}
const parakeetModels: Parakeet[] = [
  parakeet("parakeet-v3", "Parakeet v3 (compressed)", true, true, 640, 0.9, 0.85, "25 European languages."),
  parakeet("parakeet-v3-full", "Parakeet v3 (full precision)", true, false, 2500, 0.8, 0.88, "25 European languages, larger."),
  parakeet("parakeet-v2", "Parakeet v2 (compressed)", false, true, 640, 0.9, 0.84, "English only."),
  parakeet("parakeet-v2-full", "Parakeet v2 (full precision)", false, false, 2400, 0.8, 0.87, "English only, larger."),
  parakeet("parakeet-ultra", "Parakeet Ultra", true, true, 380, 0.97, 0.7, "Smallest and fastest, many European languages."),
  parakeet("parakeet-unified-en", "Parakeet Unified (English)", false, true, 700, 0.85, 0.86, "English with punctuation."),
  parakeet("parakeet-unified-en-full", "Parakeet Unified (English, full)", false, false, 2600, 0.75, 0.89, "English with punctuation, larger."),
];

type Gguf = {
  id: string; display_name: string; size_bytes: number; context_length: number; notes: string;
  downloaded: boolean; on_disk_bytes: number | null; path: string | null; imported: boolean; removable: boolean;
  downloading_elsewhere?: boolean; other_apps?: string[];
};
function gguf(id: string, name: string, gb: number, ctx: number, notes: string): Gguf {
  return {
    id, display_name: name, size_bytes: Math.round(gb * GB), context_length: ctx, notes,
    downloaded: false, on_disk_bytes: null, path: null, imported: false, removable: true,
    downloading_elsewhere: false, other_apps: [],
  };
}
const ggufModels: Gguf[] = [
  gguf("granite-4.2-3b", "IBM Granite 4.2 3B", 1.9, 32768, "Recommended. Fast, runs well on most computers."),
  gguf("qwen3.5-4b", "Qwen 3.5 4B", 2.6, 32768, "A little slower, better with long text."),
  gguf("qwen3.5-9b", "Qwen 3.5 9B", 5.4, 32768, "Best quality, needs 16 GB of memory."),
];
markDownloaded(ggufModels[0]);

// -- Cloud speech + LLM providers -------------------------------------------

const CLOUD_PROVIDERS = [
  { id: "groq", display_name: "Groq", api_key_url: "https://console.groq.com/keys" },
  { id: "deepgram", display_name: "Deepgram", api_key_url: "https://console.deepgram.com" },
  { id: "elevenlabs", display_name: "ElevenLabs", api_key_url: "https://elevenlabs.io/app/settings/api-keys" },
  { id: "mistral", display_name: "Mistral", api_key_url: "https://console.mistral.ai/api-keys" },
  { id: "soniox", display_name: "Soniox", api_key_url: "https://console.soniox.com" },
  { id: "speechmatics", display_name: "Speechmatics", api_key_url: "https://portal.speechmatics.com" },
  { id: "assemblyai", display_name: "AssemblyAI", api_key_url: "https://www.assemblyai.com/dashboard" },
  { id: "xai", display_name: "xAI", api_key_url: "https://console.x.ai" },
  { id: "gemini", display_name: "Gemini", api_key_url: "https://aistudio.google.com/apikey" },
  { id: "openrouter", display_name: "OpenRouter", api_key_url: "https://openrouter.ai/keys" },
];
const CLOUD_MODELS = [
  ["groq", "whisper-large-v3-turbo", "Groq Whisper Large v3 Turbo", 0.95, 0.88],
  ["groq", "whisper-large-v3", "Groq Whisper Large v3", 0.85, 0.92],
  ["deepgram", "nova-3", "Deepgram Nova 3", 0.9, 0.9],
  ["elevenlabs", "scribe_v1", "ElevenLabs Scribe", 0.7, 0.94],
  ["mistral", "voxtral-mini-latest", "Mistral Voxtral Mini", 0.85, 0.88],
  ["soniox", "stt-async-v4", "Soniox V4", 0.8, 0.9],
  ["speechmatics", "enhanced", "Speechmatics Enhanced", 0.7, 0.9],
  ["assemblyai", "universal-3.6-pro", "AssemblyAI Universal 3.6 Pro", 0.75, 0.92],
  ["xai", "grok-stt", "xAI Grok STT", 0.8, 0.85],
  ["gemini", "gemini-3-flash", "Gemini 3 Flash", 0.8, 0.88],
  ["openrouter", "mistralai/voxtral-mini-transcribe-3-0", "Voxtral Mini Transcribe 3 (OpenRouter)", 0.8, 0.95],
  ["openrouter", "openai/gpt-transcribe", "GPT Transcribe (OpenRouter)", 0.7, 0.97],
] as const;

const LLM_PROVIDERS = [
  { id: "openai", label: "OpenAI", endpoint: "https://api.openai.com/v1/chat/completions", default_model: "gpt-6-luna", models: ["gpt-6-luna", "gpt-6-mini"], requires_api_key: true },
  { id: "anthropic", label: "Anthropic", endpoint: "https://api.anthropic.com/v1/messages", default_model: "claude-haiku-5", models: ["claude-haiku-5", "claude-sonnet-5"], requires_api_key: true },
  { id: "gemini", label: "Gemini", endpoint: "https://generativelanguage.googleapis.com", default_model: "gemini-3-flash", models: ["gemini-3-flash", "gemini-3-pro"], requires_api_key: true },
  { id: "groq", label: "Groq", endpoint: "https://api.groq.com/openai/v1/chat/completions", default_model: "llama-4-scout", models: ["llama-4-scout", "llama-4-maverick"], requires_api_key: true },
  { id: "cerebras", label: "Cerebras", endpoint: "https://api.cerebras.ai/v1/chat/completions", default_model: "llama-4-scout", models: ["llama-4-scout"], requires_api_key: true },
  { id: "mistral", label: "Mistral", endpoint: "https://api.mistral.ai/v1/chat/completions", default_model: "mistral-small-latest", models: ["mistral-small-latest"], requires_api_key: true },
  { id: "openrouter", label: "OpenRouter", endpoint: "https://openrouter.ai/api/v1/chat/completions", default_model: "openrouter/auto", models: ["openrouter/auto"], requires_api_key: true },
  { id: "llamacpp", label: "Local AI (built in)", endpoint: "local", default_model: "granite-4.2-3b", models: ["granite-4.2-3b", "qwen3.5-4b", "qwen3.5-9b"], requires_api_key: false },
  { id: "ollama", label: "Ollama", endpoint: "http://localhost:11434", default_model: "", models: [], requires_api_key: false },
  { id: "custom", label: "Custom (OpenAI compatible)", endpoint: "", default_model: "", models: [], requires_api_key: false },
  { id: "localcli", label: "Local command", endpoint: "", default_model: "", models: [], requires_api_key: false },
];

// -- Prompts, dictionary, power modes, history -------------------------------

type Prompt = {
  id: string; title: string; prompt_text: string; icon: string; description: string | null;
  is_predefined: boolean; trigger_words: string[]; use_system_instructions: boolean;
};
const prompts: Prompt[] = [
  { id: "00000000-0000-0000-0000-000000000001", title: "Default", prompt_text: "Clean up the transcript: fix grammar, remove filler words, keep my wording.", icon: "checkmark-seal", description: "Cleans up the transcript (grammar, filler words, formatting).", is_predefined: true, trigger_words: [], use_system_instructions: true },
  { id: "00000000-0000-0000-0000-000000000002", title: "Assistant", prompt_text: "Answer the request directly, without markdown.", icon: "chat", description: "Assistant mode: a direct answer, no markdown.", is_predefined: true, trigger_words: ["hey assistant"], use_system_instructions: false },
  { id: "mock-prompt-email", title: "Polite email", prompt_text: "Rewrite as a short, polite email.", icon: "envelope", description: "Turns a dictation into a short email.", is_predefined: false, trigger_words: ["email"], use_system_instructions: true },
];
const extraTemplates: Prompt[] = [
  { id: "mock-tpl-bullets", title: "Bullet points", prompt_text: "Turn the text into short bullet points.", icon: "list", description: "Bullet list of the main points.", is_predefined: false, trigger_words: [], use_system_instructions: true },
  { id: "mock-tpl-formal", title: "Formal tone", prompt_text: "Rewrite in a formal tone.", icon: "briefcase", description: "More formal wording.", is_predefined: false, trigger_words: [], use_system_instructions: true },
];

type Word = { id: string; original_text: string; replacement_text: string; date_added: string; is_enabled: boolean };
const words: Word[] = [
  { id: "w1", original_text: "parlato", replacement_text: "Parlato", date_added: minutesAgo(60 * 24 * 9), is_enabled: true },
  { id: "w2", original_text: "acme corp", replacement_text: "Acme Corp (fake)", date_added: minutesAgo(60 * 24 * 6), is_enabled: true },
  { id: "w3", original_text: "kubernetes", replacement_text: "Kubernetes", date_added: minutesAgo(60 * 24 * 3), is_enabled: true },
  { id: "w4", original_text: "jon doe", replacement_text: "John Doe (fake)", date_added: minutesAgo(60 * 24), is_enabled: false },
];

type Snip = { id: string; trigger_text: string; content: string; date_added: string; is_enabled: boolean };
const snippets: Snip[] = [
  { id: "sn1", trigger_text: "my signature", content: "Best regards,\nAlex Example\nExample Co.", date_added: minutesAgo(60 * 24 * 4), is_enabled: true },
  { id: "sn2", trigger_text: "my address", content: "221B Baker Street\nLondon NW1 6XE", date_added: minutesAgo(60 * 24 * 2), is_enabled: true },
  { id: "sn3", trigger_text: "out of office", content: "I'm away until Monday and will reply when I'm back.", date_added: minutesAgo(60 * 24), is_enabled: false },
];

type Power = Record<string, unknown> & { id: string; name: string };
const powerConfigs: Power[] = [
  {
    id: "pm1", name: "Mail (example)", emoji: "✉️",
    app_triggers: [{ id: "pa1", exe_name: "com.apple.mail", app_name: "Mail" }],
    url_triggers: [], is_enhancement_enabled: true, use_screen_capture: false,
    selected_prompt_id: "mock-prompt-email", selected_llm_provider: null, selected_llm_model: null,
    transcription_kind: null, whisper_model_id: null, cloud_provider: null, cloud_model: null, parakeet_model_id: null,
    language: null, auto_send_key: "none", shortcut_slot: 1, is_enabled: true, is_default: false,
  },
  {
    id: "pm2", name: "Code chat (example)", emoji: "💻",
    app_triggers: [{ id: "pa2", exe_name: "com.microsoft.VSCode", app_name: "Visual Studio Code" }],
    url_triggers: [{ id: "pu1", url: "github.com" }], is_enhancement_enabled: false, use_screen_capture: null,
    selected_prompt_id: null, selected_llm_provider: null, selected_llm_model: null,
    transcription_kind: "local", whisper_model_id: "whisper-turbo", cloud_provider: null, cloud_model: null, parakeet_model_id: null,
    language: "en", auto_send_key: "enter", shortcut_slot: null, is_enabled: true, is_default: false,
  },
];
let activePower: { config_id: string; config_name: string; emoji: string } | null = null;

type Rec = {
  id: string; timestamp: string; status: "pending" | "completed" | "failed"; text: string; enhanced_text: string | null; edited_text: string | null;
  duration_sec: number | null; transcription_duration_sec: number | null; enhancement_duration_sec: number | null;
  audio_file_name: string | null; transcription_model_name: string | null; ai_enhancement_model_name: string | null;
  prompt_name: string | null; ai_request_system_message: string | null; ai_request_user_message: string | null;
  power_mode_name: string | null; power_mode_emoji: string | null; language: string | null;
};
function rec(minAgo: number, text: string, enhanced: string | null, dur: number, extra: Partial<Rec> = {}): Rec {
  return {
    id: uid(), timestamp: minutesAgo(minAgo), status: "completed", text, enhanced_text: enhanced, edited_text: null,
    duration_sec: dur, transcription_duration_sec: dur / 6, enhancement_duration_sec: enhanced ? 0.8 : null,
    audio_file_name: `mock-${Math.round(minAgo)}.wav`,
    transcription_model_name: "Whisper Large v3 Turbo",
    ai_enhancement_model_name: enhanced ? "IBM Granite 4.2 3B" : null,
    prompt_name: enhanced ? "Default" : null,
    ai_request_system_message: enhanced ? "(mock) system prompt" : null,
    ai_request_user_message: enhanced ? text : null,
    power_mode_name: null, power_mode_emoji: null, language: "en", ...extra,
  };
}
const history: Rec[] = [
  rec(4, "so um the quarterly numbers for the fake acme project look fine we should send them tomorrow", "The quarterly numbers for the fake Acme project look fine. We should send them tomorrow.", 9.4),
  rec(35, "reminder to pick up the sample package from the pretend post office", null, 4.1),
  rec(90, "hey john doe fake name here is a dummy status update everything on track", "Hi John Doe (fake), here is a dummy status update: everything is on track.", 7.8, { power_mode_name: "Mail (example)", power_mode_emoji: "✉️", prompt_name: "Polite email" }),
  rec(60 * 5, "this is a made up test dictation for the preview screen lorem ipsum dolor sit amet", "This is a made-up test dictation for the preview screen. Lorem ipsum dolor sit amet.", 12.2),
  rec(60 * 26, "", null, 2.0, { status: "failed", text: "", transcription_model_name: "Whisper Large v3 Turbo" }),
  rec(60 * 50, "ceci est une dictee fictive en francais pour tester l'apercu", null, 5.5, { language: "fr" }),
  rec(60 * 24 * 8, "another fake entry from last week with absolutely no real content in it", null, 6.3),
];

// -- Recording / pipeline simulation ----------------------------------------

let recording = false;
let meterT = 0;

async function runPipeline(job: "dictation" | "file" | "retry", text: string, durationMs = 1200) {
  await emit("pipeline:state", { state: "transcribing", message: null, text: null, duration_ms: null, job });
  await sleep(durationMs);
  if ((settings.enhancement_enabled as boolean) && job !== "file") {
    await emit("pipeline:state", { state: "enhancing", message: null, text: null, duration_ms: null, job });
    await sleep(800);
  }
  await emit("pipeline:state", { state: "pasting", message: null, text, duration_ms: durationMs, job });
  await emit("pipeline:state", { state: "done", message: null, text, duration_ms: durationMs, job });
}

async function finishDictation() {
  recording = false;
  await emit("recording:stopped", null);
  const text = "This is a mock dictation produced by the UI preview.";
  await runPipeline("dictation", text);
  const r = rec(0, text, text, 5.2);
  history.unshift(r);
  await emit("history:created", r.id);
}

async function simulateDownload(
  prefix: "model" | "parakeet_model" | "llm_model",
  list: Array<{ id: string; size_bytes: number; downloaded: boolean; on_disk_bytes: number | null; path: string | null }>,
  id: string,
) {
  const m = list.find((x) => x.id === id);
  if (!m) throw new Error("PARLA_ERR:unknownModel");
  cancelled.delete(id);
  const steps = 8;
  const kind = prefix === "model" ? "whisper" : prefix === "parakeet_model" ? "parakeet" : "llm";
  const active: ActiveDownload = { id, kind, name: (m as { display_name?: string }).display_name ?? id, downloaded: 0, total: m.size_bytes };
  activeDownloads.set(id, active);
  for (let i = 1; i <= steps; i++) {
    await sleep(350);
    active.downloaded = Math.round((m.size_bytes * (i - 1)) / steps);
    if (cancelled.has(id)) {
      cancelled.delete(id);
      activeDownloads.delete(id);
      await emit(`${prefix}:download:error`, { id, message: "PARLA_ERR:cancelled" });
      throw new Error("PARLA_ERR:cancelled");
    }
    const payload: Record<string, unknown> = { id, downloaded: Math.round((m.size_bytes * i) / steps), total: m.size_bytes };
    if (prefix === "parakeet_model") payload.current_file = "encoder.onnx";
    await emit(`${prefix}:download:progress`, payload);
  }
  activeDownloads.delete(id);
  markDownloaded(m as Parameters<typeof markDownloaded>[0]);
  await emit(`${prefix}:download:complete`, { id, path: m.path });
  return m.path;
}
const cancelled = new Set<string>();
type ActiveDownload = { id: string; kind: "whisper" | "parakeet" | "llm" | "vad"; name: string; downloaded: number; total: number };
/** Downloads running right now, for list_active_downloads. */
const activeDownloads = new Map<string, ActiveDownload>();

// -- Command table ----------------------------------------------------------

const noop: Handler = () => null;
const ret = (_v: unknown) => null;
const get = (key: string): Handler => () => settings[key];
const set = (key: string, arg: string): Handler => (a) => {
  settings[key] = a?.[arg];
  return null;
};
const pick = <T extends { id: string }>(list: T[], id: unknown) => list.find((x) => x.id === id);

function permissionStatus() {
  const bad = FLAGS.permMissing;
  return {
    microphone: bad
      ? { ok: false, label_key: "permissions.mac.micDenied", label_args: null, hint_key: "permissions.mac.micDeniedHint", diagnostic: null }
      : { ok: true, label_key: "permissions.mac.micAllowed", label_args: null, hint_key: null, diagnostic: null },
    ocr: bad
      ? { ok: false, label_key: "permissions.mac.screenMissing", label_args: null, hint_key: "permissions.mac.screenHint", diagnostic: null }
      : { ok: true, label_key: "permissions.mac.screenAllowed", label_args: null, hint_key: null, diagnostic: null },
    autostart: { ok: false, label_key: "permissions.autostart.disabled", label_args: null, hint_key: "permissions.autostart.disabledHint", diagnostic: null },
    hotkey: { ok: true, label_key: "permissions.hotkey.active", label_args: null, hint_key: null, diagnostic: null },
    accessibility: bad
      ? { ok: false, label_key: "permissions.mac.accessibilityMissing", label_args: null, hint_key: "permissions.mac.accessibilityHint", diagnostic: null }
      : { ok: true, label_key: "permissions.mac.accessibilityAllowed", label_args: null, hint_key: null, diagnostic: null },
  };
}

const commands: Record<string, Handler> = {
  ping: () => "pong (mock)",

  // Recording
  list_audio_devices: () => [
    { name: "MacBook Pro Microphone", is_default: true, default_sample_rate: 48000, default_channels: 1 },
    { name: "External USB Mic (mock)", is_default: false, default_sample_rate: 44100, default_channels: 2 },
  ],
  start_recording: () => {
    recording = true;
    return { recording_id: uid(), wav_path: "/mock/recording.wav" };
  },
  stop_recording: (a) => {
    recording = false;
    void emit("recording:stopped", null);
    if (a?.runPipeline) void finishDictation();
    return { wav_path: "/mock/recording.wav" };
  },
  cancel_recording: () => {
    recording = false;
    void emit("recording:cancelled", null);
    return null;
  },
  cancel_processing: () => {
    void emit("recording:cancelled", null);
    return null;
  },
  get_audio_meter: () => {
    if (!recording) return { rms_db: -160, peak_db: -160 };
    meterT += 0.35;
    const level = (Math.sin(meterT) + Math.sin(meterT * 2.7) * 0.5 + 1.5) / 3; // 0..1
    const rms = -52 + level * 38;
    return { rms_db: rms, peak_db: rms + 6 };
  },
  is_recording: () => recording,
  is_busy: () => recording,
  toggle_recording_from_ui: () => {
    if (recording) void finishDictation();
    else recording = true;
    return null;
  },
  take_recorder_notice: () => null,

  // Whisper
  list_whisper_models: () => whisperModels,
  history_available: () => true,
  download_whisper_model: (a) => simulateDownload("model", whisperModels, a?.id as string),
  cancel_download_whisper_model: (a) => {
    cancelled.add(a?.id as string);
    return null;
  },
  delete_whisper_model: (a) => {
    const m = pick(whisperModels, a?.id);
    if (m) Object.assign(m, { downloaded: false, on_disk_bytes: null, path: null });
    return null;
  },
  import_whisper_model: () => "whisper-imported-mock",
  remove_model_everywhere: (a) => {
    const id = a?.id;
    for (const list of [whisperModels, parakeetModels, ggufModels] as Array<Array<{ id: string; downloaded: boolean; on_disk_bytes: number | null; path: string | null }>>) {
      const m = pick(list, id);
      if (m) Object.assign(m, { downloaded: false, on_disk_bytes: null, path: null });
    }
    return "removed";
  },
  transcribe_wav: () => ({ text: "This is a mock transcription.", model_id: "whisper-turbo", duration_ms: 900 }),
  transcribe_file: () => {
    void runPipeline("file", "Mock transcript of the dropped file. Nothing was actually transcribed.", 1800);
    return { duration_sec: 42 };
  },
  set_selected_whisper_model: set("selected_whisper", "id"),
  get_selected_whisper_model: get("selected_whisper"),

  get_dictation_language: get("dictation_language"),
  set_dictation_language: (a) => {
    settings.dictation_language = a?.code;
    return null;
  },
  get_speech_threads: get("speech_threads"),
  set_speech_threads: (a) => {
    const s = settings.speech_threads as { value: number };
    s.value = a?.threads as number;
    return null;
  },
  get_use_gpu: () => ({ enabled: settings.use_gpu, default_reason: "mac", card_name: null, platform: "macos", failed: false }),
  set_use_gpu: (a) => {
    settings.use_gpu = a?.enabled;
    window.dispatchEvent(new Event("use-gpu-changed"));
    return null;
  },
  get_model_keep_alive: get("model_keep_alive"),
  set_model_keep_alive: (a) => {
    (settings.model_keep_alive as { value: number }).value = a?.secs as number;
    return null;
  },

  // Text processing + settings toggles
  get_text_processing_settings: get("text_processing"),
  set_text_formatting_enabled: (a) => ret(((settings.text_processing as Record<string, unknown>).text_formatting_enabled = a?.enabled)),
  set_spoken_commands: (a) => ret(((settings.text_processing as Record<string, unknown>).spoken_commands = a?.enabled)),
  set_spoken_punctuation: (a) => ret(((settings.text_processing as Record<string, unknown>).spoken_punctuation = a?.enabled)),
  set_review_before_paste: (a) => ret(((settings.text_processing as Record<string, unknown>).review_before_paste = a?.enabled)),
  review_text: () => "This is the dictated text.\nYou can fix it before it is pasted.",
  review_paste: () => null,
  review_cancel: () => null,
  set_remove_filler_words: (a) => ret(((settings.text_processing as Record<string, unknown>).remove_filler_words = a?.enabled)),
  set_filler_words: (a) => ret(((settings.text_processing as Record<string, unknown>).filler_words = a?.words)),
  set_append_trailing_space: (a) => ret(((settings.text_processing as Record<string, unknown>).append_trailing_space = a?.enabled)),
  set_restore_clipboard_after_paste: (a) => ret(((settings.text_processing as Record<string, unknown>).restore_clipboard_after_paste = a?.enabled)),
  get_close_to_tray: get("close_to_tray"),
  set_close_to_tray: set("close_to_tray", "enabled"),
  get_hide_dock_icon: get("hide_dock_icon"),
  set_hide_dock_icon: set("hide_dock_icon", "enabled"),
  get_system_mute_enabled: get("system_mute"),
  set_system_mute_enabled: set("system_mute", "enabled"),
  get_audio_resumption_delay: get("audio_resumption_delay"),
  set_audio_resumption_delay: set("audio_resumption_delay", "secs"),
  get_sound_feedback_enabled: get("sound_feedback"),
  set_sound_feedback_enabled: set("sound_feedback", "enabled"),
  show_main_window: (a) => {
    if (a?.panel) void emit("tray:navigate", a.panel);
    return null;
  },
  open_recorder_popover: () => {
    console.info("[ui-preview] open_recorder_popover (no second window in the browser; open #recorder-popover in another tab)");
    return null;
  },
  close_recorder_popover: noop,

  // Cloud speech
  list_cloud_providers: () =>
    CLOUD_PROVIDERS.map((p) => ({ ...p, requires_api_key: true, has_api_key: !!apiKeys[p.id] })),
  list_cloud_models: () =>
    CLOUD_MODELS.map(([provider_id, model_id, display_name, speed, accuracy]) => ({
      provider_id, model_id, display_name, supports_batch: true, supports_streaming: provider_id === "deepgram",
      multilingual: true, notes: "Mock cloud model.", speed, accuracy, language_codes: WHISPER_LANGS,
    })),
  set_api_key: (a) => {
    apiKeys[a?.provider as string] = true;
    return null;
  },
  delete_api_key: (a) => {
    delete apiKeys[a?.provider as string];
    return null;
  },
  has_api_key: (a) => !!apiKeys[a?.provider as string],
  verify_api_key: async () => {
    await sleep(600);
    return null;
  },
  verify_llm_api_key: async () => {
    await sleep(600);
    return null;
  },
  try_enhancement: async (a) => {
    await sleep(700);
    return `${String(a?.text ?? "").trim()} (cleaned up by the mock AI)`;
  },
  cloud_transcribe_wav: () => ({ text: "Mock cloud transcription.", provider: "groq", model: "whisper-large-v3-turbo", duration_ms: 700 }),

  get_transcription_source: get("source"),
  set_transcription_source: (a) => {
    settings.source = { whisper_model_id: null, cloud_provider: null, cloud_model: null, parakeet_model_id: null, ...(a?.source as object) };
    void emit("source:changed", settings.source);
    return null;
  },
  set_transcription_kind: (a) => {
    (settings.source as Record<string, unknown>).kind = a?.kind;
    void emit("source:changed", settings.source);
    return null;
  },

  // VAD
  vad_get_state: get("vad"),
  list_active_downloads: () => [...activeDownloads.values()],
  vad_download: async () => {
    const v = settings.vad as Record<string, unknown>;
    activeDownloads.set("vad", { id: "vad", kind: "vad", name: "Voice detection", downloaded: 0, total: 885_000 });
    for (let i = 1; i <= 5; i++) {
      await sleep(250);
      await emit("vad:download:progress", { downloaded: i * 177_000, total: 885_000 });
    }
    activeDownloads.delete("vad");
    Object.assign(v, { downloaded: true, path: "/mock/Models/vad/ggml-silero-v6.2.0.bin", on_disk_bytes: 885_000 });
    await emit("vad:download:complete", null);
    return v.path;
  },
  vad_delete: () => {
    Object.assign(settings.vad as object, { downloaded: false, path: null, on_disk_bytes: null });
    return null;
  },
  vad_is_enabled: get("vad_enabled"),
  vad_set_enabled: set("vad_enabled", "enabled"),

  // Dictionary
  list_word_replacements: () => words,
  add_word_replacement: (a) => {
    const p = a?.payload as { original_text: string; replacement_text: string; is_enabled?: boolean };
    const w: Word = { id: uid(), original_text: p.original_text, replacement_text: p.replacement_text, date_added: new Date().toISOString(), is_enabled: p.is_enabled ?? true };
    words.unshift(w);
    return w;
  },
  update_word_replacement: (a) => {
    const p = a?.payload as Partial<Word> & { id: string };
    const w = pick(words, p.id)!;
    Object.assign(w, Object.fromEntries(Object.entries(p).filter(([, v]) => v !== undefined)));
    return w;
  },
  delete_word_replacement: (a) => {
    const i = words.findIndex((w) => w.id === a?.id);
    if (i >= 0) words.splice(i, 1);
    return null;
  },
  export_dictionary_csv: () => "/mock/Downloads/parlato-dictionary.csv",
  // Pretends a file with 4 new rows and 3 duplicates was chosen.
  import_dictionary_csv: () => {
    const incoming = ["Kubernetes", "Acme Corp", "Zendesk", "Okta"];
    let added = 0;
    for (const w of incoming) {
      if (words.some((x) => x.original_text.toLowerCase() === w.toLowerCase())) continue;
      words.unshift({ id: uid(), original_text: w, replacement_text: w, date_added: new Date().toISOString(), is_enabled: true });
      added++;
    }
    return { added, skipped: 3 + (incoming.length - added) };
  },

  // Snippets
  list_snippets: () => snippets,
  add_snippet: (a) => {
    const p = a?.payload as { trigger_text: string; content: string; is_enabled?: boolean };
    const key = (s: string) => s.toLowerCase().replace(/[^\p{L}\p{N}]+/gu, " ").trim();
    if (!key(p.trigger_text) || !p.content.trim()) throw "PARLA_ERR:snippetInvalid";
    if (snippets.some((x) => key(x.trigger_text) === key(p.trigger_text))) throw "PARLA_ERR:snippetExists";
    const s = { id: uid(), trigger_text: p.trigger_text.trim(), content: p.content.trim(), date_added: new Date().toISOString(), is_enabled: p.is_enabled ?? true };
    snippets.push(s);
    return s;
  },
  update_snippet: (a) => {
    const p = a?.payload as { id: string; trigger_text?: string; content?: string; is_enabled?: boolean };
    const s = pick(snippets, p.id)!;
    if (p.trigger_text !== undefined) s.trigger_text = p.trigger_text.trim();
    if (p.content !== undefined) s.content = p.content.trim();
    if (p.is_enabled !== undefined) s.is_enabled = p.is_enabled;
    return s;
  },
  delete_snippet: (a) => {
    const i = snippets.findIndex((s) => s.id === a?.id);
    if (i >= 0) snippets.splice(i, 1);
    return null;
  },
  export_snippets_csv: () => "/mock/Downloads/parlato-snippets.csv",
  import_snippets_csv: () => {
    if (!snippets.some((s) => s.trigger_text === "meeting link")) {
      snippets.push({ id: uid(), trigger_text: "meeting link", content: "https://meet.example.com/parlato", date_added: new Date().toISOString(), is_enabled: true });
      return { added: 1, skipped: 2 };
    }
    return { added: 0, skipped: 3 };
  },

  // Enhancement
  get_enhancement_enabled: get("enhancement_enabled"),
  set_enhancement_enabled: set("enhancement_enabled", "enabled"),
  list_prompts: () => prompts,
  add_prompt: (a) => {
    const p = { ...(a?.prompt as Prompt), id: uid() };
    prompts.push(p);
    return p;
  },
  update_prompt: (a) => {
    const p = a?.prompt as Prompt;
    const i = prompts.findIndex((x) => x.id === p.id);
    if (i >= 0) prompts[i] = p;
    return null;
  },
  delete_prompt: (a) => {
    const i = prompts.findIndex((x) => x.id === a?.id);
    if (i >= 0) prompts.splice(i, 1);
    return null;
  },
  get_active_prompt_id: get("active_prompt_id"),
  set_active_prompt_id: set("active_prompt_id", "id"),
  list_extra_templates: () => extraTemplates,
  list_llm_providers: () =>
    LLM_PROVIDERS.map((p) => ({ ...p, has_api_key: p.id === "groq" ? !!apiKeys.groq : !!apiKeys[`llm:${p.id}`] })),
  get_llm_selection: get("llm_selection"),
  set_llm_selection: (a) => {
    settings.llm_selection = { provider_id: a?.providerId, model: a?.model };
    return null;
  },
  get_ollama_base_url: get("ollama_base_url"),
  set_ollama_base_url: set("ollama_base_url", "url"),
  list_ollama_models: () => ["llama3.2:3b (mock)", "qwen2.5:7b (mock)"],
  get_custom_base_url: get("custom_base_url"),
  set_custom_base_url: set("custom_base_url", "url"),
  get_localcli_custom_cmd: get("localcli_cmd"),
  set_localcli_custom_cmd: set("localcli_cmd", "cmd"),
  get_localcli_timeout_secs: get("localcli_timeout"),
  set_localcli_timeout_secs: set("localcli_timeout", "secs"),

  // Local AI (GGUF)
  list_gguf_models: () => ggufModels,
  download_gguf_model: (a) => simulateDownload("llm_model", ggufModels, a?.id as string),
  cancel_download_gguf_model: (a) => {
    cancelled.add(a?.id as string);
    return null;
  },
  delete_gguf_model: (a) => {
    const m = pick(ggufModels, a?.id);
    if (m) Object.assign(m, { downloaded: false, on_disk_bytes: null, path: null });
    return null;
  },
  import_gguf_model: () => null,
  get_selected_gguf: get("selected_gguf"),
  set_selected_gguf: set("selected_gguf", "id"),
  get_llamacpp_settings: get("llamacpp_settings"),
  set_llamacpp_settings: (a) => {
    settings.llamacpp_settings = { context_size: a?.contextSize ?? a?.context_size, max_tokens: a?.maxTokens ?? a?.max_tokens };
    return null;
  },
  llamacpp_backend: () => "metal",

  // Parakeet
  list_parakeet_models: () => parakeetModels,
  download_parakeet_model: (a) => simulateDownload("parakeet_model", parakeetModels, a?.id as string),
  cancel_download_parakeet_model: (a) => {
    cancelled.add(a?.id as string);
    return null;
  },
  delete_parakeet_model: (a) => {
    const m = pick(parakeetModels, a?.id);
    if (m) Object.assign(m, { downloaded: false, on_disk_bytes: null, path: null });
    return null;
  },
  parakeet_execution_provider: () => "coreml",

  // Power modes
  list_power_configs: () => powerConfigs,
  list_installed_apps: () => [
    { name: "Mail", exe_name: "com.apple.mail", running: true },
    { name: "Safari", exe_name: "com.apple.Safari", running: true },
    { name: "Notes", exe_name: "com.apple.Notes", running: false },
    { name: "Slack", exe_name: "com.tinyspeck.slackmacgap", running: true },
    { name: "Visual Studio Code", exe_name: "com.microsoft.VSCode", running: false },
  ],
  add_power_config: (a) => {
    const c = { ...(a?.config as Power), id: uid() };
    powerConfigs.push(c);
    return c;
  },
  update_power_config: (a) => {
    const c = a?.config as Power;
    const i = powerConfigs.findIndex((x) => x.id === c.id);
    if (i >= 0) powerConfigs[i] = c;
    return null;
  },
  delete_power_config: (a) => {
    const i = powerConfigs.findIndex((x) => x.id === a?.id);
    if (i >= 0) powerConfigs.splice(i, 1);
    return null;
  },
  reorder_power_configs: (a) => {
    const order = a?.orderedIds as string[];
    powerConfigs.sort((x, y) => order.indexOf(x.id) - order.indexOf(y.id));
    return null;
  },
  get_power_auto_restore: get("power_auto_restore"),
  set_power_auto_restore: set("power_auto_restore", "enabled"),
  select_power_config: (a) => {
    const c = pick(powerConfigs, a?.id);
    activePower = c ? { config_id: c.id, config_name: c.name, emoji: String(c.emoji) } : null;
    void emit("power_mode:active", activePower);
    return null;
  },
  get_active_power_session: () => activePower,
  power_mode_preview: () => ({
    active: { hwnd: 0, pid: 4242, title: "Inbox (mock)", exe_name: "com.apple.mail" },
    url: null,
    matched_config_id: "pm1",
    matched_config_name: "Mail (example)",
  }),

  // Screen context
  get_screen_context_enabled: get("screen_context"),
  set_screen_context_enabled: set("screen_context", "enabled"),
  get_screen_context_cached: () => null,
  clear_screen_context: noop,
  capture_screen_context_preview: () => "Mock screen text: Inbox, 3 unread messages, Draft to Jane Roe (fake).",

  // History + metrics
  list_history: (a) => {
    let rows = history.slice();
    const search = (a?.search as string | null)?.toLowerCase();
    if (search) rows = rows.filter((r) => (r.text + (r.enhanced_text ?? "")).toLowerCase().includes(search));
    if (a?.filter === "ai") rows = rows.filter((r) => r.enhanced_text);
    if (a?.filter === "failed") rows = rows.filter((r) => r.status === "failed");
    return rows.slice(0, (a?.limit as number | null) ?? 50);
  },
  get_history_item: (a) => pick(history, a?.id) ?? null,
  delete_history_item: (a) => {
    const i = history.findIndex((r) => r.id === a?.id);
    if (i >= 0) history.splice(i, 1);
    void emit("history:updated", a?.id);
    return null;
  },
  read_history_audio: () => {
    // 0.5 s of silence as a 16 kHz mono 16-bit WAV so the player has something to load.
    const samples = 8000;
    const buf = new ArrayBuffer(44 + samples * 2);
    const v = new DataView(buf);
    const w = (o: number, s: string) => [...s].forEach((c, i) => v.setUint8(o + i, c.charCodeAt(0)));
    w(0, "RIFF"); v.setUint32(4, 36 + samples * 2, true); w(8, "WAVE"); w(12, "fmt ");
    v.setUint32(16, 16, true); v.setUint16(20, 1, true); v.setUint16(22, 1, true);
    v.setUint32(24, 16000, true); v.setUint32(28, 32000, true); v.setUint16(32, 2, true); v.setUint16(34, 16, true);
    w(36, "data"); v.setUint32(40, samples * 2, true);
    return buf;
  },
  retranscribe_history_item: () => {
    void runPipeline("retry", "Retried mock transcription.", 900);
    return null;
  },
  count_history: () => history.length,
  get_usage_stats: () => ({ sessions: 412, words: 18_640, chars: 104_200, duration_sec: 5_870 }),
  export_history_csv: () => "/mock/Downloads/parlato-history.csv",
  // Home chart: realistic words per day, today last (a few quiet days, a weekend dip).
  get_usage_by_day: (a) => {
    const days = (a?.days as number | null) ?? 14;
    const pattern = [420, 610, 380, 0, 0, 540, 720, 660, 810, 295, 0, 120, 480, 355];
    return Array.from({ length: days }, (_, i) => {
      const back = days - 1 - i;
      const words = pattern[back % pattern.length];
      return { date: isoDay(back), words, sessions: words === 0 ? 0 : Math.max(1, Math.round(words / 55)) };
    });
  },
  get_usage_by_week: (a) => {
    const weeks = (a?.weeks as number | null) ?? 12;
    const pattern = [2900, 3400, 2100, 3800, 4200, 1500, 3100, 3600, 4500, 2700, 3900, 2250];
    const monday = new Date();
    monday.setDate(monday.getDate() - ((monday.getDay() + 6) % 7));
    return Array.from({ length: weeks }, (_, i) => {
      const d = new Date(monday);
      d.setDate(d.getDate() - 7 * (weeks - 1 - i));
      const words = pattern[(weeks - 1 - i) % pattern.length];
      return { date: localDay(d), words, sessions: Math.round(words / 55) };
    });
  },
  // Saves the corrected text on the mock row and suggests capitalised words
  // that are new (the real command compares word by word in Rust).
  update_history_text: (a) => {
    const r = pick(history, a?.id);
    if (!r) throw new Error("PARLA_ERR:historyItemMissing");
    const text = String(a?.text ?? "").trim();
    if (!text) throw new Error("PARLA_ERR:historyTextEmpty");
    const before = (r.edited_text ?? r.enhanced_text ?? r.text).toLowerCase();
    r.edited_text = text === (r.enhanced_text ?? r.text) ? null : text;
    const known = new Set(words.flatMap((w) => [w.original_text.toLowerCase(), w.replacement_text.toLowerCase()]));
    const suggestions = [...new Set(text.split(/\s+/).map((w) => w.replace(/^[^\p{L}\p{N}]+|[^\p{L}\p{N}]+$/gu, "")))]
      .filter((w) => /^\p{Lu}/u.test(w) && w.length > 2 && !before.includes(w.toLowerCase()) && !known.has(w.toLowerCase()))
      .slice(0, 5);
    void emit("history:updated", r.id);
    return { record: r, suggestions };
  },
  get_model_performance_metrics: () => ({
    transcription_models: [
      { name: "Whisper Large v3 Turbo", session_count: 310, total_audio_sec: 4200, total_processing_sec: 700 },
      { name: "Parakeet v3", session_count: 102, total_audio_sec: 1670, total_processing_sec: 150 },
    ],
    enhancement_models: [{ name: "IBM Granite 4.2 3B", session_count: 280, total_duration_sec: 224 }],
  }),
  get_retention_settings: get("retention"),
  set_retention_settings: set("retention", "settings"),
  run_history_cleanup: () => {
    void emit("history:cleaned", null);
    return null;
  },

  // Permissions + misc
  check_permissions: permissionStatus,
  set_autostart_enabled: (a) => {
    void emit("settings:autostart-changed", a?.enabled);
    return null;
  },
  open_privacy_microphone: () => ret(console.info("[ui-preview] open_privacy_microphone")),
  open_log_folder: () => ret(console.info("[ui-preview] open_log_folder")),
  delete_all_data: () => ret(console.info("[ui-preview] delete_all_data (mock: nothing deleted)")),
  request_microphone_access: () => ret(console.info("[ui-preview] request_microphone_access")),
  request_accessibility_access: () => !FLAGS.permMissing,
  open_language_settings: () => ret(console.info("[ui-preview] open_language_settings")),
  request_screen_recording_access: () => !FLAGS.permMissing,
  get_browser_automation_status: () => ({ denied_browser: FLAGS.permMissing ? "Safari" : null }),
  open_automation_settings: () => ret(console.info("[ui-preview] open_automation_settings")),

  get_recorder_style: get("recorder_style"),
  set_recorder_style: set("recorder_style", "style"),
  get_onboarding_completed: get("onboarding_completed"),
  set_onboarding_completed: set("onboarding_completed", "completed"),

  // Hotkeys
  get_hotkey_config: get("hotkeys"),
  set_hotkey_config: set("hotkeys", "config"),
  reset_hotkey_config: () => {
    const actions = (settings.hotkeys as { actions: unknown }).actions;
    settings.hotkeys = { ...JSON.parse(JSON.stringify(DEFAULT_HOTKEYS)), actions };
    return settings.hotkeys;
  },
  list_hotkey_options: () => [
    { id: "none", vk: null },
    { id: "rightAlt", vk: 0xa5 },
    { id: "leftAlt", vk: 0xa4 },
    { id: "rightCtrl", vk: 0xa3 },
    { id: "leftCtrl", vk: 0xa2 },
    { id: "rightWin", vk: 0x5c },
    { id: "rightShift", vk: 0xa1 },
    { id: "leftShift", vk: 0xa0 },
    { id: "leftWin", vk: 0x5b },
    { id: "capsLock", vk: 0x14 },
    { id: "menu", vk: 0x5d },
    { id: "fn", vk: null },
  ],
  reset_escape_hint: noop,

  get_cloud_transcription_timeout: get("cloud_timeout"),
  set_cloud_transcription_timeout: set("cloud_timeout", "secs"),
  get_selected_input_device: get("input_device"),
  set_selected_input_device: set("input_device", "name"),
  get_ui_language: get("ui_language"),
  set_ui_language: set("ui_language", "language"),
};

// -- Plugin commands --------------------------------------------------------

const storeData = new Map<string, unknown>();
const pluginCommands: Record<string, Handler> = {
  "plugin:app|version": () => "0.15.1",
  "plugin:app|name": () => "Parlato",
  "plugin:app|tauri_version": () => "2.10.1",
  "plugin:app|identifier": () => "com.example.parlato.preview",

  "plugin:store|load": () => 1,
  "plugin:store|get_store": () => 1,
  "plugin:store|set": (a) => ret(storeData.set(a?.key as string, a?.value)),
  "plugin:store|get": (a) => [storeData.get(a?.key as string) ?? null, storeData.has(a?.key as string)],
  "plugin:store|has": (a) => storeData.has(a?.key as string),
  "plugin:store|delete": (a) => storeData.delete(a?.key as string),
  "plugin:store|clear": () => ret(storeData.clear()),
  "plugin:store|reset": () => ret(storeData.clear()),
  "plugin:store|keys": () => [...storeData.keys()],
  "plugin:store|values": () => [...storeData.values()],
  "plugin:store|entries": () => [...storeData.entries()],
  "plugin:store|length": () => storeData.size,
  "plugin:store|reload": noop,
  "plugin:store|save": noop,

  "plugin:updater|check": () => null, // no update available
  "plugin:dialog|ask": () => true,
  "plugin:dialog|confirm": () => true,
  "plugin:dialog|message": noop,
  "plugin:dialog|open": () => null,
  "plugin:dialog|save": () => null,

  "plugin:opener|open_url": (a) => ret(console.info("[ui-preview] open_url", a?.url)),
  "plugin:opener|open_path": (a) => ret(console.info("[ui-preview] open_path", a?.path)),
  "plugin:opener|reveal_item_in_dir": (a) => ret(console.info("[ui-preview] reveal", a?.paths)),

  "plugin:autostart|is_enabled": () => false,
  "plugin:autostart|enable": noop,
  "plugin:autostart|disable": noop,

  "plugin:process|restart": () => ret(console.info("[ui-preview] relaunch ignored")),
  "plugin:process|exit": noop,

  "plugin:global-shortcut|register": noop,
  "plugin:global-shortcut|unregister": noop,
  "plugin:global-shortcut|unregister_all": noop,
  "plugin:global-shortcut|is_registered": () => false,
};

/** Number of app commands mocked (excludes plugin:* commands). */
export const MOCKED_COMMAND_COUNT = Object.keys(commands).length;

export function handle(cmd: string, args?: unknown): unknown {
  const a = args as Args;
  const h = commands[cmd] ?? pluginCommands[cmd];
  if (h) return h(a);
  // Window / webview plugin calls (setTheme, drag-drop, size...) are harmless in a browser.
  if (cmd.startsWith("plugin:window|") || cmd.startsWith("plugin:webview|")) return null;
  console.warn("[ui-preview] unmocked command", cmd, args);
  return null;
}
