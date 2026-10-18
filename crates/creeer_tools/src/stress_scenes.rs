//! The stress scenarios (plan.md §5, F7.5): each built, run tick by tick (the closure times
//! one tick), and hashed at the end (`stress.rs`).

use crate::spikes::DT;
use creeer_math::glam::{DMat3, DVec3};
use creeer_physics::structure::section::STEEL_S235;
use creeer_physics::structure::shapes::{Base, space_frame};
use creeer_physics::structure::{ImplicitSolver, Scheme, Section, Structure, statics};
use creeer_physics::world::shape::StaticShape;
use creeer_physics::world::{Body, World};
use creeer_sim::abstraction::Abstract;
use creeer_sim::powerhouse::Powerhouse;
use rayon::prelude::*;

/// FNV-1a over bytes.
pub fn hash(bytes: impl IntoIterator<Item = u8>) -> u64 {
    bytes.into_iter().fold(0xcbf2_9ce4_8422_2325, |h, b| {
        (h ^ u64::from(b)).wrapping_mul(0x0000_0100_0000_01b3)
    })
}

fn floats<'a>(v: impl IntoIterator<Item = &'a f64>) -> Vec<u8> {
    v.into_iter().flat_map(|x| x.to_le_bytes()).collect()
}

/// A scenario: one tick, the hash of its state, what else it reports, and whether its ticks
/// keep real time (a flyover's frames: the streamer works between them).
/// What a scenario reports besides its ticks: a line, and gauges (name, value, ceiling: gated
/// under `--gate`).
#[derive(Default)]
pub struct Notes {
    pub text: String,
    pub gauges: Vec<(&'static str, f64, f64)>,
}

pub struct Scene {
    pub tick: Box<dyn FnMut(u32)>,
    pub hash: Box<dyn Fn() -> u64>,
    pub notes: Box<dyn Fn() -> Notes>,
    pub paced: bool,
}

/// 50,000 beams: ten 5,000-beam bridges on their piers, each crossed by a 20 kN load.
pub fn beams() -> Scene {
    let tube = Section::round_tube(0.2, 0.01);
    let mut s = space_frame([130, 2, 4], 2.0, tube, STEEL_S235, Base::Every(10));
    statics::nonlinear(&mut s, 200);
    let lane = crate::solvers::lane(&s);
    let bridges: Vec<(Structure, ImplicitSolver, usize)> = (0..10)
        .map(|k| {
            (
                s.clone(),
                ImplicitSolver::new(&s, Scheme::Newmark, DT, 1),
                k * lane.len() / 10,
            )
        })
        .collect();
    let bridges = std::sync::Arc::new(std::sync::Mutex::new(bridges));
    let state = std::sync::Arc::clone(&bridges);
    Scene {
        tick: Box::new(move |tick| {
            let mut b = bridges.lock().expect("bridges");
            b.par_iter_mut().for_each(|(s, solver, start)| {
                let along =
                    (f64::from(tick) * DT * 20.0 + *start as f64 * 2.0) % (lane.len() - 1) as f64;
                #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
                let k = along as usize;
                let share = along - k as f64;
                s.load.fill(DVec3::ZERO);
                s.load[lane[k]].y = -20_000.0 * (1.0 - share);
                s.load[lane[k + 1]].y = -20_000.0 * share;
                solver.step(s);
            });
        }),
        hash: Box::new(move || {
            let b = state.lock().expect("bridges");
            hash(
                b.iter()
                    .flat_map(|(s, ..)| floats(s.position.iter().flat_map(|p| [&p.x, &p.y, &p.z]))),
            )
        }),
        notes: Box::new(Notes::default),
        paced: false,
    }
}

/// Ten cars crashing: five pairs of space-frame cars (1.2 t each) meeting head on at 15 m/s
/// each, on level ground before a wall.
pub fn cars() -> Scene {
    let mut world = World::new(DT);
    world.statics.push(StaticShape::Plane {
        normal: DVec3::Y,
        offset: 0.0,
    });
    world.statics.push(StaticShape::Box {
        center: DVec3::new(18.0, 1.5, 12.0),
        rotation: creeer_math::glam::DQuat::IDENTITY,
        half: DVec3::new(0.5, 1.5, 14.0),
    });
    let tube = Section::round_tube(0.08, 0.004);
    for pair in 0..5 {
        for (side, speed) in [(-1.0, 15.0), (1.0, -15.0)] {
            let mut car = space_frame([2, 1, 1], 2.0, tube, STEEL_S235, Base::Free);
            for node in 0..car.node_count() {
                car.add_mass(node, 100.0, DMat3::ZERO);
            }
            // 20 m apart: they meet after two thirds of a second.
            let x = side * 10.0 + if side > 0.0 { 0.0 } else { -4.0 };
            car.translate(DVec3::new(x, tube.extent[0] + 0.005, f64::from(pair) * 5.0));
            for v in &mut car.velocity {
                *v = DVec3::X * speed;
            }
            world.add(Body::structure(car, DT));
        }
    }
    let world = std::sync::Arc::new(std::sync::Mutex::new(world));
    let (state, seen) = (std::sync::Arc::clone(&world), std::sync::Arc::clone(&world));
    Scene {
        tick: Box::new(move |_| {
            let mut w = world.lock().expect("world");
            let (e0, p0, f0) = (
                w.energy(),
                w.bodies.iter().map(|b| b.solver.plastic_work).sum::<f64>(),
                w.bodies.iter().map(|b| b.solver.fallbacks).sum::<u32>(),
            );
            w.step();
            let (e1, p1, f1) = (
                w.energy(),
                w.bodies.iter().map(|b| b.solver.plastic_work).sum::<f64>(),
                w.bodies.iter().map(|b| b.solver.fallbacks).sum::<u32>(),
            );
            if std::env::var_os("CARS_TRACE").is_some() {
                eprintln!(
                    "SCRATCH energy {:.1} -> {:.1} kJ (lost {:.1}), plastic +{:.3} kJ, fallbacks +{}",
                    e0 / 1e3,
                    e1 / 1e3,
                    (e0 - e1) / 1e3,
                    (p1 - p0) / 1e3,
                    f1 - f0
                );
            }
        }),
        hash: Box::new(move || hash(state.lock().expect("world").to_bytes())),
        // What the crash did: the plastic work, the members buckled and broken, the motion
        // left of the 1,350 kJ the cars started with (ground friction takes the most).
        notes: Box::new(move || {
            let world = seen.lock().expect("world");
            let bodies = &world.bodies;
            let work: f64 = bodies.iter().map(|b| b.solver.plastic_work).sum();
            let split: usize = bodies.iter().map(|b| b.structure.node_count() - 12).sum();
            let broken: usize = (bodies.iter())
                .map(|b| b.structure.members().iter().filter(|m| m.3).count())
                .sum();
            let moving: f64 = bodies.iter().map(|b| b.structure.node_energy().0).sum();
            Notes {
                text: format!(
                    "plastic work {:.1} kJ, {split} members buckled, {broken} broken, {:.1} kJ of motion left",
                    work / 1e3,
                    moving / 1e3
                ),
                gauges: Vec::new(),
            }
        }),
        paced: false,
    }
}

/// A powerhouse run in full until steady, then abstract (for factories of many).
pub fn abstract_powerhouse() -> Abstract<Powerhouse> {
    let mut one = Abstract::new(Powerhouse::new());
    while !one.try_leave() {
        for _ in 0..600 {
            one.tick();
        }
    }
    one
}

/// A large factory: the reference load's machines across every domain (docs/f6-plan.md)
/// and a hundred powerhouses, half of them active, half abstract.
pub fn factory() -> Scene {
    let mut parts = crate::reference_bench::machines();
    let snapshot = abstract_powerhouse().snapshot();
    let mut active: Vec<Abstract<Powerhouse>> =
        (0..50).map(|_| Abstract::new(Powerhouse::new())).collect();
    let resting: Vec<Abstract<Powerhouse>> = (0..50)
        .map(|_| Abstract::restore(Powerhouse::new(), &snapshot))
        .collect();
    let houses = std::sync::Arc::new(std::sync::Mutex::new((Vec::new(), resting)));
    let state = std::sync::Arc::clone(&houses);
    let mut first = true;
    Scene {
        tick: Box::new(move |tick| {
            if first {
                houses.lock().expect("houses").0 = std::mem::take(&mut active);
                first = false;
            }
            for (name, part) in &mut parts {
                let _scope = creeer_core::profile::scope(name);
                part(tick);
            }
            houses
                .lock()
                .expect("houses")
                .0
                .par_iter_mut()
                .for_each(Abstract::tick);
        }),
        hash: Box::new(move || {
            let h = state.lock().expect("houses");
            hash(h.0.iter().chain(&h.1).flat_map(|a| floats(&a.amounts())))
        }),
        notes: Box::new(Notes::default),
        paced: false,
    }
}
