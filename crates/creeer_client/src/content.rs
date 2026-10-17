//! The client's data packs (F7.1, ADR-0018): found once at start (`--data`, `--mods`, the
//! environment, beside the executable, the source tree) and replaced when a hot-reload
//! succeeds. Everything the sandboxes load comes from [`data`].

use creeer_data::{Data, Location, Watcher};
use std::sync::{Arc, Mutex, PoisonError, RwLock};

static LOCATION: Mutex<Option<Location>> = Mutex::new(None);
static DATA: RwLock<Option<Arc<Data>>> = RwLock::new(None);

/// Finds the packs from the command line `args` and opens them.
///
/// # Errors
/// If a pack is broken (the message names it).
pub fn init(args: &[String]) -> Result<(), String> {
    let location = Location::find(args);
    let data = location.open().map_err(|e| e.to_string())?;
    *LOCATION.lock().unwrap_or_else(PoisonError::into_inner) = Some(location);
    *DATA.write().unwrap_or_else(PoisonError::into_inner) = Some(Arc::new(data));
    Ok(())
}

/// The current packs (the source tree's base pack if [`init`] was not called: tests).
pub fn data() -> Arc<Data> {
    let current = DATA.read().unwrap_or_else(PoisonError::into_inner).clone();
    current.unwrap_or_else(Data::base)
}

/// Where the packs are.
pub fn location() -> Location {
    LOCATION
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .clone()
        .unwrap_or_else(|| Location::find(&[]))
}

/// What a reload found.
#[derive(Debug, PartialEq, Eq)]
pub enum Reload {
    /// The packs' content is as it was (a file was touched, nothing changed).
    Same,
    /// New content, now [`data`] (its hash).
    Changed(u64),
    /// The packs are broken now: the old data stays (why).
    Broken(String),
}

/// Opens the packs again: a valid change replaces [`data`]; a broken one leaves it.
pub fn reload() -> Reload {
    let (reload, fresh) = reload_at(&location(), &data());
    if let Some(fresh) = fresh {
        *DATA.write().unwrap_or_else(PoisonError::into_inner) = Some(Arc::new(fresh));
    }
    reload
}

/// The packs at `location` compared with `current`: what changed, and the new data if it
/// is valid and different.
fn reload_at(location: &Location, current: &Data) -> (Reload, Option<Data>) {
    let fresh = location.open().map_err(|e| e.to_string()).and_then(|d| {
        let report = creeer_sim::content::validate(&d);
        if report.ok() {
            Ok(d)
        } else {
            Err(report.errors.join("\n"))
        }
    });
    match fresh {
        Err(e) => (Reload::Broken(e), None),
        Ok(fresh) if fresh.hash() == current.hash() => (Reload::Same, None),
        Ok(fresh) => (Reload::Changed(fresh.hash()), Some(fresh)),
    }
}

/// Watches the packs for changes.
pub fn watcher() -> Watcher {
    Watcher::new(location().roots())
}

/// The packs' engineering materials (the compiled-in ones if the packs' fail to build:
/// [`reload`] lets no such packs in).
pub fn materials() -> creeer_materials::MaterialTable {
    creeer_materials::MaterialTable::from_data(&data())
        .unwrap_or_else(|_| creeer_materials::MaterialTable::reference())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn copy(from: &Path, to: &Path) {
        std::fs::create_dir_all(to).unwrap();
        for entry in std::fs::read_dir(from).unwrap().map(Result::unwrap) {
            let target = to.join(entry.file_name());
            if entry.path().is_dir() {
                copy(&entry.path(), &target);
            } else {
                std::fs::copy(entry.path(), target).unwrap();
            }
        }
    }

    /// A copy of the base pack edited on disk: a valid edit is taken (and seen by a watcher),
    /// a broken one refused with its reason, and the data before it kept.
    #[test]
    fn a_changed_pack_reloads_and_a_broken_one_is_refused() {
        let root = std::env::temp_dir().join(format!("creeer_reload_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        copy(&creeer_data::locate::source_tree(), &root.join("data"));
        let location = Location {
            base: root.join("data"),
            mods: Vec::new(),
        };
        let first = location.open().unwrap();
        let mut watcher = Watcher::new(location.roots());
        assert_eq!(reload_at(&location, &first).0, Reload::Same);

        let file = root.join("data/materials/engineering.ron");
        let text = std::fs::read_to_string(&file).unwrap();
        std::fs::write(&file, text.replace("\"50 GPa\"", "\"55 GPa\"")).unwrap();
        assert!(watcher.check());
        let (reload, fresh) = reload_at(&location, &first);
        let fresh = fresh.expect("a valid change");
        assert_eq!(reload, Reload::Changed(fresh.hash()));
        let granite = creeer_materials::MaterialTable::from_data(&fresh).unwrap();
        let e = granite.get("granite").unwrap().youngs_modulus.at(20.0);
        assert!((e - 55e9).abs() < 1.0, "{e}");

        std::fs::write(&file, text.replace("\"50 GPa\"", "\"55 GPa/s\"")).unwrap();
        let (reload, none) = reload_at(&location, &fresh);
        assert!(none.is_none());
        let Reload::Broken(why) = reload else {
            panic!("{reload:?}")
        };
        assert!(why.contains("granite"), "{why}");
        let _ = std::fs::remove_dir_all(&root);
    }
}
