//! Wearables (G1.3f, ADR-0026 decision 7): each a piece of the engineer's model (its parts named
//! `<piece>:<region>@<level>`, [`crate::model_skin`]), and what it hides when worn: regions of
//! the body and of the wearables beneath, never drawn under it (`data/wearables.ron`). What a
//! worn outfit draws is held to the character's budget at each level (`art-style.md` §7).

use crate::model::{Class, Model};
use crate::model_skin::{LEVELS, REGIONS};
use creeer_data::Data;
use creeer_data::merge::ron_list;
use serde::Deserialize;
use std::collections::BTreeSet;

/// Where the wearables live in a pack.
pub const WEARABLES: &str = "wearables.ron";

/// One wearable.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
pub struct Wearable {
    /// Its piece of the model.
    pub name: String,
    /// What it hides: a piece (the body or a wearable) and its regions.
    pub hides: Vec<(String, Vec<String>)>,
}

impl Wearable {
    /// Every wearable, as the packs merge them by name.
    ///
    /// # Errors
    /// If a file does not parse, or a name repeats within a pack.
    pub fn all(data: &Data) -> Result<Vec<Self>, String> {
        (data.table("wearables", WEARABLES))
            .merge(ron_list, |w: &Self| &w.name)
            .map_err(|e| e.to_string())
    }
}

/// The triangles drawn at each level when the body wears `worn` (names among `wearables`):
/// every part of the body and of the worn pieces but the regions they hide.
pub fn drawn(model: &Model, wearables: &[Wearable], worn: &[&str]) -> [usize; LEVELS as usize] {
    let hidden: BTreeSet<(&str, &str)> = (wearables.iter())
        .filter(|w| worn.contains(&w.name.as_str()))
        .flat_map(|w| {
            (w.hides.iter()).flat_map(|(piece, regions)| {
                regions.iter().map(move |r| (piece.as_str(), r.as_str()))
            })
        })
        .collect();
    let mut out = [0; LEVELS as usize];
    for part in &model.parts {
        let shown = part.piece == "body" || worn.contains(&part.piece.as_str());
        if shown && !hidden.contains(&(part.piece.as_str(), part.region.as_str())) {
            out[usize::from(part.level)] += part.triangles();
        }
    }
    out
}

/// What is wrong with `wearables` as pieces of `model`: a name no piece of it has, a hidden
/// piece that is neither the body nor a wearable, a region not one of [`REGIONS`].
pub fn problems(model: &Model, wearables: &[Wearable]) -> Vec<String> {
    let pieces: BTreeSet<&str> = model.parts.iter().map(|p| p.piece.as_str()).collect();
    let mut out = Vec::new();
    for w in wearables {
        if !pieces.contains(w.name.as_str()) {
            out.push(format!("wearable {} is no piece of the model", w.name));
        }
        for (piece, regions) in &w.hides {
            if piece != "body" && !wearables.iter().any(|o| &o.name == piece) {
                out.push(format!(
                    "wearable {} hides {piece}, neither the body nor a wearable",
                    w.name
                ));
            }
            for region in regions {
                if !REGIONS.contains(&region.as_str()) {
                    out.push(format!(
                        "wearable {} hides {piece}'s {region}, no region",
                        w.name
                    ));
                }
            }
        }
    }
    out
}

/// The levels at which `wearables`, all worn at once, draw more than a character's budget.
pub fn over_budget(model: &Model, wearables: &[Wearable]) -> Vec<String> {
    let mut out = Vec::new();
    let all: Vec<&str> = wearables.iter().map(|w| w.name.as_str()).collect();
    for (level, triangles) in drawn(model, wearables, &all).into_iter().enumerate() {
        let level = u8::try_from(level).unwrap_or(u8::MAX);
        let budget = Class::Character.level_budget(level);
        if triangles > budget {
            out.push(format!(
                "all worn, level {level} draws {triangles} triangles, over a character's {budget}"
            ));
        }
    }
    out
}
