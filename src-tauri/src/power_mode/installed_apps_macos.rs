// Installed apps list on macOS, for the "Add app" picker of Power modes.
// Two sources, merged by bundle identifier (the macOS equivalent of the exe
// name that Power Mode matches):
//  - the .app bundles in the usual application folders, whose Info.plist
//    gives the bundle identifier and the name;
//  - the applications running right now (NSWorkspace), for apps kept
//    somewhere else.
//
// Reference VoiceInk : PowerMode/AppPicker lists the .app bundles in
// /Applications (NSWorkspace).

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::Serialize;

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct InstalledApp {
    /// Display name.
    pub name: String,
    /// What Power Mode matches: the bundle identifier, in lower case.
    pub exe_name: String,
    /// The app is running right now.
    pub running: bool,
}

/// Parlato itself is not an app to pair a mode with.
const IGNORED_BUNDLES: &[&str] = &["com.craftconceptsdigital.parlato"];

/// Where apps live. Folders that are not apps (for example a vendor folder in
/// /Applications) are searched one level deep.
const APP_FOLDERS: &[&str] = &[
    "/Applications",
    "/Applications/Utilities",
    "/System/Applications",
    "/System/Applications/Utilities",
];

/// Sorted by name. Blocking (disk scan): call it off the UI thread.
pub fn list() -> Vec<InstalledApp> {
    let mut by_bundle: BTreeMap<String, InstalledApp> = BTreeMap::new();

    for (name, exe_name) in installed_bundles() {
        by_bundle.entry(exe_name.clone()).or_insert(InstalledApp {
            name,
            exe_name,
            running: false,
        });
    }
    for (name, exe_name) in running_apps() {
        by_bundle
            .entry(exe_name.clone())
            .and_modify(|a| a.running = true)
            .or_insert(InstalledApp {
                name,
                exe_name,
                running: true,
            });
    }

    let mut apps: Vec<InstalledApp> = by_bundle
        .into_values()
        .filter(|a| !IGNORED_BUNDLES.contains(&a.exe_name.as_str()))
        .collect();
    apps.sort_by_key(|a| a.name.to_lowercase());
    apps
}

/// (name, lower-case bundle identifier) of every .app found in the folders.
fn installed_bundles() -> Vec<(String, String)> {
    let mut folders: Vec<PathBuf> = APP_FOLDERS.iter().map(PathBuf::from).collect();
    if let Some(home) = std::env::var_os("HOME") {
        folders.push(Path::new(&home).join("Applications"));
    }
    let mut out = Vec::new();
    for folder in folders {
        collect_apps(&folder, 1, &mut out);
    }
    out
}

fn collect_apps(folder: &Path, depth: u8, out: &mut Vec<(String, String)>) {
    let Ok(entries) = std::fs::read_dir(folder) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().is_some_and(|e| e == "app") {
            if let Some(found) = read_bundle(&path) {
                out.push(found);
            }
        } else if depth > 0 && path.is_dir() {
            collect_apps(&path, depth - 1, out);
        }
    }
}

fn read_bundle(app: &Path) -> Option<(String, String)> {
    let info = plist::Value::from_file(app.join("Contents").join("Info.plist")).ok()?;
    let fallback = app.file_stem()?.to_string_lossy().to_string();
    bundle_info(&info, &fallback)
}

/// Name and lower-case bundle identifier from an Info.plist, or None for a
/// bundle without an identifier and for background-only apps (helpers and
/// agents with no window, which nobody would pair a mode with).
fn bundle_info(info: &plist::Value, fallback_name: &str) -> Option<(String, String)> {
    let dict = info.as_dictionary()?;
    let text = |key: &str| {
        dict.get(key)
            .and_then(|v| v.as_string())
            .map(str::trim)
            .filter(|s| !s.is_empty())
    };
    let flag = |key: &str| dict.get(key).and_then(|v| v.as_boolean()).unwrap_or(false);
    if flag("LSUIElement") || flag("LSBackgroundOnly") {
        return None;
    }
    let id = text("CFBundleIdentifier")?.to_lowercase();
    let name = text("CFBundleDisplayName")
        .or_else(|| text("CFBundleName"))
        .unwrap_or(fallback_name);
    Some((name.to_string(), id))
}

/// Apps with a Dock icon that are running now.
fn running_apps() -> Vec<(String, String)> {
    use objc2_app_kit::{NSApplicationActivationPolicy, NSWorkspace};
    NSWorkspace::sharedWorkspace()
        .runningApplications()
        .to_vec()
        .into_iter()
        .filter(|app| app.activationPolicy() == NSApplicationActivationPolicy::Regular)
        .filter_map(|app| {
            let id = app.bundleIdentifier()?.to_string().to_lowercase();
            if id.is_empty() {
                return None;
            }
            let name = app
                .localizedName()
                .map(|n| n.to_string())
                .unwrap_or_else(|| id.clone());
            Some((name, id))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(xml: &str) -> plist::Value {
        plist::Value::from_reader_xml(xml.as_bytes()).unwrap()
    }

    const HEAD: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>"#;
    const TAIL: &str = "</dict></plist>";

    #[test]
    fn bundle_info_prefers_display_name_and_lowercases_the_id() {
        let info = parse(&format!(
            "{HEAD}<key>CFBundleIdentifier</key><string>com.Apple.Mail</string>\
             <key>CFBundleName</key><string>Mail</string>\
             <key>CFBundleDisplayName</key><string>Apple Mail</string>{TAIL}"
        ));
        assert_eq!(
            bundle_info(&info, "Mail"),
            Some(("Apple Mail".into(), "com.apple.mail".into()))
        );
    }

    #[test]
    fn bundle_info_falls_back_to_bundle_name_then_folder_name() {
        let named = parse(&format!(
            "{HEAD}<key>CFBundleIdentifier</key><string>com.example.a</string>\
             <key>CFBundleName</key><string>Alpha</string>{TAIL}"
        ));
        assert_eq!(
            bundle_info(&named, "folder"),
            Some(("Alpha".into(), "com.example.a".into()))
        );
        let bare = parse(&format!(
            "{HEAD}<key>CFBundleIdentifier</key><string>com.example.b</string>{TAIL}"
        ));
        assert_eq!(
            bundle_info(&bare, "Beta"),
            Some(("Beta".into(), "com.example.b".into()))
        );
    }

    #[test]
    fn bundle_info_skips_background_apps_and_missing_ids() {
        let agent = parse(&format!(
            "{HEAD}<key>CFBundleIdentifier</key><string>com.example.helper</string>\
             <key>LSUIElement</key><true/>{TAIL}"
        ));
        assert_eq!(bundle_info(&agent, "Helper"), None);
        let no_id = parse(&format!("{HEAD}<key>CFBundleName</key><string>X</string>{TAIL}"));
        assert_eq!(bundle_info(&no_id, "X"), None);
    }

    #[test]
    fn list_returns_sorted_apps_with_lower_case_ids() {
        let apps = list();
        assert!(apps
            .iter()
            .all(|a| !a.exe_name.is_empty() && a.exe_name == a.exe_name.to_lowercase()));
        assert!(apps
            .windows(2)
            .all(|w| w[0].name.to_lowercase() <= w[1].name.to_lowercase()));
        assert!(apps.iter().all(|a| a.exe_name != "com.craftconceptsdigital.parlato"));
    }
}
