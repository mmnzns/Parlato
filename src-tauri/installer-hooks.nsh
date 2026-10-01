; NSIS installer hooks for Parlato.
;
; Referenced from tauri.conf.json bundle.windows.nsis.installerHooks.
; Tauri's NSIS template calls the four macros below at well-defined
; stages of install / uninstall. We use the post-uninstall hook to wipe
; every piece of state Parlato has created on disk + a few registry keys,
; so a reinstall starts fresh (triggers the onboarding, forgets which
; models were downloaded, clears any API key reference).
;
; Data Parlato writes :
;   %APPDATA%\com.craftconceptsdigital.parlato\
;     parla.settings.json          store plugin - general settings + onboarding flag
;     parla.prompts.json           store plugin - custom prompts + active prompt
;     parla.power_mode.json        store plugin - Power Mode profiles
;   %LOCALAPPDATA%\com.craftconceptsdigital.parlato\
;     Models\                      Whisper .bin files
;     ParakeetModels\              Parakeet ONNX files
;     LlmModels\                   llama.cpp GGUF files
;     VAD\                         Silero VAD ONNX
;     Recordings\                  WAV audio captures
;     history.sqlite3              history DB
;     logs\                        tracing output
;   HKCU\Software\Microsoft\Windows\CurrentVersion\Run\Parlato
;     autostart registration (if user enabled it)
;
; Credential Manager (API keys): keyring-rs names each one
; "<user>.<service>", service "Parlato", user from keychain_user() in
; src/services/api_keys.rs (e.g. "openAIAPIKey.Parlato"). The "Yes" answer
; deletes all of them with cmdkey. Keep the list below in sync with
; keychain_user() (Parlato, 2026-09-30).

!macro NSIS_HOOK_PREINSTALL
!macroend

!macro NSIS_HOOK_POSTINSTALL
!macroend

!macro NSIS_HOOK_PREUNINSTALL
!macroend

!macro NSIS_HOOK_POSTUNINSTALL
  ; Silent uninstalls (auto-updater flow : new installer runs old
  ; uninstaller with /S before installing the new version) must NOT
  ; wipe user data. Otherwise every auto-update would destroy models
  ; and settings. Only interactive uninstalls (Control Panel, Settings
  ; Apps) get to prompt the user.
  IfSilent parla_skip_wipe 0

    ; Ask the user whether to remove all Parlato data (settings, models,
    ; history, recordings, autostart). Default is "No" so clicking
    ; through preserves data by accident. /SD IDNO also makes silent
    ; runs answer No, which matches the IfSilent guard above anyway.
    MessageBox MB_YESNO|MB_ICONQUESTION \
      "Do you also want to delete all Parlato user data?$\r$\n$\r$\nThis will remove:$\r$\n    - Settings and onboarding state$\r$\n    - Custom prompts and Power Mode profiles$\r$\n    - Transcription history$\r$\n    - Downloaded models (Whisper, Parakeet, llama.cpp, VAD)$\r$\n    - Cached recordings and logs$\r$\n    - Autostart entry$\r$\n    - Account keys for online services$\r$\n$\r$\nChoose No to keep them for a future reinstall." \
      /SD IDNO IDNO parla_skip_wipe

    ; User stores (tauri-plugin-store JSON files)
    RMDir /r "$APPDATA\com.craftconceptsdigital.parlato"

    ; Models, history DB, recordings, logs
    RMDir /r "$LOCALAPPDATA\com.craftconceptsdigital.parlato"

    ; Autostart entry (if enabled via Settings > General)
    DeleteRegValue HKCU "Software\Microsoft\Windows\CurrentVersion\Run" "Parlato"

    ; Account keys in Credential Manager (missing ones just fail quietly).
    nsExec::Exec 'cmdkey /delete:groqAPIKey.Parlato'
    nsExec::Exec 'cmdkey /delete:deepgramAPIKey.Parlato'
    nsExec::Exec 'cmdkey /delete:cerebrasAPIKey.Parlato'
    nsExec::Exec 'cmdkey /delete:geminiAPIKey.Parlato'
    nsExec::Exec 'cmdkey /delete:mistralAPIKey.Parlato'
    nsExec::Exec 'cmdkey /delete:elevenLabsAPIKey.Parlato'
    nsExec::Exec 'cmdkey /delete:sonioxAPIKey.Parlato'
    nsExec::Exec 'cmdkey /delete:speechmaticsAPIKey.Parlato'
    nsExec::Exec 'cmdkey /delete:openAIAPIKey.Parlato'
    nsExec::Exec 'cmdkey /delete:anthropicAPIKey.Parlato'
    nsExec::Exec 'cmdkey /delete:openRouterAPIKey.Parlato'
    nsExec::Exec 'cmdkey /delete:customAPIKey.Parlato'
    nsExec::Exec 'cmdkey /delete:xAIAPIKey.Parlato'
    nsExec::Exec 'cmdkey /delete:cartesiaAPIKey.Parlato'
    nsExec::Exec 'cmdkey /delete:assemblyAIAPIKey.Parlato'

  parla_skip_wipe:
!macroend
