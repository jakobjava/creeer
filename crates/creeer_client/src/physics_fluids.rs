//! F6.6 fluid scenes in the physics sandbox, in the home planet's air:
//! - **air drop:** a crate coming down under its canopy, and a glider launched from 25 m;
//! - **harbour:** a sea 3 m deep over the ground: a pontoon floating at its draft, a boat that
//!   springs a leak after 6 s and sinks, a tall box that capsizes.

use creeer_fluids::aero::{Section, Shape as AeroShape, Surface};
use creeer_fluids::atmosphere::Atmosphere;
use creeer_materials::species::SpeciesTable;
use creeer_math::glam::{DMat3, DQuat, DVec3};
use creeer_physics::world::shape::Shape;
use creeer_physics::world::{Body, World, rotation};
use creeer_sim::aero::{self, Airframe, Datum, Sky};
use creeer_sim::sea::flood::Flood;
use creeer_sim::sea::{self, Floater, Level, Sea};
use creeer_terrain::Planet;

/// The harbour's sea level (m above the ground, its bed).
const SEA_LEVEL: f64 = 3.0;
/// When the boat springs its leak (s).
const LEAK_AT: f64 = 6.0;

/// A part to draw: centre, orientation, half extents.
pub type Part = (DVec3, DQuat, DVec3);

/// A scene's air and water.
#[derive(Debug)]
pub struct SceneFluids {
    sky: Sky,
    sea: Option<Sea>,
    frames: Vec<Airframe>,
    floaters: Vec<Floater>,
    leak: Option<Flood>,
    time: f64,
    /// Bodies to name in the label: the first, the second, the third.
    bodies: Vec<usize>,
}

fn home_sky() -> (Sky, Planet) {
    let planet = Planet::load(&crate::content::data(), "planets/home.ron").expect("home.ron loads");
    let data = planet.atmosphere.clone().expect("home has air");
    let atmosphere = Atmosphere::new(
        &data,
        planet.radius,
        planet.mu,
        &SpeciesTable::from_data(&crate::content::data()).expect("the species load"),
    )
    .expect("its air");
    (
        Sky {
            atmosphere,
            datum: Datum::Flat,
            wind: DVec3::ZERO,
        },
        planet,
    )
}

fn panel(
    area: f64,
    aspect: f64,
    section: Section,
    normal: DVec3,
    chord: DVec3,
    offset: DVec3,
) -> Surface {
    Surface {
        shape: AeroShape::Panel {
            area,
            aspect,
            efficiency: 0.9,
            section,
            normal,
            chord,
        },
        offset,
    }
}

/// The air drop: an 80 kg crate under a 25 m² canopy 5 m above it, dropped from 60 m; a
/// 12 kg glider (a 4 m wing of NACA 2412, a tailplane and a fin) launched level at 14 m/s.
pub fn air_drop(world: &mut World) -> SceneFluids {
    let (sky, _) = home_sky();
    let dt = world.dt();
    let drop = world.add(Body::rigid(
        Shape::Box {
            half: DVec3::splat(0.4),
        },
        80.0,
        DVec3::new(0.0, 60.0, 0.0),
        dt,
    ));
    let mut glider = Body::rigid(
        Shape::Box {
            half: DVec3::new(1.0, 0.12, 0.12),
        },
        12.0,
        DVec3::new(-20.0, 25.0, 6.0),
        dt,
    );
    glider.structure.velocity[0] = DVec3::X * 14.0;
    let glider = world.add(glider);
    let frames = vec![
        Airframe::new(
            drop,
            0,
            Surface {
                shape: AeroShape::Bluff {
                    area: 0.64,
                    drag: 1.05,
                },
                offset: DVec3::ZERO,
            },
        ),
        Airframe::new(
            drop,
            0,
            Surface {
                shape: AeroShape::Bluff {
                    area: 25.0,
                    drag: 1.3,
                },
                offset: DVec3::Y * 5.0,
            },
        ),
        // Wing just ahead of the centre of mass, tail behind: a static margin of 6 % of the chord.
        Airframe::new(
            glider,
            0,
            panel(
                1.6,
                10.0,
                Section::NACA_2412,
                DVec3::Y,
                DVec3::X,
                DVec3::new(0.1, 0.12, 0.0),
            ),
        ),
        Airframe::new(
            glider,
            0,
            panel(
                0.3,
                4.0,
                Section::PLATE,
                DVec3::Y,
                DVec3::X,
                DVec3::new(-0.9, 0.0, 0.0),
            ),
        ),
        Airframe::new(
            glider,
            0,
            panel(
                0.15,
                1.5,
                Section::PLATE,
                DVec3::Z,
                DVec3::X,
                DVec3::new(-0.9, 0.15, 0.0),
            ),
        ),
    ];
    SceneFluids {
        sky,
        sea: None,
        frames,
        floaters: Vec::new(),
        leak: None,
        time: 0.0,
        bodies: vec![drop, glider],
    }
}

/// The harbour: a 4 × 0.8 × 2 m pontoon of 2 t, a 3 × 1 × 1.4 m steel boat of 2 t (its plates
/// 0.25 m³, so 3.95 m³ inside to flood) that springs a 20 cm leak in its bottom: full, it weighs
/// 6.1 t against the 4.3 t it can displace, and sinks (a wooden one, its walls lighter than water,
/// would float flooded), a 0.6 × 2 × 0.6 m box of 300 kg let go heeled 5° (unstable upright).
pub fn harbour(world: &mut World) -> SceneFluids {
    let (sky, planet) = home_sky();
    let water = planet.ocean.expect("home has a sea");
    let sea = Sea {
        level: Level::Flat(SEA_LEVEL),
        water,
        surface_pressure: 101_325.0,
    };
    let dt = world.dt();
    let mut floaters = Vec::new();
    let mut put = |world: &mut World, half: DVec3, mass: f64, at: DVec3, heel: f64| {
        let mut body = Body::rigid(Shape::Box { half }, mass, at, dt);
        body.structure.orientation[0] = rotation(DVec3::Z, heel);
        let index = world.add(body);
        floaters.push(Floater::new(index, 0, Shape::Box { half }, 1.0));
        index
    };
    let pontoon = put(
        world,
        DVec3::new(2.0, 0.4, 1.0),
        2000.0,
        DVec3::new(-5.0, SEA_LEVEL + 0.6, 0.0),
        0.0,
    );
    let boat = put(
        world,
        DVec3::new(1.5, 0.5, 0.7),
        2000.0,
        DVec3::new(1.0, SEA_LEVEL + 0.6, 0.0),
        0.0,
    );
    let tall = put(
        world,
        DVec3::new(0.3, 1.0, 0.3),
        300.0,
        DVec3::new(6.0, SEA_LEVEL + 0.2, 0.0),
        5f64.to_radians(),
    );
    let leak = Flood::new(
        boat,
        0,
        DVec3::new(0.0, -0.45, 0.0),
        0.25 * std::f64::consts::PI * 0.2 * 0.2,
        4.0,
        3.95,
    );
    SceneFluids {
        sky,
        sea: Some(sea),
        frames: Vec::new(),
        floaters,
        leak: Some(leak),
        time: 0.0,
        bodies: vec![pontoon, boat, tall],
    }
}

impl SceneFluids {
    /// Before the world's step: the air's and the water's forces on.
    pub fn before(&mut self, world: &mut World, dt: f64) {
        if let (Some(leak), Some(sea)) = (self.leak.as_mut(), self.sea.as_ref())
            && self.time >= LEAK_AT
        {
            leak.push(sea, world, dt);
        }
        aero::push(&self.sky, self.sea.as_ref(), world, &mut self.frames);
        if let Some(sea) = &self.sea {
            sea::push(sea, world, &mut self.floaters);
        }
    }

    /// After the world's step: off again.
    pub fn after(&mut self, world: &mut World, dt: f64) {
        if self.sea.is_some() {
            sea::settle(world, &mut self.floaters, dt);
        }
        aero::settle(world, &mut self.frames);
        if let Some(leak) = self.leak.as_mut()
            && self.time >= LEAK_AT
        {
            leak.settle(world);
        }
        self.time += dt;
    }

    /// What to draw besides the bodies: canopies and panels (centre, orientation, half
    /// extents), and the water (centre, half extents).
    pub fn parts(&self, world: &World) -> (Vec<Part>, Option<(DVec3, DVec3)>) {
        let mut parts = Vec::new();
        for f in &self.frames {
            let s = &world.bodies[f.body].structure;
            let q = s.orientation[f.node];
            let at = s.position[f.node] + q * f.surface.offset;
            match f.surface.shape {
                AeroShape::Panel {
                    area,
                    aspect,
                    normal,
                    chord,
                    ..
                } => {
                    let (span, length) = ((area * aspect).sqrt(), (area / aspect).sqrt());
                    let frame = DMat3::from_cols(chord, normal, chord.cross(normal));
                    let turn = q * DQuat::from_mat3(&frame);
                    parts.push((at, turn, DVec3::new(0.5 * length, 0.01, 0.5 * span)));
                }
                AeroShape::Bluff { area, .. } if f.surface.offset != DVec3::ZERO => {
                    let side = 0.5 * area.sqrt();
                    parts.push((at, q, DVec3::new(side, 0.05, side)));
                }
                _ => {}
            }
        }
        let water = self.sea.as_ref().map(|_| {
            (
                DVec3::new(0.0, 0.5 * SEA_LEVEL, 0.0),
                DVec3::new(20.0, 0.5 * SEA_LEVEL, 12.0),
            )
        });
        (parts, water)
    }

    /// What the scene shows.
    pub fn label(&self, world: &World) -> String {
        let at = |b: usize| world.bodies[b].structure.position[0];
        let up = |b: usize| DMat3::from_quat(world.bodies[b].structure.orientation[0]) * DVec3::Y;
        if self.sea.is_none() {
            let (drop, glider) = (self.bodies[0], self.bodies[1]);
            let v = world.bodies[drop].structure.velocity[0];
            let g = world.bodies[glider].structure.velocity[0];
            return format!(
                "crate at {:.1} m falling {:.1} m/s · glider at {:.1} m, {:.1} m/s, sinking {:.2} m/s (glide ratio {:.1})",
                at(drop).y,
                -v.y,
                at(glider).y,
                g.length(),
                -g.y,
                (g.x * g.x + g.z * g.z).sqrt() / (-g.y).max(1e-6)
            );
        }
        let (pontoon, boat, tall) = (self.bodies[0], self.bodies[1], self.bodies[2]);
        let draft = SEA_LEVEL - (at(pontoon).y - 0.4);
        let inside = self.leak.as_ref().map_or(0.0, |l| l.water);
        format!(
            "pontoon draft {draft:.3} m (Archimedes 0.244 m) · leaking boat: {inside:.2} m³ in, its deck {:+.2} m from the sea · tall box heeled {:.0}°",
            at(boat).y + 0.5 - SEA_LEVEL,
            creeer_math::det::acos(up(tall).y.clamp(-1.0, 1.0)).to_degrees()
        )
    }
}
