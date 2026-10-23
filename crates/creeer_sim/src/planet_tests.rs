//! Physics on the reference planet's real terrain.

use crate::{TerrainGround, planet_gravity};
use creeer_math::glam::{DMat3, DVec3};
use creeer_physics::world::shape::Shape;
use creeer_physics::world::{Body, World};
use creeer_terrain::edits::{Brush, Edit};
use creeer_terrain::elevation::fibonacci_sphere;
use creeer_terrain::{Planet, Terrain};
use std::sync::Arc;

const DT: f64 = 1.0 / 60.0;

fn terrain() -> Arc<Terrain> {
    Arc::new(Terrain::new(
        Planet::load(&creeer_data::Data::base(), "planets/home.ron").expect("home.ron loads"),
    ))
}

/// A surface point on land whose slope lies in `degrees`, and the ground normal there.
fn spot(terrain: &Terrain, degrees: (f64, f64)) -> (DVec3, DVec3) {
    fibonacci_sphere(4000)
        .find_map(|up| {
            let height = terrain.surface_height(up);
            if height < 20.0 {
                return None;
            }
            let p = up * (terrain.planet().datum.radius(up) + height);
            let (_, gradient) = terrain.density_gradient(p);
            let normal = gradient.normalize();
            let slope = normal.angle_between(up).to_degrees();
            (degrees.0..degrees.1)
                .contains(&slope)
                .then_some((p, normal))
        })
        .expect("such a spot exists")
}

fn world(terrain: &Arc<Terrain>) -> World {
    let mut world = World::new(DT);
    world.planet = Some(planet_gravity(terrain.planet()));
    world.ground = Some(Arc::new(TerrainGround(Arc::clone(terrain))));
    world
}

/// The deepest any point of a box's surface lies inside the terrain (m, 0 if none).
fn penetration(terrain: &Terrain, world: &World, half: f64) -> f64 {
    let s = &world.bodies[0].structure;
    let r = DMat3::from_quat(s.orientation[0]);
    let mut deepest = 0.0f64;
    for i in 0..=10 {
        for j in 0..=10 {
            for k in 0..=10 {
                let local = DVec3::new(f64::from(i), f64::from(j), f64::from(k)) / 5.0 - 1.0;
                if local.abs().max_element() < 0.999 {
                    continue; // inside the box
                }
                let d = terrain.density(s.position[0] + r * (local * half));
                deepest = deepest.min(d);
            }
        }
    }
    -deepest
}

fn crate_at(position: DVec3) -> Body {
    Body::rigid(
        Shape::Box {
            half: DVec3::splat(0.5),
        },
        200.0,
        position,
        DT,
    )
}

#[test]
fn a_crate_dropped_on_the_terrain_comes_to_rest_on_it() {
    let terrain = terrain();
    let (p, normal) = spot(&terrain, (2.0, 12.0));
    let mut world = world(&terrain);
    world.add(crate_at(p + normal * 2.0));
    for _ in 0..360 {
        world.step();
    }
    assert!(world.bodies[0].sleep.asleep, "comes to rest within 6 s");
    let deepest = penetration(&terrain, &world, 0.5);
    assert!(
        deepest < 0.005,
        "the terrain pokes {deepest} m into the crate"
    );
    // It rests on the ground, not above it: its centre is within a box diagonal.
    let centre = world.bodies[0].structure.position[0];
    assert!(
        terrain.density(centre) < 0.9,
        "floats {} m up",
        terrain.density(centre)
    );
}

#[test]
fn a_ball_rolls_downhill_without_gaining_energy() {
    let terrain = terrain();
    let (p, normal) = spot(&terrain, (12.0, 25.0));
    let mut world = world(&terrain);
    world.static_friction = 0.8;
    let mut ball = Body::rigid(Shape::Sphere { radius: 0.3 }, 30.0, p + normal * 0.31, DT);
    ball.friction = 0.8;
    world.add(ball);
    world.step();
    let start_energy = world.energy();
    let start = world.bodies[0].structure.position[0];
    let kinetic = |w: &World| w.bodies[0].structure.node_energy().0;
    let mut previous = start_energy;
    for tick in 0..180 {
        world.step();
        let e = world.energy();
        // Rolling loses nothing in principle; contacts and steps may only take energy away.
        // The frozen contact plane of a step differs from the curved terrain by ~d²·κ, so
        // as in ADR-0005 a rise of 10⁻³ of the energy in motion is allowed per tick.
        let slack = 1e-3 * kinetic(&world) + 1e-6;
        assert!(
            e <= previous + slack,
            "tick {tick}: energy rose {previous} -> {e} J"
        );
        previous = e;
    }
    assert!(
        previous <= start_energy + 1e-3 * kinetic(&world),
        "energy gained overall"
    );
    let end = world.bodies[0].structure.position[0];
    assert!(end.length() < start.length() - 1.0, "rolled downhill");
    assert!(
        world.bodies[0].structure.angular_velocity[0].length() > 0.5,
        "it rolls"
    );
}

#[test]
fn digging_under_a_resting_crate_drops_it_into_the_hole() {
    let terrain = terrain();
    let (p, normal) = spot(&terrain, (0.0, 6.0));
    let mut world = world(&terrain);
    world.add(crate_at(p + normal * 0.6));
    for _ in 0..240 {
        world.step();
    }
    assert!(world.bodies[0].sleep.asleep);
    let rest = world.bodies[0].structure.position[0];
    terrain.edit(Edit::Dig(Brush::Sphere {
        center: p - normal * 0.5,
        radius: 2.5,
    }));
    world.wake_near(p, 3.0);
    for _ in 0..240 {
        world.step();
    }
    let end = world.bodies[0].structure.position[0];
    assert!(
        (rest - end).dot(normal) > 1.0,
        "fell {} m",
        (rest - end).dot(normal)
    );
    assert!(
        penetration(&terrain, &world, 0.5) < 0.005,
        "rests on the hole's floor"
    );
}

#[test]
fn planet_physics_is_bit_identical_across_platforms() {
    // Golden hash: crates and a ball dropped on the real terrain in the rotating frame
    // (terrain heights, planet gravity, Coriolis, contacts). CI runs it on Windows, Linux and
    // macOS; if it changes on purpose, update the constant.
    let terrain = terrain();
    let (p, normal) = spot(&terrain, (2.0, 15.0));
    let mut world = world(&terrain);
    for i in 0..3 {
        world.add(crate_at(p + normal * (1.0 + 1.2 * f64::from(i))));
    }
    world.add(Body::rigid(
        Shape::Sphere { radius: 0.3 },
        30.0,
        p + normal * 5.0,
        DT,
    ));
    for _ in 0..120 {
        world.step();
    }
    let hash = world
        .bodies
        .iter()
        .fold(0u64, |h, b| h.rotate_left(13) ^ b.structure.state_hash());
    // Regenerated once, for a deliberate change of the model (ADR-0007): the Coriolis force
    // from the velocity extrapolated to the end of the step, and the rotating frame's torque
    // on spinning bodies. With both switched off the old value (0x6439_5b38_a0ac_6ddd)
    // came back exactly. Regenerated again in F5 for the new terrain shape (monotone continent
    // curve, rounded ridge crests; `elevation.rs`): with the old elevation the previous value
    // (0xcca2_311e_b5a7_1167) came back exactly. And for the F5 contacts (ADR-0005 amended:
    // Hertz and face patches from the materials, the resolvable stiffness limit, critically
    // damped dashpots, round bodies measured against the ground itself, convergence to 1 %
    // of the deepest contact's force; the terrain's exact gradient away from edits), after
    // 0xf482_3148_3885_64ad; then for dashpots on the gap's own rate and contacts at zero gap
    // counting as touching, after 0x3f99_3c02_7f7d_3aa5 (which those two reverted gave back);
    // then for one Newton block per patch instead of one per corner and corner pair (summed
    // before scaling: rounding only; kept apart, 0xb059_c3b8_ddda_7fd5 came back); then for
    // patches as the material in series with the limit (`patch.rs`), dashpots on the gap's
    // rate along its own history, patch blocks as rank-one products, the solver's stricter
    // stop and the contacts' limits set before any energy is measured, after
    // 0x3e10_ca3d_3687_df04 (these were made together and not reverted one by one); then for
    // a patch's products grouped by corner (rounding only; pairwise, 0xcd3a_e134_88b3_b01b
    // came back); then for contacts stiffening under the load they carry at rest, steps redone
    // with the substeps a box's turn needed, lifted patch corners without a dashpot, and the
    // step's tolerance from the most loaded contact's F/k, after 0xcb56_b163_9b50_9eb7; then
    // for the contact law revised (ADR-0005, 2026-10-17: lifted patch corners' forces applied,
    // friction in impact impulses, face dashpots on pressed points, normals turning with their
    // bodies, faces on the ground measured at their triangles' corners and pressing as a punch
    // of their pressed area, faces counted by any part within the margin), after
    // 0x0b2b_3e03_525c_b032 (made together, not reverted one by one). Then for rotations
    // carried as momentum through the torques' share of a step (F6.4, `structure/step.rs`),
    // after 0x0949_6c57_efc7_e0f5 (which the old predictor and end velocity gave back). Then for
    // rigid bodies' contacts solved by Newton's method proper (their stiffness at each
    // iteration's state), the step's tolerance no finer than the positions resolve, the energy
    // contacts held carried to the next step's guard, and drilling friction (F6, the rock pile's
    // lag), after 0xc08c_b8e6_5929_d6c3 (made together, not reverted one by one).
    // Then for the speed work of 2026-10-18 (contact geometry kept per state, the line
    // search's first trial where an open contact closes and its quadratic backtracking,
    // factorisations judged without the joints a joint solve takes exactly): the same
    // converged steps by another path, the bodies within 0.01 mm of the old ones after half
    // a second and 0.6 mm after two (crates settling on terrain), after 0x71ef_08e3_442a_4dc9.
    // Then for the contacts' lever arms and normals in the matrices measured at each iterate
    // (not the step's start): the bodies within 0.1 mm after two seconds, after
    // 0xccbf_8f27_2ce7_305c. Then for turning contacts starting where they are found, so that
    // the energy guard measures them as the step does (ADR-0024): the bodies within 5.6 mm
    // after two seconds, after 0x95c9_9d4f_d3cf_d727 (which the old start gave back). Then for
    // the turn's substeps only for boxes the step can bring into contact, and for their groups
    // alone (G1.8, ADR-0024): the bodies within 2.4 µm after two seconds, after
    // 0x4286_a939_2f61_b08c.
    assert_eq!(
        hash, 0xf693_96db_e738_02d4,
        "planet physics hash {hash:#018x}"
    );
}
