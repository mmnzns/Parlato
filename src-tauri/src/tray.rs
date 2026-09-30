// System tray with the VoiceInk 2.x menu bar layout.
//
// Reference VoiceInk : App/MenuBar/MenuBarView.swift (completedOnboardingMenu)
//  - Toggle Recorder
//  - Mode: <active>  ▸ enabled modes (✓ on the effective one), Manage Modes,
//                       Manage Models
//  - Audio Input     ▸ input devices (✓ on the selected one)
//  - Retry Last Transcription
//  - Copy Last Transcription        Shift+Cmd+C
//  - History                        Shift+Cmd+H
//  - Hide / Show Dock Icon          (macOS only, no Windows equivalent)
//  - Launch at Login (check item)
//  - Settings                       Cmd+,
//  - Check for Updates
//  - Quit VoiceInk
// Before the onboarding is completed the menu only offers "Complete
// Onboarding" and "Quit" (MenuBarView.onboardingMenu).
//
// Windows adaptations :
//  - "Open Parla" stays first : a left click on the tray icon also opens
//    the main window (standard Windows UX).
//  - Shortcut labels are displayed in the native accelerator column (text
//    after a tab character, which Windows menus right-align). They are the
//    user's configured global shortcuts (hotkeys::keyboard_hook), the menu
//    itself never registers accelerators : VoiceInk's Shift+Cmd+C is also a
//    menu-only accelerator, the global bindings live in ShortcutMonitor.
//  - The menu is rebuilt (`refresh`) whenever its inputs change : shortcut
//    config, Power Mode profiles / active session, selected microphone,
//    UI language, onboarding state, autostart. A SwiftUI menu is reactive,
//    a Win32 menu is not.
//  - Labels are localized from the UI language mirrored in the settings
//    store by the frontend (`set_ui_language`).

use tauri::{
    menu::{CheckMenuItem, Menu, MenuItem, PredefinedMenuItem, Submenu},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    AppHandle, Emitter, Manager, Wry,
};
use tracing::{info, warn};

use crate::audio;
use crate::commands::{hotkey as hotkey_cfg, permissions, settings};
use crate::history::last_transcription;
use crate::hotkeys::keyboard_hook::{trigger_label, HotkeyTrigger};
use crate::power_mode;
use crate::transcription::engine;

const TRAY_ID: &str = "main";

pub fn setup(app: &AppHandle) -> tauri::Result<()> {
    let menu = build_menu(app)?;
    #[cfg(not(target_os = "macos"))]
    let icon = app
        .default_window_icon()
        .cloned()
        .expect("default window icon is bundled");
    // Parlato : macOS menu bar icons are one-color "template" images that
    // macOS tints for light and dark menu bars.
    #[cfg(target_os = "macos")]
    let icon = tauri::image::Image::from_bytes(include_bytes!("../icons/tray-template.png"))?;

    // Parlato : on macOS a left click opens the menu (the platform habit,
    // "Open Parlato" is its first item); on Windows it opens the window.
    #[cfg(target_os = "macos")]
    let left_click_menu = true;
    #[cfg(not(target_os = "macos"))]
    let left_click_menu = false;

    TrayIconBuilder::with_id(TRAY_ID)
        .tooltip("Parlato")
        .icon(icon)
        .icon_as_template(cfg!(target_os = "macos"))
        .menu(&menu)
        .show_menu_on_left_click(left_click_menu)
        .on_menu_event(|app, event| handle_menu_event(app, event.id.as_ref()))
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                show_main_window(tray.app_handle());
            }
        })
        .build(app)?;

    Ok(())
}

/// Rebuilds the tray menu from the current state. Safe to call from any
/// thread : Tauri marshals menu creation and `set_menu` to the main thread.
/// No-op before `setup` (tray not created yet).
pub fn refresh(app: &AppHandle) {
    let Some(tray) = app.tray_by_id(TRAY_ID) else {
        return;
    };
    match build_menu(app) {
        Ok(menu) => {
            if let Err(e) = tray.set_menu(Some(menu)) {
                warn!(error = %e, "tray: set_menu failed");
            }
        }
        Err(e) => warn!(error = %e, "tray: build_menu failed"),
    }
}

/// Appends the configured shortcut label in the accelerator column.
fn with_accel(text: String, trigger: HotkeyTrigger) -> String {
    match trigger_label(trigger) {
        // Parlato : a tab lines the shortcut up on the right in Windows
        // menus; macOS menus show the tab as a gap, so use parentheses.
        #[cfg(not(target_os = "macos"))]
        Some(accel) => format!("{text}\t{accel}"),
        #[cfg(target_os = "macos")]
        Some(accel) => format!("{text} ({accel})"),
        None => text,
    }
}

fn build_menu(app: &AppHandle) -> tauri::Result<Menu<Wry>> {
    let lang = settings::ui_language(app);
    let t = |key: &str| tr(&lang, key);

    if !permissions::get_onboarding_completed(app.clone()) {
        let complete = MenuItem::with_id(app, "open", t("completeOnboarding"), true, None::<&str>)?;
        let sep = PredefinedMenuItem::separator(app)?;
        let quit = MenuItem::with_id(app, "quit", t("quit"), true, None::<&str>)?;
        return Menu::with_items(app, &[&complete, &sep, &quit]);
    }

    let hk = hotkey_cfg::load(app);

    let open = MenuItem::with_id(app, "open", t("open"), true, None::<&str>)?;
    let toggle = MenuItem::with_id(
        app,
        "toggle_record",
        with_accel(t("toggleRecorder"), hk.primary.trigger),
        true,
        None::<&str>,
    )?;

    // -- Power Mode submenu (VoiceInk "Mode: <name>") -----------------------
    let configs = power_mode::session::enabled_configs(app);
    let effective = power_mode::session::effective_config_id(app);
    let active_name = configs
        .iter()
        .find(|c| Some(&c.id) == effective.as_ref())
        .map(|c| c.name.clone())
        .unwrap_or_else(|| t("none"));
    let mode_menu = Submenu::with_id(
        app,
        "mode_menu",
        format!("{}: {}", t("mode"), active_name),
        true,
    )?;
    if configs.is_empty() {
        mode_menu.append(&MenuItem::with_id(
            app,
            "mode_none",
            t("noModes"),
            false,
            None::<&str>,
        )?)?;
    }
    for c in &configs {
        let is_active = Some(&c.id) == effective.as_ref();
        let text = if is_active {
            format!("{} {}  \u{2713}", c.emoji, c.name)
        } else {
            format!("{} {}", c.emoji, c.name)
        };
        mode_menu.append(&MenuItem::with_id(
            app,
            format!("mode:{}", c.id),
            text,
            true,
            None::<&str>,
        )?)?;
    }
    mode_menu.append(&PredefinedMenuItem::separator(app)?)?;
    mode_menu.append(&MenuItem::with_id(
        app,
        "manage_modes",
        t("manageModes"),
        true,
        None::<&str>,
    )?)?;
    mode_menu.append(&MenuItem::with_id(
        app,
        "manage_models",
        t("manageModels"),
        true,
        None::<&str>,
    )?)?;

    // -- Audio input submenu (VoiceInk "Audio Input") -----------------------
    let selected_device = audio::device::selected_input_device(app);
    let audio_menu = Submenu::with_id(app, "audio_menu", t("audioInput"), true)?;
    audio_menu.append(&CheckMenuItem::with_id(
        app,
        "device:",
        t("systemDefault"),
        true,
        selected_device.is_none(),
        None::<&str>,
    )?)?;
    for d in audio::list_input_devices() {
        let checked = selected_device.as_deref() == Some(d.name.as_str());
        audio_menu.append(&CheckMenuItem::with_id(
            app,
            format!("device:{}", d.name),
            &d.name,
            true,
            checked,
            None::<&str>,
        )?)?;
    }

    // -- Last transcription actions ------------------------------------------
    let retry = MenuItem::with_id(
        app,
        "retry_last",
        with_accel(t("retryLast"), hk.actions.retry_last_transcription),
        true,
        None::<&str>,
    )?;
    let copy_last = MenuItem::with_id(
        app,
        "copy_last",
        with_accel(t("copyLast"), hk.actions.copy_last_transcription),
        true,
        None::<&str>,
    )?;
    let history = MenuItem::with_id(
        app,
        "history",
        with_accel(t("history"), hk.actions.open_history),
        true,
        None::<&str>,
    )?;
    let launch_at_login = CheckMenuItem::with_id(
        app,
        "launch_at_login",
        t("launchAtLogin"),
        true,
        autostart_enabled(app),
        None::<&str>,
    )?;

    let settings_item = MenuItem::with_id(app, "settings", t("settings"), true, None::<&str>)?;
    let check_update = MenuItem::with_id(
        app,
        "check_update",
        t("checkUpdates"),
        true,
        None::<&str>,
    )?;
    let quit = MenuItem::with_id(app, "quit", t("quit"), true, None::<&str>)?;

    let sep1 = PredefinedMenuItem::separator(app)?;
    let sep2 = PredefinedMenuItem::separator(app)?;
    let sep3 = PredefinedMenuItem::separator(app)?;

    Menu::with_items(
        app,
        &[
            &open,
            &toggle,
            &sep1,
            &mode_menu,
            &audio_menu,
            &sep2,
            &retry,
            &copy_last,
            &history,
            &launch_at_login,
            &sep3,
            &settings_item,
            &check_update,
            &quit,
        ],
    )
}

fn handle_menu_event(app: &AppHandle, id: &str) {
    match id {
        "open" => show_main_window(app),
        "toggle_record" => engine::toggle_from_ui(app),
        "manage_modes" => navigate(app, "powermode"),
        "manage_models" => navigate(app, "models"),
        "retry_last" => report(app, "retry", last_transcription::retry_last(app)),
        "copy_last" => report(app, "copy", last_transcription::copy_last(app)),
        "history" => last_transcription::open_history(app),
        "launch_at_login" => toggle_autostart(app),
        "settings" => navigate(app, "settings"),
        "check_update" => {
            show_main_window(app);
            let _ = app.emit("tray:check-update", ());
        }
        "quit" => app.exit(0),
        other => {
            if let Some(mode_id) = other.strip_prefix("mode:") {
                select_mode(app, mode_id);
            } else if let Some(device) = other.strip_prefix("device:") {
                select_device(app, device);
            }
        }
    }
}

fn report(app: &AppHandle, what: &str, result: anyhow::Result<()>) {
    if let Err(e) = result {
        warn!(action = what, error = %e, "tray action failed");
        let _ = app.emit("tray:notice", format!("{what} failed: {e}"));
    }
}

fn navigate(app: &AppHandle, panel: &str) {
    crate::mini_recorder::show_main_window(app.clone(), Some(panel.to_string()));
}

fn select_mode(app: &AppHandle, mode_id: &str) {
    if let Some(session) = power_mode::session::select_by_id(app, mode_id) {
        let _ = app.emit("power_mode:active", &session);
    }
    refresh(app);
}

fn select_device(app: &AppHandle, device: &str) {
    let name = if device.is_empty() {
        None
    } else {
        Some(device.to_string())
    };
    if let Err(e) = audio::device::set_selected_input_device(app, name) {
        warn!(error = %e, "tray: set_selected_input_device failed");
    }
    refresh(app);
}

fn autostart_enabled(app: &AppHandle) -> bool {
    use tauri_plugin_autostart::ManagerExt;
    app.autolaunch().is_enabled().unwrap_or(false)
}

fn toggle_autostart(app: &AppHandle) {
    use tauri_plugin_autostart::ManagerExt;
    let mgr = app.autolaunch();
    let enabled = mgr.is_enabled().unwrap_or(false);
    let result = if enabled { mgr.disable() } else { mgr.enable() };
    match result {
        Ok(()) => info!(enabled = !enabled, "tray: launch at login toggled"),
        Err(e) => warn!(error = %e, "tray: launch at login toggle failed"),
    }
    let _ = app.emit("settings:autostart-changed", !enabled);
    refresh(app);
}

fn show_main_window(app: &AppHandle) {
    if let Some(win) = app.get_webview_window("main") {
        let _ = win.show();
        let _ = win.unminimize();
        let _ = win.set_focus();
    }
}

/// Parlato: one-time Windows notification after the first close-to-tray,
/// so closing the window does not look like quitting.
pub fn notify_still_running(app: &AppHandle) {
    use tauri_plugin_notification::NotificationExt;
    let lang = settings::ui_language(app);
    if let Err(e) = app
        .notification()
        .builder()
        .title(tr(&lang, "stillRunningTitle"))
        .body(tr(&lang, "stillRunningBody"))
        .show()
    {
        warn!(error = %e, "tray: still-running notification failed");
    }
}

/// Native menu labels. Mirrors the `tray.*` keys of the frontend locales
/// (en / fr / es) ; the native menu cannot use react-i18next.
fn tr(lang: &str, key: &str) -> String {
    let col = match lang {
        "fr" => 1,
        "es" => 2,
        _ => 0,
    };
    let row: [&str; 3] = match key {
        "open" => ["Open Parlato", "Ouvrir Parlato", "Abrir Parlato"],
        "completeOnboarding" => [
            "Complete onboarding",
            "Terminer la configuration",
            "Completar la configuraci\u{f3}n",
        ],
        "toggleRecorder" => [
            "Start or stop dictation",
            "D\u{e9}marrer / arr\u{ea}ter la dict\u{e9}e",
            "Iniciar / detener el dictado",
        ],
        "mode" => ["Power mode", "Mode", "Modo"],
        "none" => ["None", "Aucun", "Ninguno"],
        "noModes" => [
            "No power modes yet",
            "Aucun mode pour l'instant",
            "Sin modos por ahora",
        ],
        "manageModes" => [
            "Manage power modes",
            "G\u{e9}rer les modes",
            "Gestionar modos",
        ],
        "manageModels" => ["Speech model", "Mod\u{e8}le vocal", "Modelo de voz"],
        "audioInput" => ["Microphone", "Micro", "Micr\u{f3}fono"],
        "systemDefault" => [
            "Same as computer",
            "Comme l'ordinateur",
            "Igual que el ordenador",
        ],
        "retryLast" => [
            "Retry last transcription",
            "R\u{e9}essayer la derni\u{e8}re transcription",
            "Reintentar la \u{fa}ltima transcripci\u{f3}n",
        ],
        "copyLast" => [
            "Copy last transcription",
            "Copier la derni\u{e8}re transcription",
            "Copiar la \u{fa}ltima transcripci\u{f3}n",
        ],
        "history" => ["History", "Historique", "Historial"],
        "launchAtLogin" => [
            "Start with computer",
            "Lancer avec l'ordinateur",
            "Iniciar con el ordenador",
        ],
        "settings" => ["Settings", "Param\u{e8}tres", "Ajustes"],
        "checkUpdates" => [
            "Check for updates",
            "V\u{e9}rifier les mises \u{e0} jour",
            "Buscar actualizaciones",
        ],
        "quit" => ["Quit Parlato", "Quitter Parlato", "Salir de Parlato"],
        "stillRunningTitle" => [
            "Parlato is still running",
            "Parlato tourne toujours",
            "Parlato sigue funcionando",
        ],
        // Parlato : on macOS the icon is in the menu bar and a click opens
        // its menu (Open Parlato, Quit...).
        #[cfg(target_os = "macos")]
        "stillRunningBody" => [
            "Dictation keeps working. Parlato's icon is in the menu bar near the clock: click it to open Parlato or quit.",
            "La dict\u{e9}e reste active. L'ic\u{f4}ne de Parlato est dans la barre des menus pr\u{e8}s de l'horloge : cliquez dessus pour ouvrir Parlato ou quitter.",
            "El dictado sigue activo. El icono de Parlato est\u{e1} en la barra de men\u{fa}s junto al reloj: haz clic para abrir Parlato o salir.",
        ],
        #[cfg(not(target_os = "macos"))]
        "stillRunningBody" => [
            "Dictation keeps working. Parlato's icon is near the clock: click it to open, right-click to quit.",
            "La dict\u{e9}e reste active. L'ic\u{f4}ne de Parlato est pr\u{e8}s de l'horloge : clic pour ouvrir, clic droit pour quitter.",
            "El dictado sigue activo. El icono de Parlato est\u{e1} junto al reloj: clic para abrir, clic derecho para salir.",
        ],
        _ => [key, key, key],
    };
    row[col].to_string()
}
