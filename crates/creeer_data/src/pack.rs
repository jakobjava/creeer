//! One pack: its manifest and its files, read once when it is opened.

use crate::DataError;
use crate::manifest::Manifest;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// The extension of the files a pack's content is made of.
pub const EXTENSION: &str = "ron";

/// A pack: a directory with a `pack.ron`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Pack {
    pub manifest: Manifest,
    /// Its directory.
    pub root: PathBuf,
    /// Its content files by path relative to `root` (`/`-separated), with their text (line
    /// ends as `\n`, so a pack written on any OS reads and hashes the same).
    pub files: BTreeMap<String, String>,
}

impl Pack {
    /// Reads the pack at `root`.
    ///
    /// # Errors
    /// If it has no valid manifest or a file cannot be read.
    pub fn open(root: &Path) -> Result<Self, DataError> {
        let origin = root.display().to_string();
        let manifest_path = root.join("pack.ron");
        let text = read(&manifest_path, &origin)?;
        let manifest = Manifest::parse(&text, &origin)?;
        let mut files = BTreeMap::new();
        walk(root, "", &mut files, &manifest.name)?;
        files.remove("pack.ron");
        Ok(Self {
            manifest,
            root: root.to_path_buf(),
            files,
        })
    }

    /// Its name.
    pub fn name(&self) -> &str {
        &self.manifest.name
    }
}

/// Whether `dir` is a pack (has a manifest).
pub fn is_pack(dir: &Path) -> bool {
    dir.join("pack.ron").is_file()
}

fn read(path: &Path, origin: &str) -> Result<String, DataError> {
    let text = std::fs::read_to_string(path)
        .map_err(|e| DataError::new(origin, format!("{}: {e}", path.display())))?;
    Ok(if text.contains('\r') {
        text.replace("\r\n", "\n")
    } else {
        text
    })
}

/// Every content file under `dir` (`prefix` its path in the pack), hidden ones skipped,
/// in name order.
fn walk(
    dir: &Path,
    prefix: &str,
    files: &mut BTreeMap<String, String>,
    pack: &str,
) -> Result<(), DataError> {
    let entries = std::fs::read_dir(dir)
        .map_err(|e| DataError::new(pack, format!("{}: {e}", dir.display())))?;
    let mut names: Vec<(String, PathBuf)> = entries
        .filter_map(Result::ok)
        .filter_map(|e| Some((e.file_name().into_string().ok()?, e.path())))
        .filter(|(name, _)| !name.starts_with('.'))
        .collect();
    names.sort();
    for (name, path) in names {
        let relative = format!("{prefix}{name}");
        if path.is_dir() {
            walk(&path, &format!("{relative}/"), files, pack)?;
        } else if path.extension().is_some_and(|e| e == EXTENSION) {
            files.insert(relative, read(&path, pack)?);
        }
    }
    Ok(())
}
