//! The merged packs: the base, then the mods in dependency order.

use crate::DataError;
use crate::manifest::BASE;
use crate::merge::{Step, Table};
use crate::pack::{Pack, is_pack};
use std::path::{Path, PathBuf};
use std::sync::{Arc, OnceLock};

/// Every pack, read once and kept: a consistent snapshot (a hot-reload opens a new one).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Data {
    packs: Vec<Pack>,
    hash: u64,
}

impl Data {
    /// The base pack at `base` and every pack in each of `mods` (a directory of packs).
    ///
    /// # Errors
    /// If a pack cannot be read, two share a name, a requirement is missing or circular, or
    /// the base is not named `base`.
    pub fn open(base: &Path, mods: &[PathBuf]) -> Result<Self, DataError> {
        let base = Pack::open(base)?;
        if base.name() != BASE {
            return Err(DataError::new(
                base.name(),
                format!("the base pack must be named {BASE:?}"),
            ));
        }
        let mut found = Vec::new();
        for dir in mods {
            found.extend(packs_in(dir)?);
        }
        Self::from_packs(base, found)
    }

    /// The base pack and `mods`, ordered: each after the packs it requires, ties by name.
    ///
    /// # Errors
    /// As [`Data::open`].
    pub fn from_packs(base: Pack, mut mods: Vec<Pack>) -> Result<Self, DataError> {
        mods.sort_by(|a, b| a.name().cmp(b.name()));
        for pair in mods.windows(2) {
            if pair[0].name() == pair[1].name() {
                return Err(DataError::new(
                    pair[0].name(),
                    format!(
                        "two packs have this name ({} and {})",
                        pair[0].root.display(),
                        pair[1].root.display()
                    ),
                ));
            }
        }
        if let Some(clash) = mods.iter().find(|p| p.name() == BASE) {
            return Err(DataError::new(
                clash.name(),
                "only the base pack may be named base",
            ));
        }
        let mut packs = vec![base];
        while !mods.is_empty() {
            let ready = mods.iter().position(|p| {
                p.manifest
                    .requires
                    .iter()
                    .all(|r| packs.iter().any(|q| q.name() == r))
            });
            let Some(k) = ready else {
                let p = &mods[0];
                let missing: Vec<&String> = p
                    .manifest
                    .requires
                    .iter()
                    .filter(|r| !packs.iter().any(|q| q.name() == r.as_str()))
                    .collect();
                let known = |r: &String| mods.iter().any(|q| q.name() == r);
                let why = if missing.iter().all(|r| known(r)) {
                    "a circle of requirements"
                } else {
                    "a pack that is not there"
                };
                return Err(DataError::new(
                    p.name(),
                    format!("requires {missing:?}: {why}"),
                ));
            };
            packs.push(mods.remove(k));
        }
        let hash = content_hash(&packs);
        Ok(Self { packs, hash })
    }

    /// The repository's own `data/` alone (tools and tests).
    pub fn base() -> Arc<Self> {
        static BASE_DATA: OnceLock<Arc<Data>> = OnceLock::new();
        Arc::clone(BASE_DATA.get_or_init(|| {
            let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../data");
            Arc::new(Self::open(&root, &[]).expect("the repository's data/ opens"))
        }))
    }

    /// The packs in load order (the base first).
    pub fn packs(&self) -> &[Pack] {
        &self.packs
    }

    /// The 64-bit hash of every pack's name and files in load order: two installations with
    /// the same hash have the same content.
    pub fn hash(&self) -> u64 {
        self.hash
    }

    /// The text of file `path` from the last pack that has it.
    ///
    /// # Errors
    /// If no pack has it.
    pub fn file(&self, path: &str) -> Result<&str, DataError> {
        self.packs
            .iter()
            .rev()
            .find_map(|p| p.files.get(path))
            .map(String::as_str)
            .ok_or_else(|| DataError::new(path, "no pack has this file"))
    }

    /// The bytes of binary file `path` (a model) from the last pack that has it.
    ///
    /// # Errors
    /// If no pack has it.
    pub fn binary(&self, path: &str) -> Result<&[u8], DataError> {
        self.packs
            .iter()
            .rev()
            .find_map(|p| p.binaries.get(path))
            .map(Vec::as_slice)
            .ok_or_else(|| DataError::new(path, "no pack has this file"))
    }

    /// The pack the text or bytes of `path` come from.
    pub fn origin(&self, path: &str) -> Option<&str> {
        self.packs
            .iter()
            .rev()
            .find(|p| p.files.contains_key(path) || p.binaries.contains_key(path))
            .map(Pack::name)
    }

    /// The files whose path starts with `dir` (e.g. `"planets/"`), from every pack, sorted.
    pub fn list(&self, dir: &str) -> Vec<&str> {
        let mut all: Vec<&str> = self
            .packs
            .iter()
            .flat_map(|p| p.files.keys())
            .filter(|k| k.starts_with(dir))
            .map(String::as_str)
            .collect();
        all.sort_unstable();
        all.dedup();
        all
    }

    /// Table `name` made of the files `pattern` names in every pack: a path, or a directory
    /// and `*.ron` (`"materials/*.ron"`), with each pack's removals before its files.
    pub fn table(&self, name: &str, pattern: &str) -> Table<'_> {
        let matches = |path: &str| match pattern.strip_suffix("*.ron") {
            Some(dir) => path.starts_with(dir) && !path[dir.len()..].contains('/'),
            None => path == pattern,
        };
        let mut steps = Vec::new();
        for pack in &self.packs {
            if let Some(names) = pack.manifest.remove.get(name) {
                steps.push(Step::Remove {
                    pack: pack.name(),
                    names,
                });
            }
            for (path, text) in pack.files.iter().filter(|(p, _)| matches(p)) {
                steps.push(Step::File {
                    pack: pack.name(),
                    path,
                    text,
                });
            }
        }
        Table {
            name: name.to_owned(),
            steps,
        }
    }
}

/// The packs in `dir`: each subdirectory with a manifest (none if `dir` does not exist).
///
/// # Errors
/// If a pack cannot be read.
pub fn packs_in(dir: &Path) -> Result<Vec<Pack>, DataError> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Ok(Vec::new());
    };
    let mut dirs: Vec<PathBuf> = entries
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| is_pack(p))
        .collect();
    dirs.sort();
    dirs.iter().map(|d| Pack::open(d)).collect()
}

/// FNV-1a over the packs' names, file paths and texts, each followed by a 0 byte.
fn content_hash(packs: &[Pack]) -> u64 {
    let mut h = 0xcbf2_9ce4_8422_2325_u64;
    let mut eat = |bytes: &[u8]| {
        for &b in bytes.iter().chain(&[0]) {
            h = (h ^ u64::from(b)).wrapping_mul(0x0000_0100_0000_01b3);
        }
    };
    for pack in packs {
        eat(pack.name().as_bytes());
        for (table, names) in &pack.manifest.remove {
            eat(table.as_bytes());
            names.iter().for_each(|n| eat(n.as_bytes()));
        }
        for (path, text) in &pack.files {
            eat(path.as_bytes());
            eat(text.as_bytes());
        }
        for (path, bytes) in &pack.binaries {
            eat(path.as_bytes());
            eat(bytes);
        }
    }
    h
}
