//! The character against mechanics: it stands on its legs at their height, walks at the speed
//! asked for, climbs slopes up to the friction angle and slides beyond, pushes back what it
//! stands on (momentum kept), jumps as high as its legs' impulse, and lands softly from a
//! step but hard from a fall.

use super::*;
use creeer_physics::world::shape::StaticShape;

const DT: f64 = 1.0 / 60.0;
const G: f64 = 9.806_65;

/// A world with ground along `normal` through the origin, friction `mu`.
fn ground(normal: DVec3, mu: f64) -> World {
    let mut world = World::new(DT);
    world.statics.push(StaticShape::Plane {
        normal: normal.normalize(),
        offset: 0.0,
    });
    world.static_friction = mu;
    world
}

/// Steps `world` `n` times, the character `index` asking for `act(step)`.
fn run(world: &mut World, index: usize, n: usize, act: impl Fn(usize) -> (DVec3, bool)) {
    let mut reactions = Vec::new();
    for k in 0..n {
        let (walk, jump) = act(k);
        steer(
            world,
            index,
            Intent::walk(walk, jump),
            (DVec3::Y, None),
            &mut reactions,
        );
        world.step();
        release(world, &mut reactions);
    }
}

fn feet(world: &World, index: usize) -> DVec3 {
    world.bodies[index].structure.position[0] - DVec3::Y * CENTRE
}

#[test]
fn it_stands_on_its_legs_at_their_height() {
    let mut world = ground(DVec3::Y, 0.6);
    let c = world.add(body(DVec3::new(0.0, 0.05, 0.0), DVec3::Y, DT));
    run(&mut world, c, 180, |_| (DVec3::ZERO, false));
    let s = &world.bodies[c].structure;
    // The legs hold the hip at its height, pressed by the weight as their spring says.
    assert!(
        feet(&world, c).y.abs() < 0.01,
        "feet at {}",
        feet(&world, c).y
    );
    assert!(
        s.velocity[0].length() < 1e-3,
        "still moves at {}",
        s.velocity[0].length()
    );
    assert!(
        (s.orientation[0] * DVec3::Y).dot(DVec3::Y) > 0.9999,
        "leans"
    );
    // No body part touches the ground: it stands on its legs.
    assert!(world.contacts.is_empty());
}

#[test]
fn it_walks_at_the_speed_asked_for() {
    let mut world = ground(DVec3::Y, 0.6);
    let c = world.add(body(DVec3::ZERO, DVec3::Y, DT));
    run(&mut world, c, 60, |_| (DVec3::ZERO, false));
    run(&mut world, c, 90, |_| (DVec3::X * 1.4, false));
    let v = world.bodies[c].structure.velocity[0];
    assert!((v.x - 1.4).abs() < 0.02 && v.z.abs() < 1e-3, "walks at {v}");
    // And stops when asked for nothing.
    run(&mut world, c, 90, |_| (DVec3::ZERO, false));
    assert!(world.bodies[c].structure.velocity[0].length() < 0.02);
}

#[test]
fn it_stands_on_slopes_up_to_the_friction_angle_and_slides_beyond() {
    // The ground's friction 0.6 (boots 0.9): the friction angle is 31°.
    for (degrees, holds) in [(25.0f64, true), (36.0, false)] {
        let a = degrees.to_radians();
        let (s, c) = creeer_math::det::sin_cos(a);
        let normal = DVec3::new(-s, c, 0.0);
        let mut world = ground(normal, 0.6);
        // Feet on the slope at x = 0.
        let ch = world.add(body(DVec3::new(0.0, 0.02, 0.0), DVec3::Y, DT));
        run(&mut world, ch, 30, |_| (DVec3::ZERO, false));
        let start = feet(&world, ch);
        run(&mut world, ch, 120, |_| (DVec3::ZERO, false));
        let slid = (feet(&world, ch) - start).length();
        if holds {
            assert!(slid < 0.01, "slid {slid} m on {degrees}°");
        } else {
            assert!(slid > 0.3, "held on {degrees}° (slid {slid} m)");
        }
    }
}

#[test]
fn walking_on_a_crate_on_ice_pushes_the_crate_back() {
    // Frictionless ground: the crate and the character exchange momentum alone.
    let mut world = ground(DVec3::Y, 0.0);
    let half = DVec3::new(2.0, 0.25, 2.0);
    let mut crate_ = Body::rigid(Shape::Box { half }, 400.0, DVec3::new(0.0, 0.25, 0.0), DT);
    crate_.friction = 0.8;
    let raft = world.add(crate_);
    let c = world.add(body(DVec3::new(0.0, 0.5, 0.0), DVec3::Y, DT));
    run(&mut world, c, 90, |_| (DVec3::ZERO, false));
    let momentum = |w: &World| {
        let p = w.momentum();
        DVec3::new(p.x, 0.0, p.z)
    };
    let before = momentum(&world);
    run(&mut world, c, 60, |_| (DVec3::X * 1.4, false));
    let (vc, vr) = (
        world.bodies[c].structure.velocity[0],
        world.bodies[raft].structure.velocity[0],
    );
    assert!(vc.x > 1.0, "the character walks ({vc})");
    assert!(vr.x < -0.2, "the crate moves back ({vr})");
    let after = momentum(&world);
    assert!(
        (after - before).length() < 1e-3 * MASS * vc.x,
        "momentum {before} → {after}"
    );
}

#[test]
fn it_jumps_as_high_as_its_legs_push() {
    let mut world = ground(DVec3::Y, 0.6);
    let c = world.add(body(DVec3::ZERO, DVec3::Y, DT));
    run(&mut world, c, 90, |_| (DVec3::ZERO, false));
    let base = feet(&world, c).y;
    let mut highest = base;
    let mut reactions = Vec::new();
    for k in 0..90 {
        steer(
            &mut world,
            c,
            Intent::walk(DVec3::ZERO, k == 0),
            (DVec3::Y, None),
            &mut reactions,
        );
        world.step();
        release(&mut world, &mut reactions);
        highest = highest.max(feet(&world, c).y);
    }
    // v²/2g above where the legs leave the ground (their spring then unloaded, 3.4 cm up).
    let rise = highest - base;
    let want = JUMP_SPEED * JUMP_SPEED / (2.0 * G);
    assert!((rise - want).abs() < 0.08, "rose {rise} m, v²/2g {want}");
}

#[test]
fn a_step_down_lands_on_the_legs_and_a_fall_on_the_body() {
    // Peak deceleration of the body (g) dropped from `height` (feet above the ground).
    let landing = |height: f64| {
        let mut world = ground(DVec3::Y, 0.6);
        let c = world.add(body(DVec3::new(0.0, height, 0.0), DVec3::Y, DT));
        let (mut last, mut peak) = (0.0, 0.0f64);
        let mut reactions = Vec::new();
        for _ in 0..240 {
            steer(
                &mut world,
                c,
                Intent::default(),
                (DVec3::Y, None),
                &mut reactions,
            );
            world.step();
            release(&mut world, &mut reactions);
            let v = world.bodies[c].structure.velocity[0].y;
            peak = peak.max((v - last) / DT / G);
            last = v;
        }
        (peak, world.contacts.is_empty())
    };
    // Half a metre: the legs take it, the body never touches the ground.
    let (soft, on_legs) = landing(0.5);
    assert!(on_legs && soft < 4.0, "a step down: {soft} g");
    // Four metres: the legs give way, the body strikes.
    let (hard, _) = landing(4.0);
    assert!(hard > 2.0 * soft, "a fall: {hard} g against {soft}");
}

#[test]
fn an_idle_character_falls_asleep_and_stays_so() {
    let mut world = ground(DVec3::Y, 0.6);
    let c = world.add(body(DVec3::ZERO, DVec3::Y, DT));
    run(&mut world, c, 240, |_| (DVec3::ZERO, false));
    assert!(world.bodies[c].sleep.asleep, "never fell asleep");
    run(&mut world, c, 120, |_| (DVec3::ZERO, false));
    assert!(world.bodies[c].sleep.asleep, "woke without being asked");
    // Asked to walk, it wakes and goes.
    run(&mut world, c, 30, |_| (DVec3::X, false));
    assert!(world.bodies[c].structure.velocity[0].x > 0.3);
}

#[test]
fn it_stops_from_a_walk_or_a_run_within_a_second() {
    // Letting go of the keys, it brakes at its legs' most (about 4 m/s²) until all but stopped:
    // people stop in about a step (Ridge et al. 2013). Easing to a stop with the walk's own
    // response, it crept on for 1.4 s from a walk (the user: "walks way too long after stopping").
    for speed in [3.0, MOST_SPEED] {
        let mut world = ground(DVec3::Y, 0.6);
        let c = world.add(body(DVec3::ZERO, DVec3::Y, DT));
        run(&mut world, c, 120, |_| (DVec3::Z * speed, false));
        let from = world.bodies[c].structure.position[0];
        let mut stopped = None;
        let mut reactions = Vec::new();
        // Let go of the keys: stopping (as the client marks the second after the last walk).
        let stopping = Intent {
            stopping: true,
            ..Intent::default()
        };
        for k in 0..120 {
            steer(&mut world, c, stopping, (DVec3::Y, None), &mut reactions);
            world.step();
            release(&mut world, &mut reactions);
            let v = world.bodies[c].structure.velocity[0];
            if stopped.is_none() && v.reject_from(DVec3::Y).length() < 0.03 {
                stopped = Some(k);
            }
        }
        #[allow(clippy::cast_precision_loss)]
        let seconds = stopped.expect("it stopped") as f64 * DT;
        let went = (world.bodies[c].structure.position[0] - from)
            .reject_from(DVec3::Y)
            .length();
        // At about 4 m/s² from v: v/a seconds and v²/2a metres, and a little settling.
        assert!(
            seconds < speed / 4.0 + 0.15,
            "from {speed} m/s: {seconds} s"
        );
        assert!(
            went < speed * speed / 8.0 + 0.1,
            "from {speed} m/s: {went} m"
        );
        assert!(
            world.bodies[c].structure.velocity[0].length() < 0.01,
            "at rest"
        );
    }
}

/// Legs found pressed deep into their length (ground suddenly just under the hip: a step up,
/// or a crate's top under a person tipping over it) push no faster than legs extend (G2.10,
/// ADR-0023): pressed 0.45 m, the body leaves the ground no faster than about 2.6 m/s (2.14
/// measured), a squat jump's from that depth, its push falling from 3,600 N to nothing at
/// 3.5 m/s. (Pushing its whole 3,600 N as it straightened, it left at 3.01 m/s.)
#[test]
fn legs_pressed_deep_push_no_faster_than_legs_extend() {
    let mut world = ground(DVec3::Y, 0.6);
    let c = world.add(body(DVec3::new(0.0, -0.45, 0.0), DVec3::Y, DT));
    let mut fastest = 0.0f64;
    let mut reactions = Vec::new();
    for _ in 0..60 {
        steer(
            &mut world,
            c,
            Intent::default(),
            (DVec3::Y, None),
            &mut reactions,
        );
        world.step();
        release(&mut world, &mut reactions);
        fastest = fastest.max(world.bodies[c].structure.velocity[0].y);
    }
    eprintln!("left the ground at {fastest:.2} m/s");
    assert!(fastest < 2.6, "left the ground at {fastest:.2} m/s");
}
