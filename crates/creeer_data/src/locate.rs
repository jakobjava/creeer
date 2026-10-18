//! Where the data is at run time.
//!
//! The base pack: `--data <dir>`, else `CREEER_DATA`, else a `data/` beside the executable,
//! else the source tree's (development builds). Mods: every `--mods <dir>` and the
//! directories in `CREEER_MODS` (separated as `PATH` is), else a `mods/` beside the base
//! pack's directory if there is one.

use crate::{Data, DataError};
use std::path::{Path, PathBuf};

/// The directories the packs come from.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Location {
    pub base: PathBuf,
    pub mods: Vec<PathBuf>,
}

impl Location {
    /// From command-line `args` (`--data <dir>`, `--mods <dir>`, others ignored) and the
    /// environment.
    pub fn find(args: &[String]) -> Self {
        let value = |flag: &str| -> Vec<PathBuf> {
            args.windows(2)
                .filter(|w| w[0] == flag)
                .map(|w| PathBuf::from(&w[1]))
                .collect()
        };
        let base = value("--data")
            .pop()
            .or_else(|| std::env::var_os("CREEER_DATA").map(PathBuf::from))
            .or_else(|| {
                let exe = std::env::current_exe().ok()?;
                let beside = exe.parent()?.join("data");
                crate::pack::is_pack(&beside).then_some(beside)
            })
            .unwrap_or_else(source_tree);
        let mut mods = value("--mods");
        if let Some(list) = std::env::var_os("CREEER_MODS") {
            mods.extend(std::env::split_paths(&list));
        }
        if mods.is_empty()
            && let Some(beside) = base.parent().map(|p| p.join("mods"))
            && beside.is_dir()
        {
            mods.push(beside);
        }
        Self { base, mods }
    }

    /// The packs at this location.
    ///
    /// # Errors
    /// As [`Data::open`].
    pub fn open(&self) -> Result<Data, DataError> {
        Data::open(&self.base, &self.mods)
    }

    /// Every directory a change in which changes the data.
    pub fn roots(&self) -> Vec<PathBuf> {
        let mut roots = vec![self.base.clone()];
        roots.extend(self.mods.iter().cloned());
        roots
    }
}

/// The source tree's `data/` (where development builds find it).
pub fn source_tree() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../data")
}
