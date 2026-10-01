// Gestion des cles API des providers cloud via Windows Credential Manager.
//
// Reference VoiceInk : VoiceInk/Services/APIKeyManager.swift
// VoiceInk utilise le Keychain macOS. Ici on utilise `keyring` (Windows
// Credential Manager via la feature windows-native).
//
// Mapping identique a VoiceInk (APIKeyManager.swift L15-27) :
//   groq          -> groqAPIKey
//   deepgram      -> deepgramAPIKey
//   cerebras      -> cerebrasAPIKey
//   gemini        -> geminiAPIKey
//   mistral       -> mistralAPIKey
//   elevenlabs    -> elevenLabsAPIKey
//   soniox        -> sonioxAPIKey
//   speechmatics  -> speechmaticsAPIKey
//   openai        -> openAIAPIKey
//   anthropic     -> anthropicAPIKey
//   openrouter    -> openRouterAPIKey

use anyhow::{Context, Result};
use keyring::Entry;

const SERVICE: &str = "Parlato";

/// Parlato : every provider that can have a saved key (same list as
/// keychain_user). Used by "Delete all Parlato data"; the Windows
/// uninstaller (installer-hooks.nsh) deletes the same keys.
pub const PROVIDERS: [&str; 15] = [
    "groq",
    "deepgram",
    "cerebras",
    "gemini",
    "mistral",
    "elevenlabs",
    "soniox",
    "speechmatics",
    "openai",
    "anthropic",
    "openrouter",
    "custom",
    "xai",
    "cartesia",
    "assemblyai",
];

/// Retourne le username stocke cote keyring pour un provider donne.
/// Case-insensitive comme VoiceInk (APIKeyManager L183).
fn keychain_user(provider: &str) -> &'static str {
    match provider.to_lowercase().as_str() {
        "groq" => "groqAPIKey",
        "deepgram" => "deepgramAPIKey",
        "cerebras" => "cerebrasAPIKey",
        "gemini" => "geminiAPIKey",
        "mistral" => "mistralAPIKey",
        "elevenlabs" => "elevenLabsAPIKey",
        "soniox" => "sonioxAPIKey",
        "speechmatics" => "speechmaticsAPIKey",
        "openai" => "openAIAPIKey",
        "anthropic" => "anthropicAPIKey",
        "openrouter" => "openRouterAPIKey",
        "custom" => "customAPIKey",
        "xai" => "xAIAPIKey",
        "cartesia" => "cartesiaAPIKey",
        "assemblyai" => "assemblyAIAPIKey",
        _ => "unknownProviderAPIKey",
    }
}

fn entry(provider: &str) -> Result<Entry> {
    Entry::new(SERVICE, keychain_user(provider))
        .with_context(|| format!("keyring entry for {provider}"))
}

pub fn set_api_key(provider: &str, key: &str) -> Result<()> {
    entry(provider)?
        .set_password(key)
        .map_err(|e| anyhow::anyhow!("keyring set: {e}"))
}

pub fn get_api_key(provider: &str) -> Result<Option<String>> {
    match entry(provider)?.get_password() {
        Ok(s) => Ok(Some(s)),
        Err(keyring::Error::NoEntry) => Ok(None),
        Err(e) => Err(anyhow::anyhow!("keyring get: {e}")),
    }
}

pub fn delete_api_key(provider: &str) -> Result<()> {
    match entry(provider)?.delete_credential() {
        Ok(_) => Ok(()),
        Err(keyring::Error::NoEntry) => Ok(()),
        Err(e) => Err(anyhow::anyhow!("keyring delete: {e}")),
    }
}

pub fn has_api_key(provider: &str) -> bool {
    matches!(get_api_key(provider), Ok(Some(_)))
}

#[cfg(test)]
mod tests {
    use super::{keychain_user, PROVIDERS};

    #[test]
    fn every_provider_has_a_keychain_name() {
        for p in PROVIDERS {
            assert_ne!(keychain_user(p), "unknownProviderAPIKey", "{p}");
        }
    }
}
