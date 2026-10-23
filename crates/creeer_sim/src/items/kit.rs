//! Items from the packs (G1.6a, ADR-0028): the pockets of what a character wears
//! (`wearables.ron`), the creative kit (`kit.ron`: what a creative server hands out to try
//! things with), and a material's density, from the engineering materials or the rocks.

use super::{Form, Item, Piece, Store};
use creeer_data::Data;
use creeer_data::merge::ron_list;
use creeer_materials::MaterialTable;
use creeer_math::glam::DVec3;
use creeer_terrain::geology::rocks::RockTable;
use serde::Deserialize;

/// Where the creative kit is kept in a pack.
pub const KIT: &str = "kit.ron";

#[derive(Clone, Copy, Debug, PartialEq, Deserialize)]
enum FormDef {
    Block { size: (f64, f64, f64) },
    Ball { radius: f64 },
    Rod { radius: f64, length: f64 },
}

impl From<FormDef> for Form {
    fn from(f: FormDef) -> Self {
        match f {
            FormDef::Block { size: (x, y, z) } => Self::Block {
                size: DVec3::new(x, y, z),
            },
            FormDef::Ball { radius } => Self::Ball { radius },
            FormDef::Rod { radius, length } => Self::Rod { radius, length },
        }
    }
}

/// One of the creative kit's items, as the pack has it.
#[derive(Clone, Debug, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct KitItem {
    pub name: String,
    pub material: String,
    form: FormDef,
    /// Its wall (m) if it is a container.
    #[serde(default)]
    pub wall: f64,
}

#[derive(Deserialize)]
struct PocketDef {
    name: String,
    size: (f64, f64, f64),
}

#[derive(Deserialize)]
struct WearableDef {
    name: String,
    #[serde(default)]
    pockets: Vec<PocketDef>,
}

/// Material densities: the packs' engineering materials, then their rocks.
#[derive(Clone, Debug, Default)]
pub struct Densities {
    materials: MaterialTable,
    rocks: RockTable,
    palette: creeer_art::palette::Palette,
}

impl Densities {
    /// From the packs `data`.
    ///
    /// # Errors
    /// If the materials or the rocks do not load.
    pub fn from_data(data: &Data) -> Result<Self, String> {
        Ok(Self {
            materials: MaterialTable::from_data(data)?,
            rocks: RockTable::from_data(data).map_err(|e| e.to_string())?,
            palette: creeer_art::palette::Palette::from_data(data)?,
        })
    }

    /// The density of `material` (kg/m³): an engineering material's id, or a rock's name.
    pub fn of(&self, material: &str) -> Option<f64> {
        (self.materials.get(material).map(|m| m.density)).or_else(|| {
            let rock = self.rocks.rock(material)?;
            Some(self.rocks.rocks[usize::from(rock)].density)
        })
    }

    /// How a rock breaks, if `material` is one that does.
    pub fn breaks(&self, material: &str) -> Option<creeer_terrain::geology::rocks::Breaks> {
        self.rocks.rocks[usize::from(self.rocks.rock(material)?)].breaks
    }

    /// The tensile strength of engineering material `material` at 20 °C (Pa).
    pub fn strength(&self, material: &str) -> Option<f64> {
        Some(self.materials.get(material)?.ultimate_strength.at(20.0))
    }

    /// The colour of `material` (sRGB), as it is drawn: a rock's own, or the palette's token
    /// for an engineering material's kind (`art-style.md` §3).
    pub fn colour(&self, material: &str) -> Option<[u8; 3]> {
        use creeer_materials::engineering::Kind;
        if let Some(rock) = self.rocks.rock(material) {
            return Some(self.rocks.rocks[usize::from(rock)].color);
        }
        let token = match self.materials.get(material)?.kind {
            Kind::Metal => "steel",
            Kind::Wood => "timber",
            Kind::Concrete | Kind::Rock => "concrete",
            Kind::Glass => "glass",
            Kind::Ice => "ice",
        };
        Some(self.palette.token(token)?.shade.fixed()?.0)
    }

    /// `kit` made, as a new empty piece.
    ///
    /// # Errors
    /// If its material is unknown or its shape is not a shape.
    pub fn make(&self, kit: &KitItem) -> Result<Item, String> {
        let density = (self.of(&kit.material))
            .ok_or_else(|| format!("{}: no material {}", kit.name, kit.material))?;
        let form = Form::from(kit.form);
        let valid = form.valid() && kit.wall.is_finite() && kit.wall >= 0.0;
        if !valid {
            return Err(format!("{}: a size that is not positive", kit.name));
        }
        let mut piece = Piece::solid(&kit.material, density, form);
        piece.wall = kit.wall;
        if kit.wall > 0.0 && piece.inside().is_none() {
            return Err(format!(
                "{}: a wall only a block thick enough has",
                kit.name
            ));
        }
        Ok(Item::Piece(piece, Vec::new()))
    }
}

/// The creative kit, as the packs merge it by name.
///
/// # Errors
/// If a file does not parse, or a name repeats within a pack.
pub fn kit(data: &Data) -> Result<Vec<KitItem>, String> {
    (data.table("kit", KIT))
        .merge(ron_list, |k: &KitItem| &k.name)
        .map_err(|e| e.to_string())
}

/// The pockets of `outfit` (wearables by name), empty, in the outfit's order.
///
/// # Errors
/// If the wearables do not parse, or a pocket's size is not positive.
pub fn pockets(data: &Data, outfit: &[String]) -> Result<Vec<Store>, String> {
    let all: Vec<WearableDef> = (data.table("wearables", creeer_art::wearable::WEARABLES))
        .merge(ron_list, |w: &WearableDef| &w.name)
        .map_err(|e| e.to_string())?;
    let mut out = Vec::new();
    for name in outfit {
        for w in all.iter().filter(|w| &w.name == name) {
            for p in &w.pockets {
                let (x, y, z) = p.size;
                if ![x, y, z].iter().all(|s| s.is_finite() && *s > 0.0) {
                    return Err(format!(
                        "{}.{}: a size that is not positive",
                        w.name, p.name
                    ));
                }
                out.push(Store::new(&format!("{}.{}", w.name, p.name), [x, y, z]));
            }
        }
    }
    Ok(out)
}

/// What a player sees of `item`: what it is made of, its size, and its mass. A found stone and
/// a bulk show their size, mass and grains, not what they are (ADR-0028: telling the rocks
/// apart is the player's to learn, by their colour, weight and how they break).
pub fn describe(item: &Item) -> String {
    match item {
        Item::Piece(p, inside) => {
            let size = match p.form {
                Form::Block { size } => format!(
                    "{:.0} × {:.0} × {:.0} cm",
                    size.x * 100.0,
                    size.y * 100.0,
                    size.z * 100.0
                ),
                Form::Ball { radius } => format!("{:.0} cm across", radius * 200.0),
                Form::Lump { size } => format!(
                    "a stone of {:.0} × {:.0} × {:.0} cm",
                    size.x * 100.0,
                    size.y * 100.0,
                    size.z * 100.0
                ),
                Form::Rod { radius, length } => {
                    format!(
                        "{:.0} cm long, {:.0} mm thick",
                        length * 100.0,
                        radius * 2000.0
                    )
                }
            };
            let holds = if p.inside().is_some() {
                format!(", holding {}", inside.len())
            } else {
                String::new()
            };
            let edge = p
                .edge
                .map_or_else(String::new, |e| format!("; {}", edge(e)));
            if matches!(p.form, Form::Lump { .. }) {
                return format!("{size}, {:.2} kg{edge}", item.mass());
            }
            format!("{}, {size}, {:.2} kg{holds}{edge}", p.material, item.mass())
        }
        Item::Made(a) => {
            let parts: Vec<String> = (a.parts.iter())
                .map(|p| describe(&Item::Piece(p.piece.clone(), Vec::new())))
                .collect();
            format!("made: {}; {:.2} kg", parts.join(" + "), a.mass())
        }
        Item::Bulk(b) => format!(
            "loose, {:.2} kg, {:.2} l, grains {:.1} mm",
            b.mass,
            b.volume() * 1000.0,
            b.grain * 1000.0
        ),
    }
}

/// An edge as a person sees and feels it: its length, its angle and how keen it is (G1.6).
fn edge(e: crate::items::Edge) -> String {
    let keen = match e.radius {
        r if r < 1e-6 => "razor sharp",
        r if r < 2e-5 => "sharp",
        r if r < 2e-4 => "fairly sharp",
        _ => "blunt",
    };
    format!(
        "an edge {:.0} cm long, {:.0}°, {keen}",
        e.length * 100.0,
        e.angle.to_degrees()
    )
}

/// A word for an item, by its shape (never its material: principle 9).
pub fn word(item: &Item) -> &'static str {
    match item {
        Item::Piece(p, _) => match p.form {
            Form::Lump { .. } | Form::Ball { .. } => match p.edge {
                // A flake's margin is keen (20–45°); a core's or a worked stone's steep.
                Some(e) if e.angle < 45f64.to_radians() => "flake",
                Some(_) => "worked stone",
                None => "stone",
            },
            Form::Rod { .. } => "stick",
            Form::Block { size } if size.min_element() < 0.004 => "strip",
            Form::Block { .. } => "block",
        },
        Item::Made(_) => "tool",
        Item::Bulk(_) => "grit",
    }
}
