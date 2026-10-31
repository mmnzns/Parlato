// Local CLI provider : lance un outil en ligne de commande local avec le
// prompt transcription + systeme.
//
// Reference VoiceInk : VoiceInk/Services/AIEnhancement/LocalCLIService.swift.
// VoiceInk utilise /bin/zsh -lc. Sous Windows, on passe par
// `powershell.exe -NoProfile -Command` qui est present partout (pwsh.exe
// pourrait etre absent). Parlato : sur macOS, /bin/zsh -lc comme VoiceInk
// (memes outils, syntaxe shell), pour qu'un Power mode cree sur Windows
// fonctionne aussi sur le Mac.
//
// Templates reproduits (argument passe a -Command) :
//   - pi     : & pi -ne -ns -p --no-tools --system-prompt $env:PARLA_SYSTEM_PROMPT $env:PARLA_USER_PROMPT
//   - claude : & claude -p $env:PARLA_FULL_PROMPT
//   - codex  : $t = New-TemporaryFile; & codex exec --skip-git-repo-check --output-last-message $t.FullName $env:PARLA_FULL_PROMPT *> $null; Get-Content $t.FullName; Remove-Item $t
//   - custom : commande libre tapee par l'utilisateur.
//
// Le modele (champ "model") est le nom du template (pi/claude/codex/custom).
// La commande custom est stockee dans parla.settings.json:llm_localcli_custom_cmd.
// Le timeout est configurable dans parla.settings.json:llm_localcli_timeout_secs
// (defaut 45s).

use std::process::Stdio;
use std::time::Duration;

use anyhow::{anyhow, Result};
use async_trait::async_trait;
use tauri::AppHandle;
use tauri_plugin_store::StoreExt;
use tokio::io::AsyncWriteExt;
use tokio::process::Command;

use crate::enhancement::provider::{
    EnhancementRequest, EnhancementResponse, LLMProvider,
};

pub struct LocalCLIProvider;

const STORE_FILE: &str = "parla.settings.json";
const KEY_CUSTOM_CMD: &str = "llm_localcli_custom_cmd";
const KEY_TIMEOUT: &str = "llm_localcli_timeout_secs";
const DEFAULT_TIMEOUT: u64 = 45;

const TEMPLATES: &[&str] = &["pi", "claude", "codex", "custom"];

pub fn get_custom_cmd(app: &AppHandle) -> Option<String> {
    app.store(STORE_FILE)
        .ok()
        .and_then(|s| s.get(KEY_CUSTOM_CMD).and_then(|v| v.as_str().map(String::from)))
        .filter(|s| !s.is_empty())
}

pub fn set_custom_cmd(app: &AppHandle, cmd: &str) -> Result<()> {
    let store = app.store(STORE_FILE).map_err(|e| anyhow!("store: {e}"))?;
    if cmd.is_empty() {
        store.delete(KEY_CUSTOM_CMD);
    } else {
        store.set(KEY_CUSTOM_CMD, serde_json::Value::String(cmd.into()));
    }
    store.save().map_err(|e| anyhow!("store save: {e}"))?;
    Ok(())
}

pub fn get_timeout_secs(app: &AppHandle) -> u64 {
    app.store(STORE_FILE)
        .ok()
        .and_then(|s| s.get(KEY_TIMEOUT).and_then(|v| v.as_u64()))
        .unwrap_or(DEFAULT_TIMEOUT)
        .max(5)
}

pub fn set_timeout_secs(app: &AppHandle, secs: u64) -> Result<()> {
    let store = app.store(STORE_FILE).map_err(|e| anyhow!("store: {e}"))?;
    store.set(
        KEY_TIMEOUT,
        serde_json::Value::Number(serde_json::Number::from(secs.max(5))),
    );
    store.save().map_err(|e| anyhow!("store save: {e}"))?;
    Ok(())
}

/// Shell qui execute les modeles : PowerShell sur Windows, zsh ailleurs.
const ZSH: bool = cfg!(not(windows));

fn template_script(template: &str, custom_cmd: Option<&str>) -> Result<String> {
    template_script_for(template, custom_cmd, ZSH)
}

/// Parlato : memes modeles en syntaxe zsh (reference VoiceInk
/// LocalCLIService.swift, variables PARLA_* au lieu de VOICEINK_*).
fn zsh_template(template: &str) -> Option<&'static str> {
    match template {
        "pi" => Some(
            "pi -ne -ns -p --no-tools --system-prompt \"$PARLA_SYSTEM_PROMPT\" \"$PARLA_USER_PROMPT\"",
        ),
        "claude" => Some("claude -p \"$PARLA_FULL_PROMPT\""),
        "codex" => Some(concat!(
            "t=$(mktemp); ",
            "codex exec --skip-git-repo-check --output-last-message \"$t\" \"$PARLA_FULL_PROMPT\" >/dev/null 2>&1; ",
            "cat \"$t\"; ",
            "rm -f \"$t\""
        )),
        _ => None,
    }
}

fn template_script_for(template: &str, custom_cmd: Option<&str>, zsh: bool) -> Result<String> {
    if zsh {
        if let Some(script) = zsh_template(template) {
            return Ok(script.to_string());
        }
    }
    match template {
        "pi" => Ok(String::from(
            "& pi -ne -ns -p --no-tools --system-prompt $env:PARLA_SYSTEM_PROMPT $env:PARLA_USER_PROMPT",
        )),
        "claude" => Ok(String::from("& claude -p $env:PARLA_FULL_PROMPT")),
        "codex" => Ok(String::from(concat!(
            "$t = New-TemporaryFile; ",
            "& codex exec --skip-git-repo-check --output-last-message $t.FullName $env:PARLA_FULL_PROMPT *> $null; ",
            "Get-Content -Raw $t.FullName; ",
            "Remove-Item $t"
        ))),
        "custom" => {
            let cmd = custom_cmd.ok_or_else(|| {
                anyhow!("LocalCLI custom: definissez llm_localcli_custom_cmd")
            })?;
            Ok(cmd.to_string())
        }
        other => Err(anyhow!("LocalCLI template inconnu: {other}")),
    }
}

fn full_prompt(system: &str, user: &str) -> String {
    format!(
        "<SYSTEM_PROMPT>\n{system}\n</SYSTEM_PROMPT>\n\n<USER_PROMPT>\n{user}\n</USER_PROMPT>"
    )
}

/// Parlato : short translatable code for a failed command, from its exit
/// code and lowercased stderr. Never carries the command's output.
fn failure_code(code: i32, stderr_lower: &str, success: bool, stdout_empty: bool) -> Option<String> {
    if code == 127
        || stderr_lower.contains("is not recognized")
        || stderr_lower.contains("command not found")
    {
        return Some("PARLA_ERR:cliNotFound".into());
    }
    if !success {
        return Some(format!("PARLA_ERR:cliFailed:{code}"));
    }
    if stdout_empty {
        return Some("PARLA_ERR:cliEmpty".into());
    }
    None
}

#[async_trait]
impl LLMProvider for LocalCLIProvider {
    fn id(&self) -> &'static str {
        "localcli"
    }
    fn label(&self) -> &'static str {
        "Local CLI"
    }
    fn default_models(&self) -> &'static [&'static str] {
        TEMPLATES
    }
    fn default_model(&self) -> &'static str {
        "pi"
    }
    fn endpoint(&self) -> &'static str {
        if ZSH {
            "/bin/zsh -lc"
        } else {
            "powershell.exe -NoProfile -Command"
        }
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
        // endpoint_override sert ici a passer la commande custom si template=custom.
        let custom_cmd = req.endpoint_override.as_deref();
        let script = template_script(&req.model, custom_cmd)?;
        let fp = full_prompt(&req.system_prompt, &req.user_message);

        // Note : on utilise powershell.exe (v5, installe partout sur Windows 10+).
        // pwsh.exe (PowerShell Core) peut etre absent.
        let (program, args): (&str, [&str; 2]) = if ZSH {
            ("/bin/zsh", ["-lc", &script])
        } else {
            ("powershell.exe", ["-NoProfile", "-Command"])
        };
        let mut cmd = Command::new(program);
        cmd.args(args);
        if !ZSH {
            cmd.arg(&script);
        }
        let mut child = cmd
            .env("PARLA_SYSTEM_PROMPT", &req.system_prompt)
            .env("PARLA_USER_PROMPT", &req.user_message)
            .env("PARLA_FULL_PROMPT", &fp)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| anyhow!("spawn {program}: {e}"))?;

        // Ecrit le full prompt dans stdin (comme VoiceInk) puis ferme.
        if let Some(stdin) = child.stdin.as_mut() {
            let _ = stdin.write_all(fp.as_bytes()).await;
            let _ = stdin.shutdown().await;
        }

        let timeout = Duration::from_secs(req.timeout.as_secs().max(5));
        let output = match tokio::time::timeout(timeout, child.wait_with_output()).await {
            Ok(r) => r.map_err(|e| anyhow!("wait: {e}"))?,
            Err(_) => {
                return Err(anyhow!(
                    "timeout: LocalCLI template {} > {:?}",
                    req.model,
                    timeout
                ))
            }
        };

        let code = output.status.code().unwrap_or(-1);
        let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
        // Parlato : stderr is only inspected, never put in an error or a log: it
        // can echo the prompt (the dictated text) or other data.
        let low = String::from_utf8_lossy(&output.stderr).to_lowercase();
        if let Some(err) = failure_code(code, &low, output.status.success(), stdout.is_empty()) {
            return Err(anyhow!(err));
        }
        Ok(EnhancementResponse { text: stdout })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn templates_exist_for_both_shells() {
        for t in ["pi", "claude", "codex"] {
            let ps = template_script_for(t, None, false).unwrap();
            let zsh = template_script_for(t, None, true).unwrap();
            assert!(ps.contains("$env:PARLA_"), "{t}: {ps}");
            assert!(
                zsh.contains("\"$PARLA_") && !zsh.contains("$env:"),
                "{t}: {zsh}"
            );
        }
        assert_eq!(
            template_script_for("custom", Some("my-tool"), true).unwrap(),
            "my-tool"
        );
        assert!(template_script_for("custom", None, true).is_err());
    }

    #[test]
    fn failure_codes_never_carry_command_output() {
        assert_eq!(
            failure_code(127, "", false, true).as_deref(),
            Some("PARLA_ERR:cliNotFound")
        );
        assert_eq!(
            failure_code(1, "zsh: command not found: pi", false, true).as_deref(),
            Some("PARLA_ERR:cliNotFound")
        );
        assert_eq!(
            failure_code(2, "error: the text i dictated", false, false).as_deref(),
            Some("PARLA_ERR:cliFailed:2")
        );
        assert_eq!(
            failure_code(0, "the text i dictated", true, true).as_deref(),
            Some("PARLA_ERR:cliEmpty")
        );
        assert_eq!(failure_code(0, "", true, false), None);
    }
}
