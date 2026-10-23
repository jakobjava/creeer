//! The hand tools (G1.4c, ADR-0027 decision 3): catalogue tools' masses and sizes
//! (`data/tools.ron`), each drawn by its model (`data/models/models.ron`, class `Tool`) and
//! held in the right hand by its grip. Their strikes (G1.4e) take their energy from these.

use creeer_art::model::{Class, Entry};
use creeer_data::Data;
use creeer_data::merge::ron_list;
use serde::Deserialize;

/// Where the table is kept in a pack.
pub const TABLE: &str = "tools.ron";

/// What a tool is for.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
pub enum Kind {
    /// Breaking rock (G1.5).
    Pick,
    /// Moving loose ground (G1.5).
    Shovel,
    /// Felling and splitting wood (G1.7).
    Axe,
    /// Cutting wood through (G1.7).
    Saw,
}

/// A hand tool.
#[derive(Clone, Debug, PartialEq, Deserialize)]
pub struct Tool {
    pub name: String,
    /// Its model's name in the models' manifest.
    pub model: String,
    pub kind: Kind,
    /// Its mass and its head's (kg).
    pub mass: f64,
    pub head: f64,
    /// Its length (m).
    pub length: f64,
    /// Where it is gripped (m up from the handle's end), and the handle's radius there (m).
    pub grip: f64,
    pub radius: f64,
    /// Where it is carried (m up from the handle's end: at its balance), and the radius there.
    pub carry: f64,
    pub carry_radius: f64,
    /// Swung about the handle's end (G1.4e): its radius of gyration there (m), and how far out
    /// from there it strikes (m).
    pub gyration: f64,
    pub strike: f64,
    /// Its edge, if it has one (G1.7): its included angle (°) and its length (m).
    #[serde(default)]
    pub edge: Option<(f64, f64)>,
}

impl Tool {
    /// Every tool, as the packs merge them by name.
    ///
    /// # Errors
    /// If a table does not parse, or a name repeats within a pack.
    pub fn all(data: &Data) -> Result<Vec<Self>, String> {
        (data.table("tools", TABLE))
            .merge(ron_list, |t: &Self| &t.name)
            .map_err(|e| e.to_string())
    }
}

/// What is wrong with the tools against the models: each must have a tool's model, a head
/// lighter than the whole, a grip and a carry within its length, and handles a hand closes on
/// there (1–3 cm).
pub fn problems(tools: &[Tool], models: &[Entry]) -> Vec<String> {
    let mut out = Vec::new();
    for t in tools {
        let at = &t.name;
        match models.iter().find(|m| m.name == t.model) {
            None => out.push(format!("tool {at}: no model {}", t.model)),
            Some(m) if m.class != Class::Tool => {
                out.push(format!("tool {at}: its model {} is no tool's", t.model));
            }
            Some(_) => {}
        }
        if !(t.mass > 0.0 && t.head > 0.0 && t.head < t.mass) {
            out.push(format!("tool {at}: mass {} kg, head {} kg", t.mass, t.head));
        }
        if !(0.0..t.length).contains(&t.grip) {
            out.push(format!(
                "tool {at}: gripped {} m up its {} m",
                t.grip, t.length
            ));
        }
        if !(0.0..t.length).contains(&t.carry) {
            out.push(format!(
                "tool {at}: carried {} m up its {} m",
                t.carry, t.length
            ));
        }
        // (About the handle's end its inertia is at least its mass at its balance's.)
        if !(t.gyration > t.carry && t.strike > t.carry && t.strike < 1.2 * t.length) {
            out.push(format!(
                "tool {at}: gyration {} m and strike {} m, balanced {} m up its {} m",
                t.gyration, t.strike, t.carry, t.length
            ));
        }
        for r in [t.radius, t.carry_radius] {
            if !(0.01..=0.03).contains(&r) {
                out.push(format!("tool {at}: a handle {r} m round"));
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_four_tools_are_catalogue_tools_with_their_models() {
        let data = Data::base();
        let tools = Tool::all(&data).expect("tools");
        let models = Entry::all(&data).expect("models");
        assert!(
            problems(&tools, &models).is_empty(),
            "{:?}",
            problems(&tools, &models)
        );
        let kinds: Vec<Kind> = tools.iter().map(|t| t.kind).collect();
        assert_eq!(kinds, [Kind::Pick, Kind::Shovel, Kind::Axe, Kind::Saw]);
        // The 5 lb pick mattock's head.
        let pick = &tools[0];
        assert!((pick.head - 5.0 * 0.453_592_37).abs() < 0.01);
    }

    #[test]
    fn a_tool_without_its_model_is_a_problem() {
        let bad = Tool {
            name: "hammer".into(),
            model: "hammer".into(),
            kind: Kind::Pick,
            mass: 1.0,
            head: 2.0,
            length: 0.3,
            grip: 0.5,
            radius: 0.05,
            carry: 0.4,
            carry_radius: 0.005,
            gyration: 0.2,
            strike: 0.3,
            edge: None,
        };
        assert_eq!(problems(&[bad], &[]).len(), 7);
    }
}
