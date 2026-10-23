//! Items (G1.6a, ADR-0028): what players carry, put away and drop, as one generic kind of
//! thing. A **piece** is a material and a shape (its mass, volume and size computed from them);
//! a **bulk** is loose matter: a mass, a composition and a temperature. Any piece with a wall
//! is a container and holds what fits inside it. There is no list of item types: a stone, a
//! stick, a box and (from G1.6d) a tool are all pieces, told apart by what they are made of
//! and their shape.
//!
//! Where items are kept is a [`Store`]: a pocket of the clothes, or a container's inside.
//! What fits is decided by size and volume, as clothes' pockets hold things (DayZ's rule: no
//! wider than the pocket in any direction, ADR-0028); bulk takes the pocket's shape.

use creeer_math::glam::DVec3;
use std::f64::consts::PI;

/// A piece's shape, about its centre (m).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Form {
    /// A block of these edge lengths.
    Block { size: DVec3 },
    /// A ball.
    Ball { radius: f64 },
    /// A rod (a cylinder) along its length.
    Rod { radius: f64, length: f64 },
    /// A lump (an ellipsoid) of these axes' lengths: a stone's shape (G1.6c).
    Lump { size: DVec3 },
}

impl Form {
    /// Its volume (m³).
    pub fn volume(&self) -> f64 {
        match *self {
            Self::Block { size } => size.x * size.y * size.z,
            Self::Ball { radius } => 4.0 / 3.0 * PI * radius * radius * radius,
            Self::Rod { radius, length } => PI * radius * radius * length,
            Self::Lump { size } => PI / 6.0 * size.x * size.y * size.z,
        }
    }

    /// The edges of the box it fits in, longest first (m).
    pub fn extents(&self) -> [f64; 3] {
        let mut e = match *self {
            Self::Block { size } | Self::Lump { size } => [size.x, size.y, size.z],
            Self::Ball { radius } => [2.0 * radius; 3],
            Self::Rod { radius, length } => [length, 2.0 * radius, 2.0 * radius],
        };
        e.sort_by(|a, b| b.total_cmp(a));
        e
    }

    /// Whether every size is positive and finite.
    pub fn valid(&self) -> bool {
        self.extents().iter().all(|e| e.is_finite() && *e > 0.0)
    }
}

/// A piece: a material and a shape, hollow if it has a wall (a container).
#[derive(Clone, Debug, PartialEq)]
pub struct Piece {
    /// Its material: an engineering material's id or a rock's name in the packs.
    pub material: String,
    /// The material's density (kg/m³), as it was found when the piece was made.
    pub density: f64,
    pub form: Form,
    /// Its wall's thickness if it is hollow (m; 0: solid). Only blocks are hollow.
    pub wall: f64,
    /// Its wear from use (0 new, 1 worn out).
    pub wear: f64,
    /// Its cutting edge, if it has one (G1.6d: knapped, later ground or forged).
    pub edge: Option<Edge>,
}

/// A cutting edge: its included angle (rad), its length (m) and its tip's radius (m: how sharp,
/// never sharper than half the material's grain).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Edge {
    pub angle: f64,
    pub length: f64,
    pub radius: f64,
}

impl Piece {
    /// A solid piece of `material` (of `density`).
    pub fn solid(material: &str, density: f64, form: Form) -> Self {
        Self {
            material: material.to_owned(),
            density,
            form,
            wall: 0.0,
            wear: 0.0,
            edge: None,
        }
    }

    /// Its inside's edges, longest first (none: it is solid).
    pub fn inside(&self) -> Option<[f64; 3]> {
        let Form::Block { size } = self.form else {
            return None;
        };
        let inner = size - DVec3::splat(2.0 * self.wall);
        (self.wall > 0.0 && inner.min_element() > 0.0)
            .then(|| Form::Block { size: inner }.extents())
    }

    /// A strip (a block at most 3 mm thin, a tenth as thick as wide) rolled up: its width and
    /// the coil's diameter both ways, longest first (bark and cloth roll; boards do not).
    pub fn coiled(&self) -> Option<[f64; 3]> {
        let Form::Block { .. } = self.form else {
            return None;
        };
        let [long, wide, thin] = self.form.extents();
        if thin > 0.003 || thin * 10.0 > wide {
            return None;
        }
        let coil = (4.0 * long * thin / PI).sqrt().max(thin);
        let mut e = [wide, coil, coil];
        e.sort_by(|a, b| b.total_cmp(a));
        Some(e)
    }

    /// Its own mass, without anything in it (kg).
    pub fn mass(&self) -> f64 {
        let hollow = self.inside().map_or(0.0, |e| e[0] * e[1] * e[2]);
        self.density * (self.form.volume() - hollow)
    }
}

/// Loose matter: crushed rock, soil, sand, ore.
#[derive(Clone, Debug, PartialEq)]
pub struct Bulk {
    /// Its mass (kg).
    pub mass: f64,
    /// What it is made of: species or minerals by name, mass fractions summing to 1.
    pub composition: Vec<(String, f64)>,
    /// Its temperature (K) and specific heat (J/(kg·K)).
    pub temperature: f64,
    pub specific_heat: f64,
    /// Its grains' mean size (m).
    pub grain: f64,
    /// Its bulk density, the voids between grains included (kg/m³).
    pub bulk_density: f64,
}

impl Bulk {
    /// Its volume, the voids included (m³).
    pub fn volume(&self) -> f64 {
        self.mass / self.bulk_density
    }

    /// `mass` (kg, at most all of it) taken out as a bulk of its own, of the same matter: what is
    /// left keeps the rest exactly.
    pub fn split(&mut self, mass: f64) -> Self {
        let taken = mass.clamp(0.0, self.mass);
        self.mass -= taken;
        Self {
            mass: taken,
            ..self.clone()
        }
    }

    /// `other` mixed in: mass and each species' mass conserved exactly, and the heat (the
    /// temperature from the heat capacities); the volumes add, so the bulk density follows.
    pub fn merge(&mut self, other: &Self) {
        let mass = self.mass + other.mass;
        if mass <= 0.0 {
            return;
        }
        let (a, b) = (self.mass, other.mass);
        let mut composition: Vec<(String, f64)> = Vec::new();
        for (part, m) in [(&self.composition, a), (&other.composition, b)] {
            for (name, f) in part {
                match composition.iter_mut().find(|(n, _)| n == name) {
                    Some((_, x)) => *x += f * m,
                    None => composition.push((name.clone(), f * m)),
                }
            }
        }
        for (_, x) in &mut composition {
            *x /= mass;
        }
        let (ca, cb) = (a * self.specific_heat, b * other.specific_heat);
        self.temperature = (ca * self.temperature + cb * other.temperature) / (ca + cb);
        self.specific_heat = (ca + cb) / mass;
        self.grain = (a * self.grain + b * other.grain) / mass;
        self.bulk_density = mass / (self.volume() + other.volume());
        self.mass = mass;
        self.composition = composition;
    }
}

/// An item: a piece (and what it holds, if hollow), a bulk, or something made of pieces (G1.6d).
#[derive(Clone, Debug, PartialEq)]
pub enum Item {
    Piece(Piece, Vec<Item>),
    Bulk(Bulk),
    Made(Box<Assembly>),
}

impl Item {
    /// Its mass, with what it holds (kg).
    pub fn mass(&self) -> f64 {
        match self {
            Self::Piece(p, inside) => p.mass() + inside.iter().map(Self::mass).sum::<f64>(),
            Self::Bulk(b) => b.mass,
            Self::Made(a) => a.mass(),
        }
    }

    /// The room it takes in a store (m³): a piece its box's, a bulk its own.
    pub fn room(&self) -> f64 {
        match self {
            Self::Piece(p, _) => (p.coiled().unwrap_or_else(|| p.form.extents()))
                .iter()
                .product(),
            Self::Bulk(b) => b.volume(),
            Self::Made(a) => a.extents().iter().product(),
        }
    }

    /// Its edges, longest first (a bulk takes the store's shape: none; a strip rolls up).
    pub fn extents(&self) -> Option<[f64; 3]> {
        match self {
            Self::Piece(p, _) => Some(p.coiled().unwrap_or_else(|| p.form.extents())),
            Self::Made(a) => Some(a.extents()),
            Self::Bulk(_) => None,
        }
    }

    /// What it holds, if it is a container.
    pub fn store(&self) -> Option<Store> {
        match self {
            Self::Piece(p, inside) => p.inside().map(|size| Store {
                name: String::new(),
                size,
                items: inside.clone(),
            }),
            Self::Bulk(_) | Self::Made(_) => None,
        }
    }

    /// Every species' mass in it and in what it holds, added into `out` (by name).
    pub fn species(&self, out: &mut Vec<(String, f64)>) {
        match self {
            Self::Piece(p, inside) => {
                add(out, &p.material, p.mass());
                inside.iter().for_each(|i| i.species(out));
            }
            Self::Bulk(b) => {
                for (name, f) in &b.composition {
                    add(out, name, f * b.mass);
                }
            }
            Self::Made(a) => {
                for p in a.parts.iter().map(|p| &p.piece).chain(&a.bindings) {
                    add(out, &p.material, p.mass());
                }
            }
        }
    }
}

fn add(out: &mut Vec<(String, f64)>, name: &str, mass: f64) {
    match out.iter_mut().find(|(n, _)| n == name) {
        Some((_, m)) => *m += mass,
        None => out.push((name.to_owned(), mass)),
    }
}

/// Where items are kept: a pocket, or a container's inside. Its bulk is one mix.
#[derive(Clone, Debug, PartialEq)]
pub struct Store {
    /// Its name (a pocket's: `"jacket.chest.L"`; a container's inside: empty).
    pub name: String,
    /// Its inside's edges, longest first (m).
    pub size: [f64; 3],
    pub items: Vec<Item>,
}

impl Store {
    /// An empty store of `size` (any order).
    pub fn new(name: &str, size: [f64; 3]) -> Self {
        let mut size = size;
        size.sort_by(|a, b| b.total_cmp(a));
        Self {
            name: name.to_owned(),
            size,
            items: Vec::new(),
        }
    }

    /// Its volume (m³).
    pub fn volume(&self) -> f64 {
        self.size.iter().product()
    }

    /// The room taken (m³).
    pub fn used(&self) -> f64 {
        self.items.iter().map(Item::room).sum()
    }

    /// What it holds weighs (kg).
    pub fn mass(&self) -> f64 {
        self.items.iter().map(Item::mass).sum()
    }

    /// Why `item` does not fit (none: it fits): wider than the store in some direction, or
    /// more than the room left.
    pub fn refuses(&self, item: &Item) -> Option<String> {
        if let Some(e) = item.extents()
            && e.iter().zip(&self.size).any(|(a, b)| a > b)
        {
            return Some(format!(
                "too large for it ({:.0} × {:.0} × {:.0} cm inside)",
                self.size[0] * 100.0,
                self.size[1] * 100.0,
                self.size[2] * 100.0
            ));
        }
        let left = self.volume() - self.used();
        (item.room() > left + 1e-12).then(|| format!("no room ({:.2} l left)", left * 1000.0))
    }

    /// Puts `item` in, if it fits (a bulk mixed into the store's bulk); gives it back if not.
    ///
    /// # Errors
    /// The item, and why it does not fit.
    pub fn put(&mut self, item: Item) -> Result<(), Box<(Item, String)>> {
        if let Some(why) = self.refuses(&item) {
            return Err(Box::new((item, why)));
        }
        if let Item::Bulk(b) = &item
            && let Some(Item::Bulk(mine)) =
                self.items.iter_mut().find(|i| matches!(i, Item::Bulk(_)))
        {
            mine.merge(b);
            return Ok(());
        }
        self.items.push(item);
        Ok(())
    }

    /// Takes item `index` out.
    pub fn take(&mut self, index: usize) -> Option<Item> {
        (index < self.items.len()).then(|| self.items.remove(index))
    }
}

mod assembly;
mod codec;
pub use assembly::{Assembly, Part, Swinging};
pub mod kit;
#[cfg(test)]
mod tests;
