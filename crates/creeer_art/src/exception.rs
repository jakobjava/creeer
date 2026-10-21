//! Exceptions to the style's limits (`docs/art-style.md` §4), each approved by the user and
//! dated (`data/art/exceptions.ron`). A check that an exception covers is not reported; an
//! exception that covers nothing is, so none outlives its reason.

use creeer_data::Data;
use creeer_data::merge::ron_list;
use serde::Deserialize;

/// A limit an exception lifts.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
pub enum Rule {
    /// The contrast against this ground: edge wear and occlusion carry it instead.
    Contrast(String),
    /// The chroma limit of the subject's group.
    Chroma,
}

/// One approved exception.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Exception {
    /// A token or a rock.
    pub subject: String,
    pub rules: Vec<Rule>,
    /// Why it is accepted.
    pub why: String,
    /// When the user approved it (`yyyy-mm-dd`).
    pub approved: String,
}

/// Where the exceptions live in a pack.
pub const FILE: &str = "art/exceptions.ron";

impl Exception {
    /// The exceptions as the packs of `data` merge them, by subject.
    ///
    /// # Errors
    /// If a file does not parse or a subject repeats within a pack.
    pub fn from_data(data: &Data) -> Result<Vec<Self>, String> {
        (data.table("exceptions", FILE))
            .merge(ron_list, |e: &Self| &e.subject)
            .map_err(|e| e.to_string())
    }

    /// The exceptions in `text` (tests, tools).
    ///
    /// # Errors
    /// If it does not parse.
    pub fn parse(text: &str) -> Result<Vec<Self>, String> {
        ron_list(text).map_err(|e| format!("{FILE}: {e}"))
    }
}
