// "Remove from this computer" : le modele disparait pour toutes les
// applications (Parlato, Talaan, Ensayo) d'un coup, au lieu d'un Delete qui ne
// lache que pour Parlato. L'ordre est celui du Delete : Parlato decharge le
// modele (son `ModelHandle` part avec), puis `SharedStore::remove_everywhere`.
// Si une autre application le garde ouvert, rien n'a change et Parlato
// recharge le modele comme il l'etait.
//
// Les logs ne portent que des ids et des nombres.

use std::io;

use serde::Serialize;
use voice_engine_models::{App, Removed};

use super::store::ModelStore;

/// Ce que la personne voit apres un "Remove from this computer".
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Removal {
    /// Le modele n'est plus sur cet ordinateur.
    Removed,
    /// Idem, mais des anciens fichiers etaient ouverts : ils partent apres un
    /// redemarrage.
    OldCopiesLeft,
    /// Une autre application l'a ouvert (ou le telecharge) : rien n'a change.
    InUse,
}

pub fn removal_of(removed: Removed) -> Removal {
    match removed {
        Removed::InUse => Removal::InUse,
        Removed::Erased { old_copies_left } if old_copies_left > 0 => Removal::OldCopiesLeft,
        // `NotThere` : il n'y avait plus rien, le resultat voulu est atteint.
        Removed::Erased { .. } | Removed::NotThere => Removal::Removed,
    }
}

/// Les noms des AUTRES applications qui utilisent le modele, pour la
/// confirmation ("Also used by Talaan and Ensayo"). Des noms propres : pas
/// traduits.
pub fn other_apps(users: &[App]) -> Vec<String> {
    users
        .iter()
        .filter(|a| **a != App::Parlato)
        .map(|a| {
            match a {
                App::Talaan => "Talaan",
                App::Ensayo => "Ensayo",
                App::Parlato => "Parlato",
            }
            .to_string()
        })
        .collect()
}

/// Decharge (`unload` rend de quoi recharger si Parlato avait le modele en
/// memoire), efface partout, et recharge (`reload`) quand rien n'a ete efface
/// (une autre application l'a ouvert, ou une erreur) pour que Parlato continue
/// de marcher. Bloquant : a lancer sur un fil, pas sur celui de l'interface.
pub fn run<T>(
    store: &ModelStore,
    id: &str,
    unload: impl FnOnce() -> Option<T>,
    reload: impl FnOnce(T),
) -> io::Result<Removal> {
    let was_loaded = unload();
    let result = store.remove_everywhere(id);
    let erased = matches!(&result, Ok(r) if *r != Removed::InUse);
    if !erased {
        if let Some(token) = was_loaded {
            reload(token);
        }
    }
    let removal = result.map(removal_of)?;
    tracing::info!(id, ?removal, "modele retire de cet ordinateur");
    Ok(removal)
}

#[cfg(test)]
mod tests {
    use super::super::store::tests::{install, temp_store};
    use super::*;
    use std::cell::{Cell, RefCell};
    use voice_engine_models::{ModelHandle, SharedStore, Status};

    /// Un autre programme (Talaan) sur le meme dossier partage.
    fn talaan(store: &ModelStore) -> SharedStore {
        SharedStore::open_at(App::Talaan, store.shared().root().to_path_buf()).unwrap()
    }

    #[test]
    fn engine_results_become_what_the_person_sees() {
        assert_eq!(removal_of(Removed::InUse), Removal::InUse);
        assert_eq!(removal_of(Removed::NotThere), Removal::Removed);
        assert_eq!(
            removal_of(Removed::Erased { old_copies_left: 0 }),
            Removal::Removed
        );
        assert_eq!(
            removal_of(Removed::Erased { old_copies_left: 2 }),
            Removal::OldCopiesLeft
        );
    }

    #[test]
    fn other_apps_leaves_parlato_out() {
        assert!(other_apps(&[]).is_empty());
        assert!(other_apps(&[App::Parlato]).is_empty());
        assert_eq!(
            other_apps(&[App::Talaan, App::Ensayo, App::Parlato]),
            ["Talaan", "Ensayo"]
        );
    }

    #[test]
    fn a_model_only_parlato_uses_is_erased() {
        let tmp = tempfile::tempdir().unwrap();
        let store = temp_store(tmp.path());
        install(&store, "whisper-tiny");
        store.shared().use_model("whisper-tiny").unwrap();
        let unloaded = Cell::new(false);
        let reloaded = Cell::new(false);
        let out = run(
            &store,
            "whisper-tiny",
            || {
                unloaded.set(true);
                Some(())
            },
            |()| reloaded.set(true),
        )
        .unwrap();
        assert_eq!(out, Removal::Removed);
        assert!(unloaded.get() && !reloaded.get());
        assert!(!store.shared().model_dir("whisper-tiny").exists());
        assert_eq!(store.shared().status("whisper-tiny"), Status::Missing);
        assert_eq!(store.path_for_id("whisper-tiny"), None);
    }

    #[test]
    fn a_shared_model_goes_for_every_app() {
        let tmp = tempfile::tempdir().unwrap();
        let store = temp_store(tmp.path());
        install(&store, "vad");
        store.shared().use_model("vad").unwrap();
        let other = talaan(&store);
        other.use_model("vad").unwrap();
        // Ce que la confirmation affiche.
        let Status::Installed { users, .. } = store.shared().status("vad") else {
            panic!("installe");
        };
        assert_eq!(other_apps(&users), ["Talaan"]);

        assert_eq!(
            run(&store, "vad", || None::<()>, |()| ()).unwrap(),
            Removal::Removed
        );
        assert!(!store.shared().model_dir("vad").exists());
        assert_eq!(store.shared().status("vad"), Status::Missing);
        assert_eq!(other.status("vad"), Status::Missing);
        // Les deux marqueurs sont partis : rien ne reste a "liberer".
        assert!(!tmp.path().join("shared/.engine/users/vad").exists());
    }

    #[test]
    fn in_use_elsewhere_changes_nothing_and_parlato_reloads() {
        let tmp = tempfile::tempdir().unwrap();
        let store = temp_store(tmp.path());
        install(&store, "vad");
        store.shared().use_model("vad").unwrap();
        let other = talaan(&store);
        other.use_model("vad").unwrap();
        let theirs = other.hold("vad").unwrap();

        // Parlato avait le modele charge : son verrou est dans `mine`.
        let mine: RefCell<Option<ModelHandle>> = RefCell::new(store.hold("vad"));
        let out = run(
            &store,
            "vad",
            || mine.borrow_mut().take().map(|_| ()),
            |()| *mine.borrow_mut() = store.hold("vad"),
        )
        .unwrap();
        assert_eq!(out, Removal::InUse);
        // Rien n'a bouge, et Parlato tient de nouveau le modele.
        assert!(store.shared().model_dir("vad").is_dir());
        assert!(matches!(
            store.shared().status("vad"),
            Status::Installed {
                used_by_me: true,
                ..
            }
        ));
        assert!(store.path_for_id("vad").is_some());
        assert!(mine.borrow().is_some());

        // Une fois l'autre application partie, le retrait passe.
        drop(theirs);
        drop(mine.borrow_mut().take());
        assert_eq!(
            run(&store, "vad", || None::<()>, |()| ()).unwrap(),
            Removal::Removed
        );
        assert!(!store.shared().model_dir("vad").exists());
    }

    #[test]
    fn parlatos_own_handle_must_be_dropped_first() {
        // Le moteur compte le verrou de Parlato aussi : sans decharger, InUse.
        let tmp = tempfile::tempdir().unwrap();
        let store = temp_store(tmp.path());
        install(&store, "vad");
        store.shared().use_model("vad").unwrap();
        let handle = store.hold("vad").unwrap();
        assert_eq!(
            run(&store, "vad", || None::<()>, |()| ()).unwrap(),
            Removal::InUse
        );
        drop(handle);
        assert_eq!(
            run(&store, "vad", || None::<()>, |()| ()).unwrap(),
            Removal::Removed
        );
    }

    #[test]
    fn a_download_elsewhere_counts_as_in_use() {
        let tmp = tempfile::tempdir().unwrap();
        let store = temp_store(tmp.path());
        install(&store, "vad");
        store.shared().use_model("vad").unwrap();
        let lock = std::fs::OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(false)
            .open(tmp.path().join("shared/.engine/locks/vad.download"))
            .unwrap();
        lock.lock().unwrap();
        assert_eq!(
            run(&store, "vad", || None::<()>, |()| ()).unwrap(),
            Removal::InUse
        );
        assert!(store.shared().model_dir("vad").is_dir());
    }

    #[test]
    fn an_unknown_id_is_refused_and_nothing_reloads() {
        let tmp = tempfile::tempdir().unwrap();
        let store = temp_store(tmp.path());
        let reloaded = Cell::new(false);
        let err = run(&store, "../x", || Some(()), |()| reloaded.set(true)).unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::InvalidInput);
        // L'erreur n'a rien efface : Parlato recharge ce qu'il avait decharge.
        assert!(reloaded.get());
    }

    /// Des anciens fichiers qu'on ne peut pas effacer (dossier en lecture
    /// seule, comme un fichier ouvert sous Windows) : retire, avec l'avis.
    #[cfg(unix)]
    #[test]
    fn old_files_that_cannot_be_deleted_are_reported() {
        use std::os::unix::fs::PermissionsExt;
        let tmp = tempfile::tempdir().unwrap();
        let local = tmp.path().join("local");
        let shared_dir = tmp.path().join("shared");
        let shared =
            SharedStore::for_move_test(App::Parlato, &shared_dir, &local, None, false).unwrap();
        let store = ModelStore::new(shared, tmp.path().join("app"));
        install(&store, "vad");
        store.shared().use_model("vad").unwrap();
        // L'ancien fichier de Talaan (disposition du Mac), a la taille du catalogue.
        let model = voice_engine_models::catalog::find("vad").unwrap();
        let folder = local.join("Talaan/models/vad");
        std::fs::create_dir_all(&folder).unwrap();
        let file = &model.files[0];
        std::fs::File::create(folder.join(file.name))
            .unwrap()
            .set_len(file.size)
            .unwrap();
        std::fs::set_permissions(&folder, std::fs::Permissions::from_mode(0o555)).unwrap();

        let out = run(&store, "vad", || None::<()>, |()| ()).unwrap();
        std::fs::set_permissions(&folder, std::fs::Permissions::from_mode(0o755)).unwrap();
        assert_eq!(out, Removal::OldCopiesLeft);
        assert!(!store.shared().model_dir("vad").exists());
    }
}
