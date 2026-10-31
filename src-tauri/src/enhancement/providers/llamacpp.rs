// Provider llama.cpp embarque.
//
// Le modele tourne dans l'assistant IA du moteur (voice-engine v0.2.2), pas
// dans ce processus : voir enhancement/local_ai.rs (client, cycle de vie du
// modele, carte graphique). Ce fichier garde les reglages (modele choisi,
// contexte, tokens max), `clean_output` (filet de securite sur la reponse) et
// le fournisseur du trait LLMProvider.
//
// Prompt : l'assistant applique le chat template du GGUF (thinking coupe).
//
// Generation : non-streamee (parite avec les autres providers).

use anyhow::{anyhow, Result};
use async_trait::async_trait;
use tauri_plugin_store::StoreExt;

use crate::enhancement::provider::{
    EnhancementRequest, EnhancementResponse, LLMProvider,
};

const STORE_FILE: &str = "parla.settings.json";
const KEY_SELECTED_GGUF: &str = "llm_selected_gguf";
const KEY_MAX_TOKENS: &str = "llm_gguf_max_tokens";
const KEY_CONTEXT_SIZE: &str = "llm_gguf_context_size";

const DEFAULT_MAX_TOKENS: u32 = 1024;
const DEFAULT_CONTEXT_SIZE: u32 = 4096;

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

// -- Reponse ----------------------------------------------------------------

/// Retire les blocs de reflexion qu'un modele "thinking" ecrirait malgre
/// le bloc vide pre-rempli.
pub fn clean_output(raw: &str) -> String {
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

/// La config que le service passe par `endpoint_override` (un seul champ
/// libre dans le trait) : `{id du modele}|{contexte}|{tokens max}`.
pub fn pack_config(id: &str, context_size: u32, max_tokens: u32) -> String {
    format!("{id}|{context_size}|{max_tokens}")
}

fn unpack_config(config: &str) -> Option<(&str, u32, u32)> {
    let mut parts = config.rsplitn(3, '|');
    let max_tokens = parts.next()?.parse().ok()?;
    let context_size = parts.next()?.parse().ok()?;
    let id = parts.next().filter(|i| !i.is_empty())?;
    Some((id, context_size, max_tokens))
}

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
        let config = req
            .endpoint_override
            .as_deref()
            .ok_or_else(|| anyhow!("llama.cpp: aucun modele GGUF selectionne"))?;
        let (id, context_size, max_tokens) =
            unpack_config(config).ok_or_else(|| anyhow!("llama.cpp: config invalide"))?;
        let ai = crate::enhancement::local_ai::local_ai()
            .ok_or_else(|| anyhow!("llama.cpp: l'assistant IA n'est pas demarre"))?;
        let model = ai.resolve(id)?;
        let system = req.system_prompt.clone();
        let user = req.user_message.clone();
        // Requete bloquante : spawn_blocking pour ne pas monopoliser le
        // runtime async Tokio.
        tokio::task::spawn_blocking(move || {
            ai.complete(&model, context_size, max_tokens, &system, &user)
        })
        .await
        .map_err(|e| anyhow!("join: {e}"))?
        .map(|text| EnhancementResponse { text })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use crate::enhancement::local_ai::{default_config, LocalAi, ModelRef};
    use crate::enhancement::prompts;

    #[test]
    fn config_round_trips_even_with_a_bar_in_the_id() {
        let packed = pack_config("granite-4.2-3b", 4096, 1024);
        assert_eq!(unpack_config(&packed), Some(("granite-4.2-3b", 4096, 1024)));
        assert_eq!(unpack_config("a|b|c"), None);
        assert_eq!(unpack_config("|1|2"), None);
        assert_eq!(unpack_config("x"), None);
    }

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

    /// Smoke test against a real GGUF and the real helper, in release:
    /// `PARLATO_GGUF=<file> [PARLATO_LLM_HELPER=<helper>] cargo test --release
    /// --lib llamacpp_smoke -- --ignored --nocapture`. Parlato's default
    /// cleanup prompt, a 50-word dictation, 3 runs. Prints counts and times
    /// only, never the prompt or the reply.
    #[test]
    #[ignore]
    fn llamacpp_smoke() {
        let path = PathBuf::from(std::env::var("PARLATO_GGUF").expect("PARLATO_GGUF"));
        let helper = std::env::var_os("PARLATO_LLM_HELPER")
            .map(PathBuf::from)
            .unwrap_or_else(|| {
                PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                    .join("binaries")
                    .join("parlato-llm-aarch64-apple-darwin")
            });
        let mut config = default_config();
        config.cpu_helper = helper;
        let ai = LocalAi::new(config, None);

        let system = prompts::predefined_seed()
            .into_iter()
            .next()
            .expect("the Default prompt")
            .final_prompt_text();
        let dictation = "um so i think we should uh move the meeting to thursday because like john is travelling and uh sarah cant make it on wednesday also can you send the budget numbers to the team before friday and uh make sure the hiring plan is in there you know okay";
        assert_eq!(dictation.split_whitespace().count(), 50);
        let user = crate::enhancement::service::build_user_message(dictation);

        let model = ModelRef { id: "smoke".into(), path };
        let t0 = std::time::Instant::now();
        let load_s = ai.ensure_loaded(&model, 4096).expect("load");
        println!(
            "load: {load_s:.2}s reported, {:.2}s wall, backend {}",
            t0.elapsed().as_secs_f32(),
            ai.backend()
        );
        for run in 1..=3 {
            let t = std::time::Instant::now();
            let out = ai.complete(&model, 4096, 1024, &system, &user).expect("complete");
            println!(
                "run {run}: reply {:.2}s, {} words, backend {}, fell_back {}",
                t.elapsed().as_secs_f32(),
                out.split_whitespace().count(),
                ai.backend(),
                ai.last_fell_back()
            );
            assert!(!out.is_empty());
            assert!(!out.contains("<think>") && !out.contains("</think>"));
        }
        ai.unload();
        ai.shutdown();
    }
}
