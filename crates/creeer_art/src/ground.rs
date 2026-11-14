//! The ground's look (G2.11, ADR-0037): what covers the bare rock where the ground is drawn.
//!
//! - **Layers** (`ground/layers.ron`): grass, moss, fallen litter, laid in patches over the
//!   rock's own colour (`geology/rocks.ron`) where their ranges allow: the ground's rock, its
//!   slope, its height, the latitude, and whether a forest stands there.
//! - **Decoratives** (`ground/decoratives.ron`): small things drawn on the ground near the
//!   camera, many of a kind: grass tufts, grit.
//!
//! Both are drawn only: nothing here touches the physics, and the ground they lie on is the
//! simulated ground. They stay small and low (`art-style.md` §8: "grass tufts below knee height,
//! pebbles in the ground's own rock colour"), so what the player needs to find on the ground is
//! never hidden. The checks here hold every entry to that.

use crate::palette::Palette;
use creeer_data::Data;
use creeer_data::merge::ron_list;
use serde::Deserialize;

/// Where the layers live in a pack.
pub const LAYERS: &str = "ground/layers.ron";
/// Where the decoratives live in a pack.
pub const DECORATIVES: &str = "ground/decoratives.ron";
/// Layers the renderer draws at once (`terrain.wgsl`: one byte each in a vertex).
pub const MOST_LAYERS: usize = 4;
/// The tallest a decorative may be (m): below the knee, and below what lies on the ground.
pub const TALLEST: f64 = 0.35;
/// The largest a decorative stone may be (m): grit, never a stone to pick up (a loose stone,
/// which the player can take, is a thing of its own).
pub const LARGEST_GRIT: f64 = 0.03;

/// Whether a layer grows where a forest stands.
#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq)]
pub enum Forest {
    /// Anywhere its other ranges allow.
    Any,
    /// Only under trees (a forest floor's moss and fallen needles).
    Only,
    /// Only in the open (grass, which shade thins out).
    Not,
}

/// One layer over the rock.
#[derive(Clone, Debug, Deserialize, PartialEq)]
pub struct Layer {
    pub name: String,
    /// The palette token it is drawn in.
    pub token: String,
    /// The grounds it covers (`geology/rocks.ron` names).
    pub on: Vec<String>,
    /// |Latitude| (°), least and most.
    pub latitude: (f64, f64),
    /// The ground's height above the datum (m), least and most.
    pub elevation: (f64, f64),
    /// The steepest slope it stays on (°).
    pub slope: f64,
    /// Under trees, in the open, or both.
    pub forest: Forest,
    /// The share of the ground it covers where it grows (0–1).
    pub cover: f64,
    /// The size of its patches (m).
    pub patch: f64,
    /// The size of its fine grain (m): a blade's width, a moss cushion's, a needle's length.
    pub grain: f64,
    /// How thickly it covers within its patches (0–1; 1 hides the ground beneath).
    pub density: f64,
}

/// The shape a decorative is drawn as.
#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq)]
pub enum Shape {
    /// Blades from one root, leaning out (grass).
    Tuft,
    /// A rounded stone half in the ground (grit).
    Pebble,
}

/// One kind of decorative.
#[derive(Clone, Debug, Deserialize, PartialEq)]
pub struct Decorative {
    pub name: String,
    pub shape: Shape,
    /// The palette token it is drawn in; none: the ground's own rock colour.
    pub token: Option<String>,
    /// The layer it grows in (it stands where that layer covers the ground); none: anywhere.
    pub layer: Option<String>,
    /// How many stand on a square metre where it grows.
    pub per_m2: f64,
    /// Its height (m), least and most.
    pub height: (f64, f64),
    /// Its width (m), least and most.
    pub width: (f64, f64),
}

/// The ground's look: its layers (in the order they are laid, lowest first) and decoratives.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Ground {
    pub layers: Vec<Layer>,
    pub decoratives: Vec<Decorative>,
}

impl Ground {
    /// The ground's look as the packs merge it by name.
    ///
    /// # Errors
    /// If a file does not parse, or a name repeats within a pack.
    pub fn from_data(data: &Data) -> Result<Self, String> {
        let layers = (data.table("ground layers", LAYERS))
            .merge(ron_list, |l: &Layer| &l.name)
            .map_err(|e| e.to_string())?;
        let decoratives = (data.table("decoratives", DECORATIVES))
            .merge(ron_list, |d: &Decorative| &d.name)
            .map_err(|e| e.to_string())?;
        Ok(Self {
            layers,
            decoratives,
        })
    }

    /// What is wrong with the entries, against the palette and the rocks' names.
    pub fn problems(&self, palette: &Palette, rocks: &[&str]) -> Vec<String> {
        let mut out = Vec::new();
        if self.layers.len() > MOST_LAYERS {
            out.push(format!(
                "{LAYERS}: {} layers, the renderer draws at most {MOST_LAYERS}",
                self.layers.len()
            ));
        }
        let colour = |token: &str| palette.token(token).and_then(|t| t.shade.fixed());
        let range = |(a, b): (f64, f64)| a.is_finite() && b.is_finite() && a <= b;
        for l in &self.layers {
            let mut bad = |what: String| out.push(format!("{LAYERS}: {}: {what}", l.name));
            if colour(&l.token).is_none() {
                bad(format!("no fixed colour {}", l.token));
            }
            for g in l.on.iter().filter(|g| !rocks.contains(&g.as_str())) {
                bad(format!("no ground {g}"));
            }
            if !(range(l.latitude) && range(l.elevation)) {
                bad("a range runs backwards".into());
            }
            let shares = (0.0..=1.0).contains(&l.cover) && (0.0..=1.0).contains(&l.density);
            if !(shares && l.patch > 0.0 && l.grain > 0.0) {
                bad("cover and density must be 0–1, patch and grain above 0".into());
            }
        }
        for d in &self.decoratives {
            let mut bad = |what: String| out.push(format!("{DECORATIVES}: {}: {what}", d.name));
            if let Some(token) = &d.token
                && colour(token).is_none()
            {
                bad(format!("no fixed colour {token}"));
            }
            if let Some(layer) = &d.layer
                && !self.layers.iter().any(|l| &l.name == layer)
            {
                bad(format!("no layer {layer}"));
            }
            if !(range(d.height) && range(d.width) && d.height.0 > 0.0 && d.width.0 > 0.0) {
                bad("sizes must be above 0, least first".into());
            }
            if d.height.1 > TALLEST {
                bad(format!(
                    "taller than {TALLEST} m: it would hide what lies on the ground"
                ));
            }
            if d.shape == Shape::Pebble && d.width.1.max(d.height.1) > LARGEST_GRIT {
                bad(format!(
                    "a decorative stone over {LARGEST_GRIT} m would be taken for one to pick up"
                ));
            }
            if !(d.per_m2.is_finite() && d.per_m2 >= 0.0) {
                bad("per_m2 must be 0 or more".into());
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn base() -> (Ground, Palette) {
        let data = Data::base();
        (
            Ground::from_data(&data).expect("the base ground"),
            Palette::from_data(&data).expect("the base palette"),
        )
    }

    #[test]
    fn the_base_ground_is_whole() {
        let (ground, palette) = base();
        assert!(!ground.layers.is_empty() && !ground.decoratives.is_empty());
        let problems = ground.problems(&palette, &["soil", "gravel", "granite", "sandstone"]);
        assert!(problems.is_empty(), "{problems:#?}");
    }

    #[test]
    fn what_could_hide_things_is_refused() {
        let (mut ground, palette) = base();
        let rocks = ["soil", "gravel", "granite", "sandstone"];
        let tuft = ground
            .decoratives
            .iter_mut()
            .find(|d| d.shape == Shape::Tuft);
        tuft.expect("a tuft").height.1 = 0.6;
        let pebble = ground
            .decoratives
            .iter_mut()
            .find(|d| d.shape == Shape::Pebble);
        pebble.expect("grit").width.1 = 0.08;
        let problems = ground.problems(&palette, &rocks).join("\n");
        assert!(problems.contains("taller than"), "{problems}");
        assert!(problems.contains("taken for one to pick up"), "{problems}");
    }

    #[test]
    fn unknown_names_are_refused() {
        let (mut ground, palette) = base();
        ground.layers[0].token = "nonsense".into();
        ground.layers[0].on.push("cheese".into());
        ground.decoratives[0].layer = Some("nowhere".into());
        let problems = ground.problems(&palette, &["soil"]).join("\n");
        for what in [
            "no fixed colour nonsense",
            "no ground cheese",
            "no layer nowhere",
        ] {
            assert!(problems.contains(what), "{what}: {problems}");
        }
    }
}
