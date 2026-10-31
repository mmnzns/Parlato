// Mini-recorder window : panel flottant non-activating bottom-center OU top-center,
// plus la fenetre popover du bouton Mode.
//
// Reference VoiceInk :
// - Features/Recording/Presentation/MiniRecorderPanel.swift : NSPanel
//   nonactivatingPanel floating, canBecomeKey=false, bottom-center padding
//   24 px, 300 x 120.
// - Features/Recording/Presentation/NotchRecorderPanel.swift : variante
//   top-center collant au notch MacBook Pro, y = screen.maxY - 200, pill
//   noir (radius top-flat / bottom-rounded), blur material .dark.
// - RecorderPanelStyle : UserDefaults "RecorderType" ∈ {"mini", "notch"},
//   par defaut "mini".
// - Features/Recording/Components/RecorderComponents.swift
//   RecorderModeButton : `.popover(isPresented:arrowEdge: .bottom)` =
//   NSPopover, donc une fenetre separee de 180 pt de large (340 max) ancree
//   sur le bouton, ouverte au survol / clic.
//
// Sur Windows on obtient le meme effet via des WebviewWindow Tauri avec
// decorations=false, always_on_top=true, skip_taskbar=true, resizable=false,
// focused=false, transparent, shadow=false, plus WS_EX_NOACTIVATE. Le
// popover est une seconde fenetre ("recorder-popover"), pre-creee cachee
// avec la bulle pour que son webview soit deja charge quand l'utilisateur
// survole le bouton, puis simplement montree / cachee. Auparavant le
// popover etait rendu DANS la fenetre de la bulle, qu'il fallait agrandir
// puis retrecir a chaque ouverture : c'etait la source des sauts visuels.

use std::sync::atomic::{AtomicU64, Ordering};

use tauri::{AppHandle, LogicalPosition, LogicalSize, Manager, WebviewUrl, WebviewWindowBuilder};
use tauri_plugin_store::StoreExt;
use tracing::{debug, warn};

pub const LABEL: &str = "recorder";
pub const POPOVER_LABEL: &str = "recorder-popover";

const STORE_FILE: &str = "parla.settings.json";
const KEY_STYLE: &str = "recorder_style";

/// Dimensions VoiceInk MiniRecorderPanel.
const WIDTH: f64 = 300.0;
const HEIGHT: f64 = 120.0;
/// Espace (pixels logiques) entre le bas de la bulle et le bas de la zone
/// utile (haut de la barre des taches ou du Dock), meme valeur sur Windows
/// et macOS pour que la bulle paraisse flotter.
const BOTTOM_PADDING: f64 = 32.0;

/// Fenetre popover du bouton Mode (VoiceInk ModePopover : 180 pt de large,
/// 340 max de haut ; ici 184 px, la largeur de la bulle).
const POPOVER_WIDTH: f64 = 184.0;
const POPOVER_HEIGHT: f64 = 320.0;
/// Espace entre la bulle et le popover (fleche NSPopover).
const POPOVER_GAP: f64 = 8.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecorderStyle {
    Mini,
    Notch,
}

impl RecorderStyle {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Mini => "mini",
            Self::Notch => "notch",
        }
    }
    pub fn parse(s: &str) -> Self {
        if s.eq_ignore_ascii_case("notch") {
            Self::Notch
        } else {
            Self::Mini
        }
    }
}

pub fn get_style(app: &AppHandle) -> RecorderStyle {
    app.store(STORE_FILE)
        .ok()
        .and_then(|s| s.get(KEY_STYLE).and_then(|v| v.as_str().map(String::from)))
        .map(|s| RecorderStyle::parse(&s))
        .unwrap_or(RecorderStyle::Mini)
}

pub fn set_style(app: &AppHandle, style: RecorderStyle) -> anyhow::Result<()> {
    let store = app
        .store(STORE_FILE)
        .map_err(|e| anyhow::anyhow!("store: {e}"))?;
    store.set(KEY_STYLE, serde_json::Value::String(style.as_str().into()));
    store
        .save()
        .map_err(|e| anyhow::anyhow!("store save: {e}"))
}

/// Parlato : numero de la bulle courante, incremente a chaque ouverture.
/// Une fermeture differee (fin de dictee, message) ne ferme la bulle que si
/// personne ne l'a rouverte entre-temps, sinon elle fermerait la bulle de
/// la dictee suivante.
static GENERATION: AtomicU64 = AtomicU64::new(0);

pub fn generation() -> u64 {
    GENERATION.load(Ordering::SeqCst)
}

/// Ferme la bulle si elle n'a pas ete rouverte depuis `generation`.
pub fn close_if_current(app: &AppHandle, generation: u64) {
    if GENERATION.load(Ordering::SeqCst) == generation {
        close(app);
    }
}

/// Parlato : ouvre la bulle pour un court message (`notice`), et retourne
/// son numero pour la fermeture differee.
pub fn show_for_notice(app: &AppHandle) -> u64 {
    ensure_open(app);
    generation()
}

/// Cree la fenetre recorder si elle n'existe pas, sinon la ramene au premier plan.
/// Pre-cree aussi la fenetre popover (cachee).
pub fn ensure_open(app: &AppHandle) {
    GENERATION.fetch_add(1, Ordering::SeqCst);
    let style = get_style(app);
    if let Some(existing) = app.get_webview_window(LABEL) {
        reposition(&existing, style);
        let _ = existing.show();
        ensure_popover_window(app);
        return;
    }

    // URL frontend : meme bundle, route hash pour detecter la vue.
    // WebviewUrl::App est resolu par Tauri vers le serveur dev en mode dev
    // (http://localhost:1420/...) et vers les assets bundle en release.
    let url = WebviewUrl::App("index.html#recorder".into());

    let builder = WebviewWindowBuilder::new(app, LABEL, url)
        .title("Parlato Recorder")
        .inner_size(WIDTH, HEIGHT)
        .resizable(false)
        .decorations(false)
        .always_on_top(true)
        .skip_taskbar(true)
        .focused(false)
        .shadow(false)
        .transparent(true);

    let window = match builder.build() {
        Ok(w) => w,
        Err(e) => {
            warn!("Impossible de creer la fenetre recorder: {e}");
            return;
        }
    };

    apply_no_activate_style(&window);
    reposition(&window, style);
    debug!(style = style.as_str(), "Fenetre recorder creee");
    ensure_popover_window(app);
}

/// Cree la fenetre popover cachee si elle n'existe pas. Elle vit tant que
/// la bulle vit (fermee dans `close`).
fn ensure_popover_window(app: &AppHandle) -> Option<tauri::WebviewWindow> {
    if let Some(existing) = app.get_webview_window(POPOVER_LABEL) {
        return Some(existing);
    }
    let url = WebviewUrl::App("index.html#recorder-popover".into());
    let builder = WebviewWindowBuilder::new(app, POPOVER_LABEL, url)
        .title("Parlato Recorder Popover")
        .inner_size(POPOVER_WIDTH, POPOVER_HEIGHT)
        .resizable(false)
        .decorations(false)
        .always_on_top(true)
        .skip_taskbar(true)
        .focused(false)
        .shadow(false)
        .transparent(true)
        .visible(false);
    match builder.build() {
        Ok(w) => {
            apply_no_activate_style(&w);
            debug!("Fenetre popover recorder creee (cachee)");
            Some(w)
        }
        Err(e) => {
            warn!("Impossible de creer la fenetre popover: {e}");
            None
        }
    }
}

/// Ajoute les styles Windows WS_EX_NOACTIVATE + WS_EX_TOOLWINDOW pour que
/// la fenetre ne prenne jamais le focus, meme quand on clique dessus.
/// Equivalent du nonactivatingPanel macOS utilise par VoiceInk
/// (MiniRecorderPanel.swift). Sans ce flag Windows, ouvrir la pill
/// vole brievement le focus a l'app cible (Notepad/VS Code) et casse le
/// paste Ctrl+V au stop.
fn apply_no_activate_style(window: &tauri::WebviewWindow) {
    #[cfg(windows)]
    {
        use windows::Win32::Foundation::HWND;
        use windows::Win32::UI::WindowsAndMessaging::{
            GetWindowLongPtrW, SetWindowLongPtrW, GWL_EXSTYLE, WS_EX_NOACTIVATE,
            WS_EX_TOOLWINDOW,
        };
        let Ok(raw_hwnd) = window.hwnd() else {
            warn!("hwnd() recorder: indisponible");
            return;
        };
        let hwnd = HWND(raw_hwnd.0 as *mut _);
        unsafe {
            let cur = GetWindowLongPtrW(hwnd, GWL_EXSTYLE);
            let extra = (WS_EX_NOACTIVATE.0 | WS_EX_TOOLWINDOW.0) as isize;
            SetWindowLongPtrW(hwnd, GWL_EXSTYLE, cur | extra);
        }
    }
    // Parlato : on macOS, keep the pill on every Space (desktop), so it
    // follows the user instead of staying on the Space it opened on.
    #[cfg(target_os = "macos")]
    {
        let _ = window.set_visible_on_all_workspaces(true);
    }
    #[cfg(not(any(windows, target_os = "macos")))]
    {
        let _ = window;
    }
}

/// Parlato : rectangle en pixels physiques (position du coin haut-gauche et
/// taille). Sert a decrire l'ecran et sa zone utile (hors barre des taches,
/// Dock, barre de menus).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct PxRect {
    x: i32,
    y: i32,
    w: i32,
    h: i32,
}

/// Parlato : position (pixels physiques, coin haut-gauche) de la fenetre de
/// la bulle. Fonction pure, testee ci-dessous.
///
/// - Horizontalement : centree dans la zone utile (`work`).
/// - Mini : le bas de la fenetre est a `gap` pixels logiques AU-DESSUS du bas
///   de la zone utile, donc au-dessus de la barre des taches ou du Dock. Une
///   barre des taches masquee automatiquement laisse la zone utile egale a
///   l'ecran entier : l'espace s'applique quand meme.
/// - Notch : colle au haut de la zone utile (sous une barre en haut).
/// - Le resultat est borne a l'ecran (`screen`) pour qu'une zone utile
///   etroite ou une barre sur le cote ne pousse jamais la bulle dehors.
fn pill_position(
    screen: PxRect,
    work: PxRect,
    scale: f64,
    style: RecorderStyle,
    gap: f64,
) -> (i32, i32) {
    let win_w = (WIDTH * scale).round();
    let win_h = (HEIGHT * scale).round();
    let x = work.x as f64 + (work.w as f64 - win_w) / 2.0;
    let y = match style {
        RecorderStyle::Mini => (work.y + work.h) as f64 - win_h - (gap * scale).round(),
        RecorderStyle::Notch => work.y as f64,
    };
    let max_x = (screen.x + screen.w) as f64 - win_w;
    let max_y = (screen.y + screen.h) as f64 - win_h;
    let x = x.min(max_x).max(screen.x as f64);
    let y = y.min(max_y).max(screen.y as f64);
    (x.round() as i32, y.round() as i32)
}

/// Parlato : place la bulle sur l'ecran sous la souris (sinon l'ecran
/// principal), dans la zone utile de cet ecran. Sur Windows cela tient
/// compte de la barre des taches (n'importe quel bord), sur macOS de la
/// barre de menus, de l'encoche et du Dock.
fn reposition(window: &tauri::WebviewWindow, style: RecorderStyle) {
    use tauri::PhysicalPosition;
    let monitor = window
        .cursor_position()
        .ok()
        .and_then(|p| window.monitor_from_point(p.x, p.y).ok().flatten())
        .or_else(|| window.primary_monitor().ok().flatten());
    let Some(monitor) = monitor else {
        return;
    };
    let scale = monitor.scale_factor();
    let to_rect = |p: &tauri::PhysicalPosition<i32>, s: &tauri::PhysicalSize<u32>| PxRect {
        x: p.x,
        y: p.y,
        w: s.width as i32,
        h: s.height as i32,
    };
    let screen = to_rect(monitor.position(), monitor.size());
    let area = monitor.work_area();
    let work = to_rect(&area.position, &area.size);
    let (x, y) = pill_position(screen, work, scale, style, BOTTOM_PADDING);
    // Position d'abord, pour que la fenetre soit deja sur l'ecran cible
    // (et son echelle) quand on fixe la taille logique.
    let _ = window.set_position(PhysicalPosition::new(x, y));
    let _ = window.set_size(LogicalSize::new(WIDTH, HEIGHT));
    let _ = window.set_position(PhysicalPosition::new(x, y));
}

/// Montre le popover du bouton Mode, ancre sur la bulle. Les ancres sont en
/// pixels logiques relatifs a la fenetre recorder : `anchor_x` = centre de
/// la bulle, `anchor_top` / `anchor_bottom` = bords de la pilule. Mini : le
/// popover se pose au-dessus de la pilule ; notch : en dessous.
#[tauri::command]
pub fn open_recorder_popover(app: AppHandle, anchor_x: f64, anchor_top: f64, anchor_bottom: f64) {
    let Some(recorder) = app.get_webview_window(LABEL) else {
        return;
    };
    let Some(popover) = ensure_popover_window(&app) else {
        return;
    };
    let scale = recorder.scale_factor().unwrap_or(1.0);
    let Ok(pos) = recorder.outer_position() else {
        return;
    };
    let rec_x = pos.x as f64 / scale;
    let rec_y = pos.y as f64 / scale;

    let mut x = rec_x + anchor_x - POPOVER_WIDTH / 2.0;
    let y = match get_style(&app) {
        RecorderStyle::Mini => rec_y + anchor_top - POPOVER_GAP - POPOVER_HEIGHT,
        RecorderStyle::Notch => rec_y + anchor_bottom + POPOVER_GAP,
    };
    // Garde le popover dans l'ecran de la bulle.
    if let Ok(Some(monitor)) = recorder.current_monitor() {
        let mscale = monitor.scale_factor();
        let mx = monitor.position().x as f64 / mscale;
        let mw = monitor.size().width as f64 / mscale;
        x = x.max(mx).min(mx + mw - POPOVER_WIDTH);
    }
    let _ = popover.set_position(LogicalPosition::new(x, y.max(0.0)));
    let _ = popover.set_size(LogicalSize::new(POPOVER_WIDTH, POPOVER_HEIGHT));
    let _ = popover.show();
}

/// Cache le popover (il reste charge pour la prochaine ouverture).
#[tauri::command]
pub fn close_recorder_popover(app: AppHandle) {
    if let Some(popover) = app.get_webview_window(POPOVER_LABEL) {
        let _ = popover.hide();
    }
}

/// Show and focus the main window, optionally navigating to a panel.
/// Callable from any webview (e.g. the mini recorder's Power Mode empty
/// state link). Panel id matches the frontend `View` union.
#[tauri::command]
pub fn show_main_window(app: AppHandle, panel: Option<String>) {
    use tauri::Emitter;
    if let Some(win) = app.get_webview_window("main") {
        let _ = win.show();
        let _ = win.unminimize();
        let _ = win.set_focus();
    }
    if let Some(p) = panel {
        let _ = app.emit("tray:navigate", p);
    }
}

/// Ferme la bulle et son popover.
pub fn close(app: &AppHandle) {
    if let Some(popover) = app.get_webview_window(POPOVER_LABEL) {
        let _ = popover.close();
    }
    if let Some(win) = app.get_webview_window(LABEL) {
        let _ = win.close();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn r(x: i32, y: i32, w: i32, h: i32) -> PxRect {
        PxRect { x, y, w, h }
    }

    const GAP: f64 = 32.0;

    #[test]
    fn taskbar_at_bottom_floats_above_work_area() {
        // 1920x1080 at 100%, taskbar 48 px at the bottom.
        let screen = r(0, 0, 1920, 1080);
        let work = r(0, 0, 1920, 1032);
        let (x, y) = pill_position(screen, work, 1.0, RecorderStyle::Mini, GAP);
        assert_eq!(x, (1920 - 300) / 2);
        // window bottom = 1032 - 32
        assert_eq!(y + 120, 1000);
    }

    #[test]
    fn auto_hidden_taskbar_still_has_the_gap() {
        let screen = r(0, 0, 1920, 1080);
        let (_, y) = pill_position(screen, screen, 1.0, RecorderStyle::Mini, GAP);
        assert_eq!(y + 120, 1080 - 32);
    }

    #[test]
    fn taskbar_on_the_left_shifts_x_only() {
        let screen = r(0, 0, 1920, 1080);
        let work = r(60, 0, 1860, 1080);
        let (x, y) = pill_position(screen, work, 1.0, RecorderStyle::Mini, GAP);
        assert_eq!(x, 60 + (1860 - 300) / 2);
        assert_eq!(y + 120, 1080 - 32);
    }

    #[test]
    fn taskbar_on_the_right_shifts_x_only() {
        let screen = r(0, 0, 1920, 1080);
        let work = r(0, 0, 1860, 1080);
        let (x, y) = pill_position(screen, work, 1.0, RecorderStyle::Mini, GAP);
        assert_eq!(x, (1860 - 300) / 2);
        assert_eq!(y + 120, 1080 - 32);
    }

    #[test]
    fn taskbar_on_top_mini_unchanged_notch_sits_below_it() {
        let screen = r(0, 0, 1920, 1080);
        let work = r(0, 48, 1920, 1032);
        let (_, y) = pill_position(screen, work, 1.0, RecorderStyle::Mini, GAP);
        assert_eq!(y + 120, 1080 - 32);
        let (_, y) = pill_position(screen, work, 1.0, RecorderStyle::Notch, GAP);
        assert_eq!(y, 48);
    }

    #[test]
    fn second_monitor_to_the_right_and_negative_origin() {
        // Secondary monitor left of the primary (negative x) and above it.
        let screen = r(-1920, -200, 1920, 1080);
        let work = r(-1920, -200, 1920, 1032);
        let (x, y) = pill_position(screen, work, 1.0, RecorderStyle::Mini, GAP);
        assert_eq!(x, -1920 + (1920 - 300) / 2);
        assert_eq!(y + 120, -200 + 1032 - 32);
        // Monitor to the right of the primary.
        let screen = r(1920, 0, 2560, 1440);
        let (x, y) = pill_position(screen, screen, 1.0, RecorderStyle::Mini, GAP);
        assert_eq!(x, 1920 + (2560 - 300) / 2);
        assert_eq!(y + 120, 1440 - 32);
    }

    #[test]
    fn high_dpi_scales_size_and_gap() {
        // 200%: 3840x2160 physical, work area loses 96 px at the bottom.
        let screen = r(0, 0, 3840, 2160);
        let work = r(0, 0, 3840, 2064);
        let (x, y) = pill_position(screen, work, 2.0, RecorderStyle::Mini, GAP);
        assert_eq!(x, (3840 - 600) / 2);
        assert_eq!(y + 240, 2064 - 64);
    }

    #[test]
    fn result_never_leaves_the_screen() {
        // Tiny work area: clamp to the screen.
        let screen = r(0, 0, 200, 100);
        let (x, y) = pill_position(screen, screen, 1.0, RecorderStyle::Mini, GAP);
        assert_eq!((x, y), (0, 0));
    }
}
