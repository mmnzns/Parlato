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
use std::collections::HashMap;
use std::sync::{LazyLock, Mutex, MutexGuard};

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

/// In-memory copy of each provider's key (None = no key saved). On macOS every
/// keychain read can prompt the user, so we read each key at most once per run.
static CACHE: LazyLock<Mutex<KeyCache>> = LazyLock::new(|| Mutex::new(HashMap::new()));

type KeyCache = HashMap<&'static str, Option<String>>;

/// Lock the cache, recovering from a poisoned mutex.
fn lock_cache() -> MutexGuard<'static, KeyCache> {
    CACHE.lock().unwrap_or_else(|e| e.into_inner())
}

/// Return the cached value, or call `read` and cache its result.
/// Errors are not cached, so a denied or failed read is retried next time.
fn get_cached(
    cache: &mut KeyCache,
    user: &'static str,
    read: impl FnOnce() -> Result<Option<String>>,
) -> Result<Option<String>> {
    if let Some(v) = cache.get(user) {
        return Ok(v.clone());
    }
    let v = read()?;
    cache.insert(user, v.clone());
    Ok(v)
}

/// Update the cache after a write: store `value` on success, drop the entry on failure.
fn update_cache<T>(
    cache: &mut KeyCache,
    user: &'static str,
    result: &Result<T>,
    value: Option<String>,
) {
    if result.is_ok() {
        cache.insert(user, value);
    } else {
        cache.remove(user);
    }
}

pub fn set_api_key(provider: &str, key: &str) -> Result<()> {
    let result = entry(provider).and_then(|e| {
        e.set_password(key)
            .map_err(|e| anyhow::anyhow!("keyring set: {e}"))
    });
    update_cache(
        &mut lock_cache(),
        keychain_user(provider),
        &result,
        Some(key.to_string()),
    );
    result
}

pub fn get_api_key(provider: &str) -> Result<Option<String>> {
    let mut cache = lock_cache();
    get_cached(&mut cache, keychain_user(provider), || {
        match entry(provider)?.get_password() {
            Ok(s) => Ok(Some(s)),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(e) => Err(anyhow::anyhow!("keyring get: {e}")),
        }
    })
}

pub fn delete_api_key(provider: &str) -> Result<()> {
    let result = entry(provider).and_then(|e| match e.delete_credential() {
        Ok(_) => Ok(()),
        Err(keyring::Error::NoEntry) => Ok(()),
        Err(e) => Err(anyhow::anyhow!("keyring delete: {e}")),
    });
    update_cache(&mut lock_cache(), keychain_user(provider), &result, None);
    result
}

pub fn has_api_key(provider: &str) -> bool {
    matches!(get_api_key(provider), Ok(Some(_)))
}

#[cfg(test)]
mod tests {
    use super::{get_cached, keychain_user, update_cache, KeyCache, PROVIDERS};
    use anyhow::{anyhow, Result};
    use std::cell::Cell;

    #[test]
    fn every_provider_has_a_keychain_name() {
        for p in PROVIDERS {
            assert_ne!(keychain_user(p), "unknownProviderAPIKey", "{p}");
        }
    }

    // Fake keychain read that counts its calls.
    fn reader<'a>(
        calls: &'a Cell<u32>,
        out: Option<&'a str>,
    ) -> impl FnOnce() -> Result<Option<String>> + 'a {
        move || {
            calls.set(calls.get() + 1);
            Ok(out.map(String::from))
        }
    }

    #[test]
    fn second_get_does_not_read_again() {
        let mut cache = KeyCache::new();
        let calls = Cell::new(0);
        let a = get_cached(&mut cache, "groqAPIKey", reader(&calls, Some("k1"))).unwrap();
        let b = get_cached(&mut cache, "groqAPIKey", reader(&calls, Some("k2"))).unwrap();
        assert_eq!(a.as_deref(), Some("k1"));
        assert_eq!(b.as_deref(), Some("k1"));
        assert_eq!(calls.get(), 1);
    }

    #[test]
    fn cached_none_is_returned_without_reading() {
        let mut cache = KeyCache::new();
        let calls = Cell::new(0);
        assert_eq!(
            get_cached(&mut cache, "groqAPIKey", reader(&calls, None)).unwrap(),
            None
        );
        assert_eq!(
            get_cached(&mut cache, "groqAPIKey", reader(&calls, Some("late"))).unwrap(),
            None
        );
        assert_eq!(calls.get(), 1);
    }

    #[test]
    fn errors_are_not_cached() {
        let mut cache = KeyCache::new();
        let calls = Cell::new(0);
        let failing = || -> Result<Option<String>> {
            calls.set(calls.get() + 1);
            Err(anyhow!("denied"))
        };
        assert!(get_cached(&mut cache, "groqAPIKey", failing).is_err());
        assert!(cache.is_empty());
        let v = get_cached(&mut cache, "groqAPIKey", reader(&calls, Some("k"))).unwrap();
        assert_eq!(v.as_deref(), Some("k"));
        assert_eq!(calls.get(), 2);
    }

    #[test]
    fn set_then_get_returns_new_value_without_reading() {
        let mut cache = KeyCache::new();
        let calls = Cell::new(0);
        get_cached(&mut cache, "groqAPIKey", reader(&calls, Some("old"))).unwrap();
        update_cache(&mut cache, "groqAPIKey", &Ok(()), Some("new".into()));
        let v = get_cached(&mut cache, "groqAPIKey", reader(&calls, Some("x"))).unwrap();
        assert_eq!(v.as_deref(), Some("new"));
        assert_eq!(calls.get(), 1);
    }

    #[test]
    fn delete_then_get_returns_none_without_reading() {
        let mut cache = KeyCache::new();
        let calls = Cell::new(0);
        get_cached(&mut cache, "groqAPIKey", reader(&calls, Some("old"))).unwrap();
        update_cache(&mut cache, "groqAPIKey", &Ok(()), None);
        let v = get_cached(&mut cache, "groqAPIKey", reader(&calls, Some("x"))).unwrap();
        assert_eq!(v, None);
        assert_eq!(calls.get(), 1);
    }

    #[test]
    fn failed_write_drops_the_cache_entry() {
        let mut cache = KeyCache::new();
        let calls = Cell::new(0);
        get_cached(&mut cache, "groqAPIKey", reader(&calls, Some("old"))).unwrap();
        update_cache::<()>(
            &mut cache,
            "groqAPIKey",
            &Err(anyhow!("boom")),
            Some("new".into()),
        );
        let v = get_cached(&mut cache, "groqAPIKey", reader(&calls, Some("disk"))).unwrap();
        assert_eq!(v.as_deref(), Some("disk"));
        assert_eq!(calls.get(), 2);
    }
}
