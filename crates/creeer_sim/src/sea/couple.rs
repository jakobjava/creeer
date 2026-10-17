//! The sea's step: buoyancy, drag and added mass on before the world's step, off after it,
//! the water's work booked (as the air's is, `aero.rs`).

use super::volume::wetted;
use super::{ADDED, Floater, Sea, effective_gravity, waterline};
use creeer_math::det;
use creeer_math::glam::{DMat3, DVec3};
use creeer_physics::structure::{AddedMass, Dashpot, Scheme};
use creeer_physics::world::World;

fn cross(r: DVec3) -> DMat3 {
    DMat3::from_cols(
        DVec3::new(0.0, r.z, -r.y),
        DVec3::new(-r.z, 0.0, r.x),
        DVec3::new(r.y, -r.x, 0.0),
    )
}

/// −k·|u|·u and its slope k·(|u|·I + u·uᵀ/|u|).
fn quadratic(k: f64, u: DVec3) -> (DVec3, DMat3) {
    let speed = u.length();
    if speed <= 0.0 {
        return (DVec3::ZERO, DMat3::ZERO);
    }
    let outer = DMat3::from_cols(u * u.x, u * u.y, u * u.z) / speed;
    (-u * (k * speed), (DMat3::IDENTITY * speed + outer) * k)
}

/// Before the world's step: each floater's buoyancy and drag on its node, the drag's slope
/// as a dashpot, the water moving with it as added mass.
pub fn push(sea: &Sea, world: &mut World, floaters: &mut [Floater]) {
    let rho = sea.water.density;
    for f in floaters.iter_mut() {
        // A sleeping body is not stepped: nothing on it (it wakes when something touches it).
        if world.bodies[f.body].sleep.asleep {
            // (Its history restarts when it wakes: no end force carried into that step.)
            (f.force, f.torque, f.dashpot, f.end, f.idle) =
                (DVec3::ZERO, DVec3::ZERO, None, DVec3::ZERO, true);
            continue;
        }
        f.idle = false;
        let (rotation, x, v, w) = {
            let s = &world.bodies[f.body].structure;
            let n = f.node;
            (
                DMat3::from_quat(s.orientation[n]),
                s.position[n],
                s.velocity[n],
                s.angular_velocity[n],
            )
        };
        let wet = wetted(&f.shape, &waterline(sea, rotation, x));
        let (volume, centre) = (wet.volume, wet.centre);
        f.volume = volume;
        f.centre = centre;
        let arm = rotation * centre;
        (f.start, f.origin) = (x + arm, x);
        let g = effective_gravity(world, f.body, x + arm);
        let buoyancy = -g * (rho * volume);
        let u = v + w.cross(arm);
        let area = wet.projected(rotation.transpose() * u);
        let (drag, d) = quadratic(0.5 * rho * f.drag * area, u);
        let (radiated, d_wave) = heave_waves(
            sea,
            world.bodies[f.body].structure.masses()[f.node] + ADDED * rho * volume,
            wet.waterplane,
            g.length(),
            sea.surface(x).1,
            u,
        );
        let (drag, d) = (drag + radiated, d + d_wave);
        let size = det::cbrt(volume);
        let (turning, d_turn) = quadratic(0.5 * rho * f.drag * size * size * size * size * size, w);
        f.force = buoyancy + drag;
        f.torque = arm.cross(f.force) + turning;
        let added = ADDED * rho * volume;
        // Water it entered is at rest; water it leaves keeps its motion.
        let momentum = v * added.min(f.added);
        f.added = added;
        let s = &mut world.bodies[f.body].structure;
        s.load[f.node] += f.force;
        s.torque[f.node] += f.torque;
        if volume > 0.0 {
            let r = cross(arm);
            let blocks = [[d, -(d * r)], [r * d, -(r * d * r) + d_turn]];
            let mut matrix = [[0.0; 6]; 6];
            for (i, row) in matrix.iter_mut().enumerate() {
                for (j, m) in row.iter_mut().enumerate() {
                    *m = blocks[i / 3][j / 3].col(j % 3)[i % 3];
                }
            }
            let dashpot = Dashpot {
                node: f.node,
                matrix,
                velocity: v,
                angular: w,
            };
            s.dashpots.push(dashpot);
            f.dashpot = Some(dashpot);
            s.added_mass.push(AddedMass {
                node: f.node,
                matrix: DMat3::IDENTITY * added,
                momentum,
            });
        }
    }
}

/// After the world's step of `h`: all of it off again, and the work the water did booked
/// (the loads and dashpots by the force the step's scheme took, the added mass's impulse at
/// the mean velocity).
pub fn settle(world: &mut World, floaters: &mut [Floater], h: f64) {
    for f in floaters.iter_mut().rev() {
        let n = f.node;
        world.bodies[f.body].take_load(n, f.force);
        let s = &mut world.bodies[f.body].structure;
        s.torque[n] -= f.torque;
        if f.idle {
            continue;
        }
        let point = s.position[n] + DMat3::from_quat(s.orientation[n]) * f.centre;
        let moved = point - f.start;
        let Some(d) = f.dashpot.take() else {
            f.work += 0.5 * (f.end + f.force).dot(moved);
            f.end = f.force;
            continue;
        };
        if let Some(k) = s.dashpots.iter().rposition(|x| *x == d) {
            s.dashpots.remove(k);
        }
        let mut water = None;
        if let Some(k) = s.added_mass.iter().rposition(|a| a.node == n) {
            water = Some(s.added_mass.remove(k));
        }
        let (dv, dw) = (
            s.velocity[n] - d.velocity,
            s.angular_velocity[n] - d.angular,
        );
        let delta = [dv.x, dv.y, dv.z, dw.x, dw.y, dw.z];
        let pushed = DVec3::from_array(std::array::from_fn(|i| {
            (0..6).map(|j| d.matrix[i][j] * delta[j]).sum()
        }));
        let end = f.force - pushed;
        let taken = match world.bodies[f.body].solver.last_scheme() {
            Scheme::Newmark => 0.5 * (f.end + f.force) - pushed,
            _ => end,
        };
        f.work += taken.dot(moved);
        let s = &world.bodies[f.body].structure;
        if let Some(a) = water {
            f.work -= (a.matrix * s.velocity[n] - a.momentum).dot(s.position[n] - f.origin) / h;
        }
        f.end = end;
    }
}

/// The waves a heaving body makes carry its heave's energy away: damping B·(u·n)·n along the
/// sea's normal n. B = ρ·ω³·A²/(2g), the long-wave limit of the Haskind–Newman relation for
/// a body of waterplane A heaving at its own frequency ω = √(ρ·g·A/(m + m_a)) in deep water
/// (exact as k·a → 0, k = ω²/g, a = √(A/π)); rolled off by 1/(1 + (k·a)²) for shorter waves,
/// where it overstates (a box of 1 m² and 0.23 m draft heaves at ζ ≈ 0.6 by it; measured
/// boxes of its shape, 0.3–0.5).
fn heave_waves(
    sea: &Sea,
    mass: f64,
    waterplane: f64,
    g: f64,
    up: DVec3,
    u: DVec3,
) -> (DVec3, DMat3) {
    if waterplane <= 0.0 {
        return (DVec3::ZERO, DMat3::ZERO);
    }
    let rho = sea.water.density;
    let omega = (rho * g * waterplane / mass).sqrt();
    let ka = omega * omega / g * (waterplane / std::f64::consts::PI).sqrt();
    let b = rho * omega * omega * omega * waterplane * waterplane / (2.0 * g) / (1.0 + ka * ka);
    (
        -up * (b * u.dot(up)),
        DMat3::from_cols(up * up.x, up * up.y, up * up.z) * b,
    )
}
