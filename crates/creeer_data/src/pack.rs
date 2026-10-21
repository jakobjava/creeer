//! One pack: its manifest and its files, read once when it is opened.

use crate::DataError;
use crate::manifest::Manifest;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// The extension of the text files a pack's content is made of.
pub const EXTENSION: &str = "ron";
/// The extensions of the binary files a pack may carry: models (binary glTF, G1.3b).
pub const BINARY_EXTENSIONS: [&str; 1] = ["glb"];

/// A pack: a directory with a `pack.ron`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Pack {
    pub manifest: Manifest,
    /// Its directory.
    pub root: PathBuf,
    /// Its content files by path relative to `root` (`/`-separated), with their text (line
    /// ends as `\n`, so a pack written on any OS reads and hashes the same).
    pub files: BTreeMap<String, String>,
    /// Its binary files (models) by path, with their bytes as they are.
    pub binaries: BTreeMap<String, Vec<u8>>,
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
        let mut found = Found::default();
        walk(root, "", &mut found, &manifest.name)?;
        found.files.remove("pack.ron");
        Ok(Self {
            manifest,
            root: root.to_path_buf(),
            files: found.files,
            binaries: found.binaries,
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

/// A pack's files as [`walk`] finds them.
#[derive(Default)]
struct Found {
    files: BTreeMap<String, String>,
    binaries: BTreeMap<String, Vec<u8>>,
}

/// Every content file under `dir` (`prefix` its path in the pack), hidden ones skipped,
/// in name order.
fn walk(dir: &Path, prefix: &str, found: &mut Found, pack: &str) -> Result<(), DataError> {
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
        let extension = path.extension().and_then(|e| e.to_str()).unwrap_or("");
        if path.is_dir() {
            walk(&path, &format!("{relative}/"), found, pack)?;
        } else if extension == EXTENSION {
            found.files.insert(relative, read(&path, pack)?);
        } else if BINARY_EXTENSIONS.contains(&extension) {
            let bytes = std::fs::read(&path)
                .map_err(|e| DataError::new(pack, format!("{}: {e}", path.display())))?;
            found.binaries.insert(relative, bytes);
        }
    }
    Ok(())
}
