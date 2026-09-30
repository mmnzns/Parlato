// Parlato : liste des applications de l'utilisateur, pour appairer un Power
// Mode sans connaitre le nom de l'exe. Deux sources, fusionnees par exe :
//  - les raccourcis du menu Demarrer (tous les utilisateurs + l'utilisateur
//    courant), dont on lit la cible .exe via IShellLinkW ;
//  - les fenetres ouvertes en ce moment, pour les applications absentes du
//    menu Demarrer (nouvel Outlook, Teams, apps portables...).
//
// Reference VoiceInk : AppPicker liste les .app de /Applications
// (NSWorkspace). Pas d'equivalent direct sous Windows.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::Serialize;
use windows::core::{Interface, BOOL, HSTRING, PCWSTR};
use windows::Win32::Foundation::{HWND, LPARAM};
use windows::Win32::Storage::FileSystem::{
    GetFileVersionInfoSizeW, GetFileVersionInfoW, VerQueryValueW,
};
use windows::Win32::System::Com::{
    CoCreateInstance, CoInitializeEx, IPersistFile, CLSCTX_INPROC_SERVER, COINIT_APARTMENTTHREADED,
    STGM_READ,
};
use windows::Win32::UI::Shell::{IShellLinkW, ShellLink};
use windows::Win32::UI::WindowsAndMessaging::{
    EnumWindows, GetWindow, GetWindowLongW, GetWindowTextLengthW, GetWindowThreadProcessId,
    IsWindowVisible, GWL_EXSTYLE, GW_OWNER, WS_EX_TOOLWINDOW,
};

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct InstalledApp {
    /// Nom affiche (nom du raccourci ou description de l'exe).
    pub name: String,
    /// Nom de l'exe sans extension, en minuscules : ce que matche le Power Mode.
    pub exe_name: String,
    /// Une fenetre de cette application est ouverte en ce moment.
    pub running: bool,
}

/// Processus systeme ou hotes qui ont des fenetres mais ne sont pas des
/// applications a appairer.
const IGNORED_EXES: &[&str] = &[
    "parla",
    "explorer",
    "applicationframehost",
    "textinputhost",
    "shellexperiencehost",
    "startmenuexperiencehost",
    "searchhost",
    "lockapp",
    "systemsettings",
];

/// Raccourcis du menu Demarrer qui ne sont pas l'application elle-meme.
const IGNORED_SHORTCUT_WORDS: &[&str] = &[
    "uninstall",
    "desinstaller",
    "readme",
    "release notes",
    "documentation",
    "help",
    "website",
    "update",
    "installer",
    "error report",
    "bug report",
    "tutorial",
];

/// Liste triee par nom. Bloquant (COM + parcours disque) : a appeler hors du
/// thread UI.
pub fn list() -> Vec<InstalledApp> {
    let mut by_exe: BTreeMap<String, InstalledApp> = BTreeMap::new();

    for (name, exe_name) in start_menu_apps() {
        by_exe.entry(exe_name.clone()).or_insert(InstalledApp {
            name,
            exe_name,
            running: false,
        });
    }
    for (name, exe_name) in running_apps() {
        by_exe
            .entry(exe_name.clone())
            .and_modify(|a| a.running = true)
            .or_insert(InstalledApp {
                name,
                exe_name,
                running: true,
            });
    }

    let mut apps: Vec<InstalledApp> = by_exe
        .into_values()
        .filter(|a| !IGNORED_EXES.contains(&a.exe_name.as_str()))
        .collect();
    apps.sort_by_key(|a| a.name.to_lowercase());
    apps
}

fn exe_stem(path: &Path) -> Option<String> {
    let is_exe = path
        .extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| e.eq_ignore_ascii_case("exe"));
    if !is_exe {
        return None;
    }
    path.file_stem()
        .and_then(|s| s.to_str())
        .map(|s| s.to_ascii_lowercase())
}

/// Exe reellement lance par un raccourci. Les apps Squirrel (Discord, Slack,
/// Teams classique...) passent par "Update.exe --processStart Discord.exe" :
/// on garde l'exe demarre, pas le lanceur.
fn launched_exe(target: &Path, args: &str) -> Option<String> {
    let exe = exe_stem(target)?;
    if exe == "update" {
        let mut parts = args.split_whitespace();
        while let Some(p) = parts.next() {
            if p.eq_ignore_ascii_case("--processStart") {
                return parts
                    .next()
                    .and_then(|n| exe_stem(Path::new(n.trim_matches('"'))));
            }
        }
        return None;
    }
    Some(exe)
}

fn wide_to_string(buf: &[u16]) -> String {
    let len = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
    String::from_utf16_lossy(&buf[..len])
}

fn starts_with_ci(path: &Path, prefix: &Path) -> bool {
    let p = path.to_string_lossy().to_lowercase();
    let pre = prefix.to_string_lossy().to_lowercase();
    p.starts_with(&pre)
}

fn start_menu_dirs() -> Vec<PathBuf> {
    ["ProgramData", "APPDATA"]
        .iter()
        .filter_map(std::env::var_os)
        .map(|base| PathBuf::from(base).join(r"Microsoft\Windows\Start Menu\Programs"))
        .collect()
}

fn collect_shortcuts(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_shortcuts(&path, out);
        } else if path
            .extension()
            .and_then(|e| e.to_str())
            .is_some_and(|e| e.eq_ignore_ascii_case("lnk"))
        {
            out.push(path);
        }
    }
}

/// (nom affiche, exe) de chaque raccourci du menu Demarrer qui pointe vers un .exe.
fn start_menu_apps() -> Vec<(String, String)> {
    let mut shortcuts = Vec::new();
    for dir in start_menu_dirs() {
        collect_shortcuts(&dir, &mut shortcuts);
    }

    // S_FALSE (deja initialise sur ce thread) est un succes ; en cas d'echec
    // on renonce au menu Demarrer, les fenetres ouvertes restent listees.
    if unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED) }.is_err() {
        return Vec::new();
    }
    let Ok(link) =
        (unsafe { CoCreateInstance::<_, IShellLinkW>(&ShellLink, None, CLSCTX_INPROC_SERVER) })
    else {
        return Vec::new();
    };
    let Ok(file) = link.cast::<IPersistFile>() else {
        return Vec::new();
    };

    let windows_dir = std::env::var_os("WINDIR").map(PathBuf::from);
    let mut out = Vec::new();
    for lnk in shortcuts {
        let Some(name) = lnk.file_stem().and_then(|s| s.to_str()).map(str::to_string) else {
            continue;
        };
        let lower = name.to_lowercase();
        if IGNORED_SHORTCUT_WORDS.iter().any(|w| lower.contains(w)) {
            continue;
        }
        let path = HSTRING::from(lnk.as_os_str());
        if unsafe { file.Load(PCWSTR(path.as_ptr()), STGM_READ) }.is_err() {
            continue;
        }
        let mut buf = [0u16; 260];
        if unsafe { link.GetPath(&mut buf, std::ptr::null_mut(), 0) }.is_err() {
            continue;
        }
        let target = PathBuf::from(wide_to_string(&buf));
        // Les outils de Windows (Editeur du Registre, Nettoyage de disque...)
        // ne sont pas des apps ou l'on dicte ; ouverts, ils restent listes
        // via les fenetres ouvertes.
        if windows_dir
            .as_ref()
            .is_some_and(|w| starts_with_ci(&target, w))
        {
            continue;
        }
        let mut args = [0u16; 1024];
        let args = match unsafe { link.GetArguments(&mut args) } {
            Ok(()) => wide_to_string(&args),
            Err(_) => String::new(),
        };
        if let Some(exe_name) = launched_exe(&target, &args) {
            out.push((name, exe_name));
        }
    }
    out
}

/// (nom affiche, exe) des applications qui ont une fenetre principale visible.
fn running_apps() -> Vec<(String, String)> {
    let mut pids: Vec<u32> = Vec::new();
    unsafe {
        let _ = EnumWindows(
            Some(collect_window_pid),
            LPARAM(&mut pids as *mut Vec<u32> as isize),
        );
    }
    pids.sort_unstable();
    pids.dedup();

    let mut out = Vec::new();
    for pid in pids {
        let Ok(path) = (unsafe { super::active_window::process_image_path(pid) }) else {
            continue;
        };
        let Some(exe_name) = exe_stem(&path) else {
            continue;
        };
        let name = file_description(&path).unwrap_or_else(|| exe_name.clone());
        out.push((name, exe_name));
    }
    out
}

/// Fenetre de premier niveau visible, avec un titre, sans proprietaire et
/// qui n'est pas une palette d'outils : ce que la barre des taches montre.
unsafe extern "system" fn collect_window_pid(hwnd: HWND, lparam: LPARAM) -> BOOL {
    unsafe {
        let pids = &mut *(lparam.0 as *mut Vec<u32>);
        let is_tool = (GetWindowLongW(hwnd, GWL_EXSTYLE) as u32 & WS_EX_TOOLWINDOW.0) != 0;
        let has_owner = GetWindow(hwnd, GW_OWNER).is_ok_and(|o| !o.0.is_null());
        if IsWindowVisible(hwnd).as_bool()
            && !is_tool
            && !has_owner
            && GetWindowTextLengthW(hwnd) > 0
        {
            let mut pid = 0u32;
            GetWindowThreadProcessId(hwnd, Some(&mut pid));
            if pid != 0 {
                pids.push(pid);
            }
        }
    }
    BOOL(1)
}

/// "FileDescription" de l'exe (ex "Microsoft Outlook"), en passant par la
/// premiere langue declaree dans ses informations de version.
fn file_description(path: &Path) -> Option<String> {
    let file = HSTRING::from(path.as_os_str());
    unsafe {
        let size = GetFileVersionInfoSizeW(PCWSTR(file.as_ptr()), None);
        if size == 0 {
            return None;
        }
        let mut data = vec![0u8; size as usize];
        GetFileVersionInfoW(PCWSTR(file.as_ptr()), None, size, data.as_mut_ptr().cast()).ok()?;

        let mut ptr = std::ptr::null_mut();
        let mut len = 0u32;
        let key = HSTRING::from(r"\VarFileInfo\Translation");
        if !VerQueryValueW(
            data.as_ptr().cast(),
            PCWSTR(key.as_ptr()),
            &mut ptr,
            &mut len,
        )
        .as_bool()
            || len < 4
        {
            return None;
        }
        let pair = std::slice::from_raw_parts(ptr as *const u16, 2);
        let key = HSTRING::from(format!(
            r"\StringFileInfo\{:04x}{:04x}\FileDescription",
            pair[0], pair[1]
        ));
        if !VerQueryValueW(
            data.as_ptr().cast(),
            PCWSTR(key.as_ptr()),
            &mut ptr,
            &mut len,
        )
        .as_bool()
            || len == 0
        {
            return None;
        }
        let text = std::slice::from_raw_parts(ptr as *const u16, len as usize);
        let end = text.iter().position(|&c| c == 0).unwrap_or(text.len());
        let s = String::from_utf16_lossy(&text[..end]).trim().to_string();
        (!s.is_empty()).then_some(s)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn squirrel_shortcuts_resolve_to_the_real_app() {
        let upd = Path::new(r"C:\Users\a\AppData\Local\Discord\Update.exe");
        assert_eq!(
            launched_exe(upd, "--processStart Discord.exe").as_deref(),
            Some("discord")
        );
        assert_eq!(
            launched_exe(upd, "--processStart \"slack.exe\"").as_deref(),
            Some("slack")
        );
        assert_eq!(launched_exe(upd, ""), None);
        assert_eq!(
            launched_exe(Path::new(r"C:\x\zoom.exe"), "").as_deref(),
            Some("zoom")
        );
    }

    #[test]
    fn exe_stem_keeps_only_exes() {
        assert_eq!(
            exe_stem(Path::new(r"C:\Program Files\App\Outlook.EXE")).as_deref(),
            Some("outlook")
        );
        assert_eq!(exe_stem(Path::new(r"C:\docs\manual.pdf")), None);
    }

    /// Lists this PC's apps: `cargo test --lib installed_apps_smoke -- --ignored --nocapture`.
    #[test]
    #[ignore]
    fn installed_apps_smoke() {
        let apps = list();
        for a in &apps {
            println!(
                "{:<40} {:<25} {}",
                a.name,
                a.exe_name,
                if a.running { "open" } else { "" }
            );
        }
        println!("--- {} apps", apps.len());
        assert!(!apps.is_empty());
    }
}
