// Models the person brought themselves, which are not in the engine catalog.
//
// - Whisper: `Models/imported/<stem>.bin`, id `imported:<stem>` (unchanged).
// - Local AI (GGUF): every `*.gguf` directly in `LlmModels/` (Parlato's own
//   folder: models the catalog dropped, and ones put there by hand) and in
//   `LlmModels/imported/`. The id is `voice_engine_models::imports::import_id`
//   (it survives a rename), the name is the file stem. They load by path and
//   are never moved. Delete only removes a file in `LlmModels/imported/`.
//
// Everything here takes folders as arguments, so the tests use temp folders.

use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{anyhow, Context, Result};
use tracing::info;

/// The file name Parlato 0.14 gave the catalog's Granite model. The engine
/// moves it into the shared folder and leaves a hard link under this name, so
/// it is not an import.
pub const LEGACY_GRANITE_FILE: &str = "granite-4.2-3b-q4.gguf";

pub const WHISPER_IMPORT_PREFIX: &str = "imported:";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImportedFile {
    pub id: String,
    pub stem: String,
    pub path: PathBuf,
    pub size: u64,
    /// True for a file in `imported/`, which Delete removes.
    pub removable: bool,
}

fn has_extension(path: &Path, ext: &str) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| e.eq_ignore_ascii_case(ext))
}

fn files_with_extension(dir: &Path, ext: &str) -> Vec<PathBuf> {
    let Ok(entries) = fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut out: Vec<PathBuf> = entries
        .filter_map(Result::ok)
        .filter(|e| e.file_type().is_ok_and(|t| t.is_file()))
        .map(|e| e.path())
        .filter(|p| has_extension(p, ext))
        .collect();
    out.sort();
    out
}

// -- Whisper ------------------------------------------------------------------

pub fn whisper_imported_dir(models_dir: &Path) -> PathBuf {
    models_dir.join("imported")
}

pub fn list_whisper_imports(models_dir: &Path) -> Vec<ImportedFile> {
    files_with_extension(&whisper_imported_dir(models_dir), "bin")
        .into_iter()
        .filter_map(|path| {
            let stem = path.file_stem()?.to_str()?.to_string();
            if stem.is_empty() {
                return None;
            }
            let size = path.metadata().ok()?.len();
            Some(ImportedFile {
                id: format!("{WHISPER_IMPORT_PREFIX}{stem}"),
                stem,
                path,
                size,
                removable: true,
            })
        })
        .collect()
}

pub fn whisper_import_path(models_dir: &Path, id: &str) -> Option<PathBuf> {
    let stem = id.strip_prefix(WHISPER_IMPORT_PREFIX)?;
    if stem.is_empty() || stem.contains(['/', '\\']) || stem.contains("..") {
        return None;
    }
    let path = whisper_imported_dir(models_dir).join(format!("{stem}.bin"));
    path.is_file().then_some(path)
}

/// Copies an external `.bin` into `Models/imported/`. Returns the model id.
pub fn import_whisper(models_dir: &Path, source: &Path) -> Result<String> {
    if !has_extension(source, "bin") {
        anyhow::bail!("le fichier doit avoir l'extension .bin");
    }
    if !source.exists() {
        anyhow::bail!("fichier introuvable: {}", source.display());
    }
    let stem = source
        .file_stem()
        .and_then(|s| s.to_str())
        .ok_or_else(|| anyhow!("nom de fichier invalide"))?;
    let dir = whisper_imported_dir(models_dir);
    fs::create_dir_all(&dir).ok();
    let target = dir.join(format!("{stem}.bin"));
    if target.exists() {
        anyhow::bail!("un modele portant ce nom existe deja: {}", target.display());
    }
    fs::copy(source, &target).with_context(|| format!("copie vers {}", target.display()))?;
    info!(target = %target.display(), "Modele Whisper importe");
    Ok(format!("{WHISPER_IMPORT_PREFIX}{stem}"))
}

pub fn delete_whisper_import(models_dir: &Path, id: &str) -> Result<()> {
    if !id.starts_with(WHISPER_IMPORT_PREFIX) {
        anyhow::bail!("id invalide (doit commencer par imported:)");
    }
    if let Some(path) = whisper_import_path(models_dir, id) {
        fs::remove_file(&path)?;
        info!(id, "Modele importe supprime");
    }
    Ok(())
}

// -- Local AI (GGUF) ----------------------------------------------------------

pub fn gguf_imported_dir(llm_dir: &Path) -> PathBuf {
    llm_dir.join("imported")
}

/// Every non-catalog `.gguf` in `LlmModels/` and `LlmModels/imported/`, in
/// that order. Two files with the same content (same import id) show once.
pub fn list_gguf_imports(llm_dir: &Path) -> Vec<ImportedFile> {
    let mut out: Vec<ImportedFile> = Vec::new();
    let folders = [
        (llm_dir.to_path_buf(), false),
        (gguf_imported_dir(llm_dir), true),
    ];
    for (dir, removable) in folders {
        for path in files_with_extension(&dir, "gguf") {
            let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
                continue;
            };
            if !removable && name == LEGACY_GRANITE_FILE {
                continue;
            }
            let Some(stem) = path.file_stem().and_then(|s| s.to_str()).map(String::from) else {
                continue;
            };
            let Ok(id) = voice_engine_models::imports::import_id(&path) else {
                continue;
            };
            if out.iter().any(|f| f.id == id) {
                continue;
            }
            let size = path.metadata().map(|m| m.len()).unwrap_or(0);
            out.push(ImportedFile {
                id,
                stem,
                path,
                size,
                removable,
            });
        }
    }
    out
}

pub fn find_gguf_import(llm_dir: &Path, id: &str) -> Option<ImportedFile> {
    if !id.starts_with("import-") {
        return None;
    }
    list_gguf_imports(llm_dir).into_iter().find(|f| f.id == id)
}

/// The import id of `<stem>.gguf` in `imported/` first, then in `LlmModels/`
/// (what the id migration of a saved choice needs).
pub fn import_id_of_stem(llm_dir: &Path, stem: &str) -> Option<String> {
    if stem.is_empty() || stem.contains(['/', '\\']) || stem.contains("..") {
        return None;
    }
    [gguf_imported_dir(llm_dir), llm_dir.to_path_buf()]
        .into_iter()
        .map(|d| d.join(format!("{stem}.gguf")))
        .find(|p| p.is_file())
        .and_then(|p| voice_engine_models::imports::import_id(&p).ok())
}

/// Copies an external `.gguf` into `LlmModels/imported/`. Returns its import id.
pub fn import_gguf(llm_dir: &Path, source: &Path) -> Result<String> {
    if !has_extension(source, "gguf") {
        anyhow::bail!("le fichier doit avoir l'extension .gguf");
    }
    if !source.exists() {
        anyhow::bail!("fichier introuvable: {}", source.display());
    }
    let stem = source
        .file_stem()
        .and_then(|s| s.to_str())
        .ok_or_else(|| anyhow!("nom de fichier invalide"))?;
    let dir = gguf_imported_dir(llm_dir);
    fs::create_dir_all(&dir).ok();
    let target = dir.join(format!("{stem}.gguf"));
    if target.exists() {
        anyhow::bail!("un modele existe deja: {}", target.display());
    }
    fs::copy(source, &target).with_context(|| format!("copie vers {}", target.display()))?;
    info!(target = %target.display(), "GGUF importe");
    voice_engine_models::imports::import_id(&target)
        .map_err(|e| anyhow!("empreinte du fichier: {e}"))
}

/// Removes an imported file. Only a file in `LlmModels/imported/` is removed;
/// one elsewhere is left where the person put it. Returns whether a file was
/// removed.
pub fn delete_gguf_import(llm_dir: &Path, id: &str) -> Result<bool> {
    match find_gguf_import(llm_dir, id) {
        Some(file) if file.removable => {
            fs::remove_file(&file.path)?;
            info!("GGUF importe supprime");
            Ok(true)
        }
        _ => Ok(false),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::File;
    use std::io::Write;

    fn write(path: &Path, bytes: &[u8]) {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        File::create(path).unwrap().write_all(bytes).unwrap();
    }

    #[test]
    fn gguf_imports_skip_the_legacy_granite_link() {
        let dir = tempfile::tempdir().unwrap();
        let llm = dir.path();
        write(&llm.join(LEGACY_GRANITE_FILE), b"granite");
        write(&llm.join("qwen2.5-3b-instruct-q4.gguf"), b"qwen");
        write(&llm.join("notes.txt"), b"not a model");
        write(&llm.join("imported").join("mine.gguf"), b"mine");
        let list = list_gguf_imports(llm);
        let stems: Vec<_> = list.iter().map(|f| f.stem.as_str()).collect();
        assert_eq!(stems, ["qwen2.5-3b-instruct-q4", "mine"]);
        assert!(!list[0].removable);
        assert!(list[1].removable);
        assert!(list
            .iter()
            .all(|f| f.id.starts_with("import-") && f.id.len() == 19));
    }

    #[test]
    fn a_granite_file_inside_imported_is_an_import() {
        let dir = tempfile::tempdir().unwrap();
        write(
            &dir.path().join("imported").join(LEGACY_GRANITE_FILE),
            b"mine",
        );
        assert_eq!(list_gguf_imports(dir.path()).len(), 1);
    }

    #[test]
    fn import_id_survives_a_rename_and_duplicates_show_once() {
        let dir = tempfile::tempdir().unwrap();
        write(&dir.path().join("a.gguf"), b"same bytes");
        let before = list_gguf_imports(dir.path());
        fs::rename(dir.path().join("a.gguf"), dir.path().join("b.gguf")).unwrap();
        let after = list_gguf_imports(dir.path());
        assert_eq!(before[0].id, after[0].id);
        assert_eq!(after[0].stem, "b");
        write(&dir.path().join("imported").join("c.gguf"), b"same bytes");
        assert_eq!(list_gguf_imports(dir.path()).len(), 1);
    }

    #[test]
    fn missing_folders_list_nothing() {
        let dir = tempfile::tempdir().unwrap();
        assert!(list_gguf_imports(&dir.path().join("none")).is_empty());
        assert!(list_whisper_imports(&dir.path().join("none")).is_empty());
    }

    #[test]
    fn find_and_stem_lookup() {
        let dir = tempfile::tempdir().unwrap();
        let llm = dir.path();
        write(&llm.join("gemma-2-2b-it-q4.gguf"), b"gemma");
        write(&llm.join("imported").join("mine.gguf"), b"mine");
        let id = import_id_of_stem(llm, "gemma-2-2b-it-q4").unwrap();
        let found = find_gguf_import(llm, &id).unwrap();
        assert_eq!(found.path, llm.join("gemma-2-2b-it-q4.gguf"));
        assert!(import_id_of_stem(llm, "mine").is_some());
        assert!(import_id_of_stem(llm, "missing").is_none());
        assert!(import_id_of_stem(llm, "../x").is_none());
        assert!(find_gguf_import(llm, "granite-4.2-3b").is_none());
    }

    #[test]
    fn delete_only_removes_files_in_imported() {
        let dir = tempfile::tempdir().unwrap();
        let llm = dir.path();
        write(&llm.join("kept.gguf"), b"kept");
        write(&llm.join("imported").join("gone.gguf"), b"gone");
        let kept = import_id_of_stem(llm, "kept").unwrap();
        let gone = import_id_of_stem(llm, "gone").unwrap();
        assert!(!delete_gguf_import(llm, &kept).unwrap());
        assert!(llm.join("kept.gguf").exists());
        assert!(delete_gguf_import(llm, &gone).unwrap());
        assert!(!llm.join("imported").join("gone.gguf").exists());
    }

    #[test]
    fn import_copies_into_imported_and_refuses_duplicates() {
        let dir = tempfile::tempdir().unwrap();
        let src = dir.path().join("src").join("model.gguf");
        write(&src, b"model");
        let llm = dir.path().join("llm");
        let id = import_gguf(&llm, &src).unwrap();
        assert_eq!(
            find_gguf_import(&llm, &id).unwrap().path,
            llm.join("imported/model.gguf")
        );
        assert!(import_gguf(&llm, &src).is_err());
        let txt = dir.path().join("x.txt");
        write(&txt, b"x");
        assert!(import_gguf(&llm, &txt).is_err());
    }

    #[test]
    fn whisper_imports_list_import_and_delete() {
        let dir = tempfile::tempdir().unwrap();
        let models = dir.path();
        let src = dir.path().join("src").join("custom.bin");
        write(&src, b"whisper");
        assert_eq!(import_whisper(models, &src).unwrap(), "imported:custom");
        assert!(import_whisper(models, &src).is_err());
        let list = list_whisper_imports(models);
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].id, "imported:custom");
        assert!(whisper_import_path(models, "imported:custom").is_some());
        assert!(whisper_import_path(models, "imported:../custom").is_none());
        delete_whisper_import(models, "imported:custom").unwrap();
        assert!(list_whisper_imports(models).is_empty());
        assert!(delete_whisper_import(models, "whisper-tiny").is_err());
    }
}
