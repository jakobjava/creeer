//! A planet's air and sea around the bodies of its world (F6.6): every body the air drags
//! (its shapes' drag: a sphere's by the drag curve, a box's or a hull's as a bluff body of
//! C_d 1 on its mean projected area, a quarter of its surface by Cauchy's formula), and the
//! sea buoys up and drags where it is under water. Kept in step with the world's bodies.

use crate::aero::{self, Airframe, Datum, Sky};
use crate::sea::{self, Floater, Level, Sea};
use creeer_fluids::aero::{Shape as AeroShape, Surface};
use creeer_fluids::atmosphere::Atmosphere;
use creeer_materials::species::SpeciesTable;
use creeer_math::glam::DVec3;
use creeer_physics::world::World;
use creeer_physics::world::shape::Shape;
use creeer_terrain::Planet;

/// A bluff body's drag coefficient (a cube's 1.05, a rock's about 1).
const BLUFF: f64 = 1.0;

/// The air and sea of a world, and what of each body they act on.
#[derive(Clone, Debug)]
pub struct Surroundings {
    pub sky: Option<Sky>,
    pub sea: Option<Sea>,
    /// Bodies they leave alone (avatars, whose players' machines predict them without).
    pub spared: Vec<bool>,
    frames: Vec<Airframe>,
    floaters: Vec<Floater>,
    /// How many of the world's bodies they have been given.
    known: usize,
    /// The water a save said moved with the floaters, for when they are met again.
    restored: Vec<f64>,
    /// The water bodies put back into the world had moving with them, by body index, for
    /// when they are met (parked bodies, `crate::lod`).
    expected: Vec<(usize, Vec<f64>)>,
}

impl Surroundings {
    /// The air and sea of `planet` (none where its data has none).
    ///
    /// # Errors
    /// If its atmosphere's gases are not in `table`.
    pub fn of(planet: &Planet, table: &SpeciesTable) -> Result<Self, String> {
        let sky = match &planet.atmosphere {
            Some(data) => Some(Sky {
                atmosphere: Atmosphere::new(data, planet.radius, planet.mu, table)?,
                datum: Datum::Sphere {
                    radius: planet.radius,
                },
                wind: DVec3::ZERO,
            }),
            None => None,
        };
        let surface_pressure = planet
            .atmosphere
            .as_ref()
            .map_or(0.0, |a| a.surface_pressure);
        let sea = planet.ocean.map(|water| Sea {
            level: Level::Planet(planet.datum),
            water,
            surface_pressure,
        });
        Ok(Self {
            sky,
            sea,
            spared: Vec::new(),
            frames: Vec::new(),
            floaters: Vec::new(),
            known: 0,
            restored: Vec::new(),
            expected: Vec::new(),
        })
    }

    /// Leaves body `index` alone (an avatar: before its first step).
    pub fn spare(&mut self, index: usize) {
        if self.spared.len() <= index {
            self.spared.resize(index + 1, false);
        }
        self.spared[index] = true;
    }

    /// Bodies added since the last step get their surfaces.
    fn meet(&mut self, world: &World) {
        for (index, body) in world.bodies.iter().enumerate().skip(self.known) {
            if self.spared.get(index).copied().unwrap_or(false) {
                continue;
            }
            for (node, shape) in &body.shapes {
                let air = match shape {
                    Shape::Sphere { radius } => AeroShape::Sphere {
                        diameter: 2.0 * radius,
                    },
                    Shape::Box { half } => {
                        let surface = 8.0 * (half.x * half.y + half.y * half.z + half.z * half.x);
                        AeroShape::Bluff {
                            area: surface / 4.0,
                            drag: BLUFF,
                        }
                    }
                    Shape::Hull(hull) => AeroShape::Bluff {
                        area: hull_surface(hull) / 4.0,
                        drag: BLUFF,
                    },
                };
                self.frames.push(Airframe::new(
                    index,
                    *node,
                    Surface {
                        shape: air,
                        offset: DVec3::ZERO,
                    },
                ));
                self.floaters
                    .push(Floater::new(index, *node, shape.clone(), BLUFF));
            }
            if let Some(k) = self.expected.iter().position(|(b, _)| *b == index) {
                let (_, water) = self.expected.swap_remove(k);
                let mine = self.floaters.iter_mut().filter(|f| f.body == index);
                for (f, m) in mine.zip(water) {
                    f.added = m;
                }
            }
        }
        if !self.restored.is_empty() {
            for (f, m) in self
                .floaters
                .iter_mut()
                .zip(std::mem::take(&mut self.restored))
            {
                f.added = m;
            }
        }
        self.known = world.bodies.len();
    }

    /// The water moving with each floating shape (kg of added mass), for a save.
    pub fn water(&self) -> Vec<f64> {
        if self.floaters.is_empty() {
            return self.restored.clone();
        }
        self.floaters.iter().map(|f| f.added).collect()
    }

    /// The water moving with body `index`'s floating shapes (none if they are not met yet).
    pub fn water_of(&self, index: usize) -> Vec<f64> {
        (self.floaters.iter())
            .filter(|f| f.body == index)
            .map(|f| f.added)
            .collect()
    }

    /// Body `index`, put back into the world, had `water` moving with it: given back to its
    /// floaters when they are met.
    pub fn expect(&mut self, index: usize, water: Vec<f64>) {
        if !water.is_empty() {
            self.expected.push((index, water));
        }
    }

    /// The water a save holds, given back to the floaters as they are met (in the same order).
    pub fn restore(&mut self, water: Vec<f64>) {
        self.restored = water;
    }

    /// Before the world's step: the air's and the sea's forces on.
    pub fn push(&mut self, world: &mut World) {
        self.meet(world);
        if let Some(sky) = &self.sky {
            // (Air only: under water the sea's floaters drag the hull.)
            aero::push(sky, None, world, &mut self.frames);
        }
        if let Some(sea) = &self.sea {
            sea::push(sea, world, &mut self.floaters);
        }
    }

    /// After the world's step of `h`: off again (last on, first off).
    pub fn settle(&mut self, world: &mut World, h: f64) {
        if self.sea.is_some() {
            sea::settle(world, &mut self.floaters, h);
        }
        if self.sky.is_some() {
            aero::settle(world, &mut self.frames);
        }
    }

    /// After the world dropped bodies (`map`: each old body's new index, if kept).
    pub fn retain(&mut self, map: &[Option<usize>]) {
        self.frames.retain_mut(|f| {
            map.get(f.body)
                .copied()
                .flatten()
                .map(|b| f.body = b)
                .is_some()
        });
        self.floaters.retain_mut(|f| {
            map.get(f.body)
                .copied()
                .flatten()
                .map(|b| f.body = b)
                .is_some()
        });
        let spared = std::mem::take(&mut self.spared);
        self.spared = vec![false; map.iter().flatten().count()];
        for (old, new) in map.iter().enumerate() {
            if let (Some(new), Some(&s)) = (new, spared.get(old)) {
                self.spared[*new] = s;
            }
        }
        self.known = self.spared.len().max(map.iter().flatten().count());
    }
}

/// A hull's surface area (m²): its faces' polygons.
fn hull_surface(hull: &creeer_physics::world::hull::Hull) -> f64 {
    (0..hull.faces().len())
        .map(|f| {
            let ring: Vec<DVec3> = hull.ring(f).collect();
            let twice: DVec3 = (1..ring.len().saturating_sub(1))
                .map(|k| (ring[k] - ring[0]).cross(ring[k + 1] - ring[0]))
                .sum();
            0.5 * twice.length()
        })
        .sum()
}
