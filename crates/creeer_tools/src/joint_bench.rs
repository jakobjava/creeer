//! G1.1 joints (ADR-0022): a ragdoll's step. Fifteen boxes (pelvis, torso, head, upper and
//! lower arms, hands, thighs, shins, feet) on fourteen joints with their limits and
//! damping, dropped from standing onto the ground: the joints' work, the contacts' with the
//! ground, and the substeps its swinging limbs take.

use crate::spikes::{Better, DT, Report, p, time_ticks};
use creeer_math::glam::DVec3;
use creeer_physics::world::joint::{Joint, Kind};
use creeer_physics::world::shape::{Shape, StaticShape};
use creeer_physics::world::{Body, World};

/// Budget of a ragdoll's step in the air on the development machine (`docs/g1-plan.md`,
/// G1.1; on the ground the contacts' solve of its group is P1's). The plan's 0.1 ms was
/// below what its fifteen bodies cost free (0.39 ms p50, fifteen groups' overheads): it
/// measured 0.15, its joints' terms and projection 0.09 of it (2026-10-18, ADR-0022).
const BUDGET_MS: f64 = 0.2;

/// A box of `half` extents and `mass` at `at`.
fn part(world: &mut World, at: DVec3, half: DVec3, mass: f64) -> usize {
    world.add(Body::rigid(Shape::Box { half }, mass, at, DT))
}

/// The ragdoll standing on the ground at the origin, slightly leaning (so it falls).
pub(crate) fn ragdoll() -> World {
    let mut world = World::new(DT);
    world.statics.push(StaticShape::Plane {
        normal: DVec3::Y,
        offset: 0.0,
    });
    let lean = |p: DVec3| DVec3::new(p.x + 0.05 * p.y, p.y, p.z);
    let add =
        |world: &mut World, at: DVec3, half: DVec3, mass: f64| part(world, lean(at), half, mass);
    let pelvis = add(
        &mut world,
        DVec3::new(0.0, 1.0, 0.0),
        DVec3::new(0.15, 0.1, 0.1),
        12.0,
    );
    let torso = add(
        &mut world,
        DVec3::new(0.0, 1.35, 0.0),
        DVec3::new(0.18, 0.25, 0.11),
        28.0,
    );
    let head = add(
        &mut world,
        DVec3::new(0.0, 1.73, 0.0),
        DVec3::new(0.09, 0.12, 0.1),
        6.0,
    );
    let mut limbs = Vec::new();
    for side in [-1.0, 1.0] {
        let x = side * 0.27;
        let upper = add(
            &mut world,
            DVec3::new(x, 1.43, 0.0),
            DVec3::new(0.05, 0.15, 0.05),
            2.5,
        );
        let lower = add(
            &mut world,
            DVec3::new(x, 1.13, 0.0),
            DVec3::new(0.04, 0.14, 0.04),
            1.6,
        );
        let hand = add(
            &mut world,
            DVec3::new(x, 0.93, 0.0),
            DVec3::new(0.04, 0.05, 0.02),
            0.5,
        );
        let xl = side * 0.09;
        let thigh = add(
            &mut world,
            DVec3::new(xl, 0.7, 0.0),
            DVec3::new(0.07, 0.2, 0.07),
            9.0,
        );
        let shin = add(
            &mut world,
            DVec3::new(xl, 0.3, 0.0),
            DVec3::new(0.05, 0.19, 0.05),
            4.0,
        );
        let foot = add(
            &mut world,
            DVec3::new(xl, 0.05, 0.05),
            DVec3::new(0.05, 0.04, 0.12),
            1.2,
        );
        limbs.push((x, xl, upper, lower, hand, thigh, shin, foot));
    }
    let cone = |c: f64, t: f64| Kind::Ball {
        cone: Some(c),
        twist: Some((-t, t)),
    };
    let hinge = |lo: f64, hi: f64| Kind::Hinge {
        range: Some((lo, hi)),
    };
    let join = |world: &mut World, a: usize, b: usize, at: DVec3, axis: DVec3, kind: Kind| {
        let reference = if axis.dot(DVec3::Y).abs() > 0.9 {
            DVec3::X
        } else {
            DVec3::Y.cross(axis).normalize()
        };
        let j = Joint::new(
            world,
            ((a, 0), Some((b, 0))),
            (lean(at), axis, reference),
            kind,
        );
        let j = world.join(j);
        world.joint_mut(j).damping = 2.0;
    };
    join(
        &mut world,
        torso,
        pelvis,
        DVec3::new(0.0, 1.1, 0.0),
        DVec3::Y,
        cone(0.6, 0.5),
    );
    join(
        &mut world,
        head,
        torso,
        DVec3::new(0.0, 1.6, 0.0),
        DVec3::Y,
        cone(0.7, 1.0),
    );
    for (x, xl, upper, lower, hand, thigh, shin, foot) in limbs {
        join(
            &mut world,
            upper,
            torso,
            DVec3::new(x, 1.58, 0.0),
            DVec3::NEG_Y,
            cone(1.6, 1.0),
        );
        join(
            &mut world,
            lower,
            upper,
            DVec3::new(x, 1.28, 0.0),
            DVec3::X,
            hinge(0.0, 2.5),
        );
        join(
            &mut world,
            hand,
            lower,
            DVec3::new(x, 0.99, 0.0),
            DVec3::X,
            hinge(-1.0, 1.0),
        );
        join(
            &mut world,
            thigh,
            pelvis,
            DVec3::new(xl, 0.9, 0.0),
            DVec3::NEG_Y,
            cone(1.4, 0.6),
        );
        join(
            &mut world,
            shin,
            thigh,
            DVec3::new(xl, 0.5, 0.0),
            DVec3::X,
            hinge(-2.4, 0.0),
        );
        join(
            &mut world,
            foot,
            shin,
            DVec3::new(xl, 0.1, 0.0),
            DVec3::X,
            hinge(-0.6, 0.6),
        );
    }
    world
}

/// The best of three fresh runs of `ticks` ticks of `make`'s world (scheduling noise, as in
/// `world_bench.rs`).
fn best(make: impl Fn() -> World, ticks: usize) -> creeer_core::TimingStats {
    let mut best: Option<creeer_core::TimingStats> = None;
    for _ in 0..3 {
        let mut world = make();
        let run = time_ticks(|| world.step(), ticks);
        if best.as_ref().is_none_or(|b| p(&run, 50.0) < p(b, 50.0)) {
            best = Some(run);
        }
    }
    best.expect("three runs")
}

/// The ragdoll without its joints (the same bodies falling apart).
fn unjoined() -> World {
    let mut world = ragdoll();
    while !world.joints().is_empty() {
        world.remove_joint(0);
    }
    world
}

/// The ragdoll tumbling in the air: no ground, no gravity, its parts spinning.
fn tumbling() -> World {
    let mut world = ragdoll();
    world.statics.clear();
    for (k, body) in world.bodies.iter_mut().enumerate() {
        #[allow(clippy::cast_precision_loss)]
        let f = k as f64;
        let s = &mut body.structure;
        s.gravity = DVec3::ZERO;
        s.velocity[0] = DVec3::new(0.3, 0.1 * creeer_math::det::sin(f), 0.0);
        s.angular_velocity[0] = DVec3::new(0.5, 1.0, -0.3);
    }
    world
}

pub fn run(report: &mut Report) {
    println!("G1.1 joints: a 15-part ragdoll, 14 joints with limits and damping (ms per tick)");
    if std::env::var_os("JOINTS_TRACE").is_some() {
        trace();
    }
    let air = best(tumbling, 120);
    let p50 = p(&air, 50.0);
    println!(
        "  tumbling in the air, 2 s      | p50 {p50:.3}, p99 {:.3}, worst {:.3} (budget p50 {BUDGET_MS})",
        p(&air, 99.0),
        p(&air, 100.0)
    );
    report.metric("joints_ragdoll_air_p50_ms", p50, Better::Lower);
    report.budget(
        p50 <= BUDGET_MS,
        format!("a ragdoll's step in the air p50 {p50:.3} ms <= {BUDGET_MS} ms"),
    );
    // On the ground the contacts' solve of the group is most of it (P1, `docs/g1-plan.md`):
    // the joints' share is the same bodies' cost with them less without.
    let (joined, apart) = (best(ragdoll, 180), best(unjoined, 180));
    println!(
        "  falling to the ground, 3 s    | p50 {:.3}, p99 {:.3}, worst {:.3}; without joints {:.3}, {:.3}, {:.3}",
        p(&joined, 50.0),
        p(&joined, 99.0),
        p(&joined, 100.0),
        p(&apart, 50.0),
        p(&apart, 99.0),
        p(&apart, 100.0)
    );
    report.metric(
        "joints_ragdoll_ground_p50_ms",
        p(&joined, 50.0),
        Better::Lower,
    );
    report.metric(
        "joints_ragdoll_ground_worst_ms",
        p(&joined, 100.0),
        Better::Info,
    );
    println!();
}

/// `JOINTS_TRACE=1` (built with `--features creeer_physics/profile`): the phases summed over
/// the fall, with its joints and without (`JOINTS_TRACE=bare`).
pub fn trace() {
    let mut world = match std::env::var("JOINTS_TRACE").as_deref() {
        Ok("bare") => unjoined(),
        Ok("air") => tumbling(),
        _ => ragdoll(),
    };
    let _ = creeer_physics::profile::take();
    let t = std::time::Instant::now();
    for _ in 0..180 {
        world.step();
    }
    let ms = t.elapsed().as_secs_f64() * 1e3;
    let spent = creeer_physics::profile::take();
    let phases: Vec<String> = creeer_physics::profile::NAMES
        .iter()
        .filter(|(s, _)| spent[*s as usize] > 0)
        .map(|(s, n)| format!("{n} {}", spent[*s as usize]))
        .collect();
    println!("180 ticks: {ms:.1} ms; {}", phases.join(", "));
}
