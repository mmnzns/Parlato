// Provider llama.cpp embarque.
//
// Backend : crate `llama-cpp-2` (ffi llama.cpp).
// CUDA : active via la feature Parla `cuda-llama` qui propage
// `llama-cpp-2/cuda`. Quand active et qu'un GPU NVIDIA est detecte, on
// pousse toutes les layers sur GPU (n_gpu_layers=1000).
//
// Prompt (Parlato) : chat template du GGUF, sans thinking, voir build_prompt.
//
// Generation : non-streamee (parity avec les autres providers). Le modele
// est maintenu charge en memoire dans un Arc<Mutex<...>> (singleton cote
// service) et rechargement paresseux si l'ID selectionne change.

use std::num::NonZeroU32;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::{anyhow, Result};
use async_trait::async_trait;
use parking_lot::Mutex;
use tauri_plugin_store::StoreExt;
use tracing::{info, warn};

use llama_cpp_2::context::params::LlamaContextParams;
use llama_cpp_2::llama_backend::LlamaBackend;
use llama_cpp_2::llama_batch::LlamaBatch;
use llama_cpp_2::model::params::LlamaModelParams;
use llama_cpp_2::model::{AddBos, LlamaChatMessage, LlamaModel};
use llama_cpp_2::sampling::LlamaSampler;
use llama_cpp_2::token::LlamaToken;

use crate::enhancement::provider::{
    EnhancementRequest, EnhancementResponse, LLMProvider,
};

const STORE_FILE: &str = "parla.settings.json";
const KEY_SELECTED_GGUF: &str = "llm_selected_gguf";
const KEY_N_GPU_LAYERS: &str = "llm_gguf_n_gpu_layers";
const KEY_MAX_TOKENS: &str = "llm_gguf_max_tokens";
const KEY_CONTEXT_SIZE: &str = "llm_gguf_context_size";

const DEFAULT_MAX_TOKENS: u32 = 1024;
const DEFAULT_CONTEXT_SIZE: u32 = 4096;

#[cfg(feature = "cuda-llama")]
const DEFAULT_GPU_LAYERS: u32 = 1000;
#[cfg(not(feature = "cuda-llama"))]
const DEFAULT_GPU_LAYERS: u32 = 0;

// -- Settings helpers -------------------------------------------------------

pub fn get_selected_gguf(app: &tauri::AppHandle) -> Option<String> {
    app.store(STORE_FILE)
        .ok()
        .and_then(|s| {
            s.get(KEY_SELECTED_GGUF)
                .and_then(|v| v.as_str().map(String::from))
        })
        .filter(|s| !s.is_empty())
}

pub fn set_selected_gguf(app: &tauri::AppHandle, id: Option<&str>) -> Result<()> {
    let store = app.store(STORE_FILE).map_err(|e| anyhow!("store: {e}"))?;
    match id {
        Some(i) => store.set(KEY_SELECTED_GGUF, serde_json::Value::String(i.into())),
        None => {
            store.delete(KEY_SELECTED_GGUF);
        }
    }
    store.save().map_err(|e| anyhow!("store save: {e}"))
}

fn get_u32(app: &tauri::AppHandle, key: &str, default: u32) -> u32 {
    app.store(STORE_FILE)
        .ok()
        .and_then(|s| s.get(key).and_then(|v| v.as_u64()))
        .map(|v| v as u32)
        .unwrap_or(default)
}

pub fn get_max_tokens(app: &tauri::AppHandle) -> u32 {
    get_u32(app, KEY_MAX_TOKENS, DEFAULT_MAX_TOKENS).max(32)
}

pub fn set_max_tokens(app: &tauri::AppHandle, v: u32) -> Result<()> {
    let store = app.store(STORE_FILE).map_err(|e| anyhow!("store: {e}"))?;
    store.set(
        KEY_MAX_TOKENS,
        serde_json::Value::Number(serde_json::Number::from(v.max(32))),
    );
    store.save().map_err(|e| anyhow!("store save: {e}"))
}

pub fn get_context_size(app: &tauri::AppHandle) -> u32 {
    get_u32(app, KEY_CONTEXT_SIZE, DEFAULT_CONTEXT_SIZE).max(512)
}

pub fn set_context_size(app: &tauri::AppHandle, v: u32) -> Result<()> {
    let store = app.store(STORE_FILE).map_err(|e| anyhow!("store: {e}"))?;
    store.set(
        KEY_CONTEXT_SIZE,
        serde_json::Value::Number(serde_json::Number::from(v.max(512))),
    );
    store.save().map_err(|e| anyhow!("store save: {e}"))
}

pub fn get_n_gpu_layers(app: &tauri::AppHandle) -> u32 {
    get_u32(app, KEY_N_GPU_LAYERS, DEFAULT_GPU_LAYERS)
}

pub fn set_n_gpu_layers(app: &tauri::AppHandle, v: u32) -> Result<()> {
    let store = app.store(STORE_FILE).map_err(|e| anyhow!("store: {e}"))?;
    store.set(
        KEY_N_GPU_LAYERS,
        serde_json::Value::Number(serde_json::Number::from(v)),
    );
    store.save().map_err(|e| anyhow!("store save: {e}"))
}

// -- Runtime singleton ------------------------------------------------------

/// Singleton runtime llama.cpp : backend + modele charge.
/// Le backend doit etre unique dans le process (cf llama.cpp ggml_init).
struct LoadedModel {
    path: PathBuf,
    n_gpu_layers: u32,
    context_size: u32,
    model: LlamaModel,
}

pub struct LlamaRuntime {
    backend: Arc<LlamaBackend>,
    current: Mutex<Option<LoadedModel>>,
}

impl LlamaRuntime {
    pub fn new() -> Result<Self> {
        let backend = LlamaBackend::init().map_err(|e| anyhow!("llama backend init: {e}"))?;
        Ok(Self {
            backend: Arc::new(backend),
            current: Mutex::new(None),
        })
    }

    /// Charge ou recharge le modele si necessaire. Bloquant (IO + mmap).
    fn ensure_loaded(&self, path: &Path, n_gpu_layers: u32, context_size: u32) -> Result<()> {
        let mut guard = self.current.lock();
        if let Some(cur) = guard.as_ref() {
            if cur.path == path && cur.n_gpu_layers == n_gpu_layers && cur.context_size == context_size {
                return Ok(());
            }
        }
        // Drop l'ancien avant de charger (libere la VRAM / RAM).
        *guard = None;

        let params = LlamaModelParams::default().with_n_gpu_layers(n_gpu_layers);
        info!(
            path = %path.display(),
            n_gpu_layers,
            context_size,
            "Chargement modele GGUF"
        );
        let model = LlamaModel::load_from_file(&self.backend, path, &params)
            .map_err(|e| anyhow!("llama load: {e}"))?;
        *guard = Some(LoadedModel {
            path: path.to_path_buf(),
            n_gpu_layers,
            context_size,
            model,
        });
        Ok(())
    }

    /// Genere la reponse a un couple system / user. Bloquant (CPU/GPU intensif).
    pub fn generate(
        &self,
        path: &Path,
        n_gpu_layers: u32,
        context_size: u32,
        system: &str,
        user: &str,
        max_tokens: u32,
    ) -> Result<String> {
        self.ensure_loaded(path, n_gpu_layers, context_size)?;
        let guard = self.current.lock();
        let loaded = guard.as_ref().ok_or_else(|| anyhow!("modele non charge"))?;
        let model = &loaded.model;

        let prompt = build_prompt(model, system, user);

        let ctx_params = LlamaContextParams::default().with_n_ctx(NonZeroU32::new(context_size));
        let mut ctx = model
            .new_context(&self.backend, ctx_params)
            .map_err(|e| anyhow!("llama context: {e}"))?;

        let mut tokens_list = model
            .str_to_token(&prompt, AddBos::Always)
            .map_err(|e| anyhow!("tokenize: {e}"))?;
        // Parlato : certains templates ecrivent deja le BOS dans le texte ;
        // AddBos::Always en ajoute alors un second, qu'on retire.
        let bos = model.token_bos();
        if tokens_list.len() > 1 && tokens_list[0] == bos && tokens_list[1] == bos {
            tokens_list.remove(0);
        }

        if tokens_list.is_empty() {
            return Err(anyhow!("prompt vide apres tokenization"));
        }

        if (tokens_list.len() as u32) + max_tokens > context_size {
            warn!(
                prompt_tokens = tokens_list.len(),
                max_tokens,
                context_size,
                "Prompt + generation risquent de depasser le contexte"
            );
        }

        let batch_capacity = tokens_list.len().max(512);
        let mut batch = LlamaBatch::new(batch_capacity, 1);
        let last_index = tokens_list.len() - 1;
        for (i, token) in tokens_list.iter().enumerate() {
            batch
                .add(*token, i as i32, &[0], i == last_index)
                .map_err(|e| anyhow!("batch add prompt: {e}"))?;
        }
        ctx.decode(&mut batch).map_err(|e| anyhow!("decode prompt: {e}"))?;

        // Greedy sampling : on veut un output deterministe pour l'enhancement.
        let mut sampler = LlamaSampler::greedy();

        let mut generated: Vec<LlamaToken> = Vec::new();
        let mut n_cur = tokens_list.len() as i32;
        let n_max = n_cur + max_tokens as i32;

        while n_cur < n_max {
            let token = sampler.sample(&ctx, batch.n_tokens() - 1);
            sampler.accept(token);
            if model.is_eog_token(token) {
                break;
            }
            generated.push(token);

            batch.clear();
            batch
                .add(token, n_cur, &[0], true)
                .map_err(|e| anyhow!("batch add tok: {e}"))?;
            ctx.decode(&mut batch).map_err(|e| anyhow!("decode step: {e}"))?;
            n_cur += 1;
        }

        // Detokenize : octets de chaque token concatenes puis decodes en une
        // fois, pour ne pas couper les sequences multi-byte UTF-8.
        // Parlato : token_to_bytes agrandit le buffer si un token depasse
        // 8 octets (tokens_to_str echouait en "Insufficient Buffer Space"),
        // et un token sans texte (UnknownTokenType) est simplement ignore.
        let mut bytes: Vec<u8> = Vec::new();
        for token in &generated {
            match model.token_to_bytes(*token, llama_cpp_2::model::Special::Plaintext) {
                Ok(b) => bytes.extend_from_slice(&b),
                Err(llama_cpp_2::TokenToStringError::UnknownTokenType) => {}
                Err(e) => return Err(anyhow!("detokenize: {e}")),
            }
        }
        let output = String::from_utf8_lossy(&bytes).into_owned();

        Ok(clean_output(&output))
    }

    /// Libere le modele charge (utile quand l'utilisateur change de GGUF
    /// ou desactive l'enhancement).
    pub fn unload(&self) {
        let mut guard = self.current.lock();
        *guard = None;
    }
}

// -- Prompt -----------------------------------------------------------------
//
// Parlato : le prompt suit le chat template embarque dans le GGUF, rendu par
// le moteur de templates integre de llama.cpp (reconnait ChatML/Qwen, Gemma,
// Granite, Llama 3, Phi...). Repli sur le format generique historique de
// Parla si le GGUF n'a pas de template ou si le motif n'est pas reconnu.
//
// Thinking (Qwen 3 / 3.5) : le moteur integre n'applique pas l'option
// enable_thinking des templates Jinja. On reproduit ce que fait le template
// Qwen avec enable_thinking = false : un bloc <think> vide pre-rempli apres
// l'ouverture de la reponse. clean_output retire tout reste de reflexion.

/// Format generique historique (repli si le modele n'a pas de template).
fn generic_prompt(system: &str, user: &str) -> String {
    format!("<|system|>\n{system}\n<|user|>\n{user}\n<|assistant|>\n")
}

/// Bloc de reflexion vide : "pas de thinking" pour les modeles Qwen.
const EMPTY_THINK: &str = "<think>\n\n</think>\n\n";

fn build_prompt(model: &LlamaModel, system: &str, user: &str) -> String {
    let system = system.trim();
    let user = user.trim();
    let Ok(tmpl) = model.chat_template(None) else {
        return generic_prompt(system, user);
    };
    let chat = [
        LlamaChatMessage::new("system".into(), system.into()),
        LlamaChatMessage::new("user".into(), user.into()),
    ];
    let [Ok(s), Ok(u)] = chat else {
        return generic_prompt(system, user);
    };
    match model.apply_chat_template(&tmpl, &[s, u], true) {
        Ok(mut p) => {
            let thinks = tmpl.to_str().map(|t| t.contains("<think>")).unwrap_or(false);
            if thinks && !p.trim_end().ends_with("</think>") {
                p.push_str(EMPTY_THINK);
            }
            p
        }
        Err(e) => {
            warn!(error = %e, "chat template non reconnu, format generique");
            generic_prompt(system, user)
        }
    }
}

/// Retire les blocs de reflexion qu'un modele "thinking" ecrirait malgre
/// le bloc vide pre-rempli.
fn clean_output(raw: &str) -> String {
    let mut out = raw.to_string();
    while let Some(start) = out.find("<think>") {
        match out[start..].find("</think>") {
            Some(len) => out.replace_range(start..start + len + "</think>".len(), ""),
            None => out.truncate(start),
        }
    }
    // Un template peut ouvrir <think> dans le prompt : la sortie commence
    // alors directement par la reflexion et ne contient que la fermeture.
    if let Some(end) = out.find("</think>") {
        out.replace_range(..end + "</think>".len(), "");
    }
    out.trim().to_string()
}

// -- Provider ---------------------------------------------------------------

pub struct LlamaCppProvider;

#[async_trait]
impl LLMProvider for LlamaCppProvider {
    fn id(&self) -> &'static str {
        "llamacpp"
    }
    fn label(&self) -> &'static str {
        "llama.cpp (local)"
    }
    fn default_models(&self) -> &'static [&'static str] {
        // Les modeles dispos sont telecharges via le GGUF model manager,
        // expose cote UI.
        &[]
    }
    fn default_model(&self) -> &'static str {
        ""
    }
    fn endpoint(&self) -> &'static str {
        "embedded: llama.cpp"
    }
    fn requires_api_key(&self) -> bool {
        false
    }
    fn rate_limited(&self) -> bool {
        false
    }

    async fn chat_completion(
        &self,
        _api_key: &str,
        req: &EnhancementRequest,
    ) -> Result<EnhancementResponse> {
        // endpoint_override porte le chemin GGUF + config packee JSON par le
        // service (pragmatique : un seul champ libre dans le trait).
        // Format : `{gguf_path}|{n_gpu_layers}|{context_size}|{max_tokens}`
        let config = req
            .endpoint_override
            .as_deref()
            .ok_or_else(|| anyhow!("llama.cpp: aucun modele GGUF selectionne"))?;
        let parts: Vec<&str> = config.splitn(4, '|').collect();
        if parts.len() != 4 {
            return Err(anyhow!("llama.cpp: config invalide"));
        }
        let path = PathBuf::from(parts[0]);
        let n_gpu_layers: u32 = parts[1].parse().unwrap_or(DEFAULT_GPU_LAYERS);
        let context_size: u32 = parts[2].parse().unwrap_or(DEFAULT_CONTEXT_SIZE);
        let max_tokens: u32 = parts[3].parse().unwrap_or(DEFAULT_MAX_TOKENS);

        // Prompt : rendu par build_prompt avec le chat template du modele.

        // Inference bloquante : spawn_blocking pour ne pas monopoliser le
        // runtime async Tokio.
        let runtime = crate::enhancement::service::llama_runtime()
            .ok_or_else(|| anyhow!("llama runtime absent"))?;
        let system = req.system_prompt.clone();
        let user = req.user_message.clone();
        let path_owned = path;
        tokio::task::spawn_blocking(move || {
            runtime.generate(&path_owned, n_gpu_layers, context_size, &system, &user, max_tokens)
        })
        .await
        .map_err(|e| anyhow!("join: {e}"))?
        .map(|text| EnhancementResponse { text })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clean_output_strips_think_blocks() {
        let raw = "<think>\nplanning...\n</think>\n\nHello, world.";
        assert_eq!(clean_output(raw), "Hello, world.");
    }

    #[test]
    fn clean_output_strips_leading_reasoning_without_open_tag() {
        let raw = "the user wants a fix</think>Fixed text.";
        assert_eq!(clean_output(raw), "Fixed text.");
    }

    #[test]
    fn clean_output_drops_unclosed_think() {
        assert_eq!(clean_output("Done.<think>more"), "Done.");
    }

    #[test]
    fn clean_output_keeps_plain_text() {
        assert_eq!(clean_output("  Plain text.\n"), "Plain text.");
    }

    /// Smoke test against a real GGUF : `PARLATO_GGUF=<path> cargo test
    /// --lib llamacpp_smoke -- --ignored --nocapture`.
    #[test]
    #[ignore]
    fn llamacpp_smoke() {
        let path = std::env::var("PARLATO_GGUF").expect("PARLATO_GGUF");
        let rt = LlamaRuntime::new().unwrap();
        let system = "You clean up dictated text. Fix grammar and punctuation, remove filler words. Output only the cleaned text.";
        let user = "um so i was thinking we could uh meet on tuesday at like three pm to go over the the budget";
        let t0 = std::time::Instant::now();
        let out = rt.generate(Path::new(&path), 0, 4096, system, user, 256).unwrap();
        println!("--- {:.1}s ---\n{out}\n---", t0.elapsed().as_secs_f32());
        assert!(!out.is_empty());
        assert!(!out.contains("<think>") && !out.contains("</think>"));
    }
}
