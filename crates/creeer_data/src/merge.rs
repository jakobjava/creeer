//! Tables of named entries across packs: each pack's entries replace those of the same name
//! from the packs before it, new names add, and a manifest may take names out.

use crate::DataError;
use std::collections::HashMap;

/// One step of building a table, in load order.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Step<'a> {
    /// Pack `pack` takes these names out (before its own files add theirs).
    Remove { pack: &'a str, names: &'a [String] },
    /// A file of pack `pack` at `path`.
    File {
        pack: &'a str,
        path: &'a str,
        text: &'a str,
    },
}

/// A table's steps, as [`crate::Data::table`] gives them.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Table<'a> {
    /// The table's name (as manifests name it in `remove`).
    pub name: String,
    pub steps: Vec<Step<'a>>,
}

impl Table<'_> {
    /// The merged entries: `parse` reads a file's entries, `key` names one. The base's
    /// order is kept; a replacing entry takes the place of the one it replaces; new ones
    /// follow in load order.
    ///
    /// # Errors
    /// If a file does not parse, a name repeats within one pack, or a pack removes a name
    /// no earlier pack defines.
    pub fn merge<T>(
        &self,
        parse: impl Fn(&str) -> Result<Vec<T>, String>,
        key: impl Fn(&T) -> &str,
    ) -> Result<Vec<T>, DataError> {
        let traced = self.merge_traced(parse, key)?;
        Ok(traced.into_iter().map(|(entry, _)| entry).collect())
    }

    /// As [`Table::merge`], each entry with where it came from (`"pack: path"`), for errors
    /// found when the entries are built.
    ///
    /// # Errors
    /// As [`Table::merge`].
    pub fn merge_traced<T>(
        &self,
        parse: impl Fn(&str) -> Result<Vec<T>, String>,
        key: impl Fn(&T) -> &str,
    ) -> Result<Vec<(T, String)>, DataError> {
        let mut entries: Vec<Option<(T, String)>> = Vec::new();
        let mut index: HashMap<String, usize> = HashMap::new();
        // Names defined by the pack whose files are being read (a repeat is an error).
        let mut current: (&str, Vec<String>) = ("", Vec::new());
        for step in &self.steps {
            match *step {
                Step::Remove { pack, names } => {
                    for name in names {
                        let Some(k) = index.remove(name) else {
                            return Err(DataError::new(
                                pack,
                                format!(
                                    "removes {:?} from {}, which no earlier pack has",
                                    name, self.name
                                ),
                            ));
                        };
                        entries[k] = None;
                    }
                }
                Step::File { pack, path, text } => {
                    if current.0 != pack {
                        current = (pack, Vec::new());
                    }
                    let origin = format!("{pack}: {path}");
                    let parsed = parse(text).map_err(|e| DataError::new(&origin, e))?;
                    for entry in parsed {
                        let name = key(&entry).to_owned();
                        if current.1.contains(&name) {
                            return Err(DataError::new(
                                &origin,
                                format!("{name:?} is defined twice in this pack"),
                            ));
                        }
                        current.1.push(name.clone());
                        match index.get(&name) {
                            Some(&k) => entries[k] = Some((entry, origin.clone())),
                            None => {
                                index.insert(name, entries.len());
                                entries.push(Some((entry, origin.clone())));
                            }
                        }
                    }
                }
            }
        }
        Ok(entries.into_iter().flatten().collect())
    }

    /// The packs that contribute to it, in order.
    pub fn packs(&self) -> Vec<&str> {
        let mut packs: Vec<&str> = Vec::new();
        for step in &self.steps {
            let (Step::Remove { pack, .. } | Step::File { pack, .. }) = *step;
            if packs.last() != Some(&pack) {
                packs.push(pack);
            }
        }
        packs
    }
}

/// Reads a RON list of entries (the usual `parse` for [`Table::merge`]).
///
/// # Errors
/// If it does not parse.
pub fn ron_list<T: serde::de::DeserializeOwned>(text: &str) -> Result<Vec<T>, String> {
    ron::from_str(text).map_err(|e| e.to_string())
}
