use super::bubble::Frame;
use super::tests_support::{DT, universe};
use super::*;
use creeer_math::glam::{DQuat, DVec3};
use creeer_orbit::rails::{Group, Particle};
use creeer_physics::world::shape::Shape;

fn ball(position: DVec3, velocity: DVec3) -> Body {
    ball_dt(position, velocity, DT)
}

fn ball_dt(position: DVec3, velocity: DVec3, dt: f64) -> Body {
    let mut body = Body::rigid(Shape::Sphere { radius: 1.0 }, 1000.0, position, dt);
    body.structure.velocity[0] = velocity;
    body
}

/// A ball in a circular orbit `altitude` above Home, given in Home's surface frame.
fn orbiting(u: &mut Universe, altitude: f64) -> u64 {
    let home = u.bodies.body(BodyRef::Major(u.home)).clone();
    let r = home.radius + altitude;
    let pose = u.bubble_pose();
    // Circular in inertial space, written in the turning frame: v_rot = v − Ω×x.
    let x = DVec3::X * r;
    let speed = (home.mu / r).sqrt();
    let axis_local = pose.rotation.conjugate() * home.spin.axis();
    let v_inertial_local = axis_local.cross(x).normalize() * speed;
    let spin_local = pose.rotation.conjugate() * pose.spin;
    let id = u.spawn("ball", ball(x, v_inertial_local - spin_local.cross(x)));
    u.focus = Focus::Vessel(id);
    id
}

fn inertial(u: &Universe, id: u64) -> (DVec3, DVec3) {
    let i = u.bubble_index(id).expect("in the bubble");
    let (x, v, _) = vessels::centre_of_mass(&u.bubble.world.bodies[i]);
    u.bubble_pose().to_inertial(x, v)
}

#[test]
fn a_ball_leaving_the_surface_zone_moves_into_an_orbit_frame_without_a_jump() {
    let mut u = universe();
    let id = orbiting(&mut u, 200_000.0);
    let before = inertial(&u, id);
    u.step();
    assert!(
        matches!(u.bubble.frame, Frame::Orbit(_)),
        "still on the surface frame"
    );
    let after = inertial(&u, id);
    // One tick of flight: x moves by v·dt; the frame change itself adds nothing.
    let predicted = before.0 + before.1 * DT;
    assert!(
        (after.0 - predicted).length() < 0.05,
        "{}",
        (after.0 - predicted).length()
    );
    assert!((after.1 - before.1).length() < 0.2);
    // The ball sits at the new floating origin.
    assert!(u.focus_local().0.length() < 1e-3);
}

/// The same orbit flown with full physics in the bubble and propagated on rails agree.
#[test]
fn the_bubble_and_the_rails_fly_the_same_orbit() {
    let mut u = universe();
    let id = orbiting(&mut u, 200_000.0);
    u.step(); // into the orbit frame
    // The free-falling origin follows the focused ball on rails: a second ball 800 m away,
    // 1.5 m/s faster, moves through the tidal field relative to it (and is passed by the
    // origin's rebasing as the first ball drifts).
    let (x, v) = u.focus_local();
    let other = u.spawn(
        "other",
        ball(
            x + DVec3::new(800.0, 0.0, 0.0),
            v + DVec3::new(0.0, 1.5, -0.5),
        ),
    );
    let t0 = u.time();
    let reference = |u: &Universe, id| {
        Group::new(
            t0,
            &[inertial(u, id)],
            vec![Particle {
                mass: 1000.0,
                mu: 0.0,
            }],
        )
    };
    let mut rails = [reference(&u, id), reference(&u, other)];
    u.set_warp(4.0).expect("physics warp");
    // The universe forgets the past: the references keep up tick by tick.
    for _ in 0..1800 {
        u.step();
        for r in &mut rails {
            r.propagate(&u.bodies.ephemeris, u.time(), None);
        }
    }
    let t1 = u.time();
    assert!((t1 - t0 - 120.0).abs() < 1e-6);
    for (k, vessel) in [id, other].into_iter().enumerate() {
        let error = (inertial(&u, vessel).0 - rails[k].state(0).0).length();
        let apart = (inertial(&u, vessel).0 - inertial(&u, id).0).length();
        println!(
            "ball {k}: bubble vs rails after 2 minutes {error:.2e} m ({apart:.0} m from the focus)"
        );
        // Rounding of inertial coordinates near 1e10 m (2 µm) is the floor. Before the field
        // was set at once after frame changes and taken at the end of each step, ball 1 was
        // 7 mm off, halving with the step.
        assert!(error < 1e-5, "ball {k}: {error} m");
    }
}

#[test]
fn time_warp_packs_and_unpacks_the_vessel() {
    let mut u = universe();
    let id = orbiting(&mut u, 300_000.0);
    u.step();
    let start = inertial(&u, id);
    let t0 = u.time();
    let mut rails = Group::new(
        t0,
        &[start],
        vec![Particle {
            mass: 1000.0,
            mu: 0.0,
        }],
    );
    u.set_warp(1000.0).expect("in orbit: can warp");
    assert!(u.bubble.vessels.is_empty(), "packed");
    assert!(matches!(
        u.vessel(id).map(|v| &v.place),
        Some(Where::Rails { .. })
    ));
    for _ in 0..60 {
        u.step(); // 1000 s
        rails.propagate(&u.bodies.ephemeris, u.time(), None);
    }
    u.set_warp(1.0).expect("back to normal");
    u.step();
    rails.propagate(&u.bodies.ephemeris, u.time(), None);
    assert!(u.bubble_index(id).is_some(), "back in the bubble");
    let error = (inertial(&u, id).0 - rails.state(0).0).length();
    assert!(error < 0.05, "{error} m off after the warp");
}

/// A crate on the ground with its engine pushing it down pushes the planet away: the planet
/// gains the engine's impulse (Newton's third law through the ground).
#[test]
fn an_engine_pushing_on_the_ground_moves_the_planet() {
    let mut u = universe();
    let terrain = u
        .terrains
        .lock()
        .expect("terrains")
        .loaded(BodyRef::Major(u.home))
        .cloned()
        .expect("home terrain");
    let up = DVec3::new(0.3, 0.8, 0.2).normalize();
    let ground = up * (terrain.planet().datum.radius(up) + terrain.surface_height(up));
    let mut crate_ = Body::rigid(
        Shape::Box {
            half: DVec3::splat(0.5),
        },
        1e4,
        ground + up * 0.6,
        DT,
    );
    crate_.structure.orientation[0] = DQuat::from_rotation_arc(DVec3::Y, up);
    crate_.friction = 0.7;
    let id = u.spawn("crate", crate_);
    u.focus = Focus::Vessel(id);
    for _ in 0..120 {
        u.step(); // settle
    }
    // Everything the planet got or will get: applied pushes plus the collected impulse (5 s
    // of 1 MN change its velocity by 1e-16 m/s, far below the threshold for applying it).
    let total = |u: &Universe| {
        u.bodies
            .ephemeris
            .pushes()
            .iter()
            .map(|p| p.impulse())
            .sum::<DVec3>()
            + u.bubble.pending_reaction()
    };
    let impulse_before = total(&u);
    let thrust = 1e6;
    let i = u.bubble_index(id).expect("in the bubble");
    let local_down = -u.bubble.world.bodies[i].structure.position[0].normalize();
    u.bubble.world.bodies[i].structure.load[0] = local_down * thrust;
    u.bubble.world.bodies[i].wake();
    let seconds = 5.0;
    for _ in 0..300 {
        u.step();
    }
    let gained = total(&u) - impulse_before;
    let expected = u.bubble_pose().rotation * local_down * (thrust * seconds);
    println!("planet gained {gained} N·s, engine gave {expected}");
    assert!(
        (gained - expected).length() < 2e-3 * expected.length(),
        "{gained} vs {expected}"
    );
}

/// A point `height` above Home's ground, and the local up direction.
fn above_ground(u: &Universe, height: f64) -> (DVec3, DVec3) {
    let terrain = u
        .terrains
        .lock()
        .expect("terrains")
        .loaded(BodyRef::Major(u.home))
        .cloned()
        .expect("home terrain");
    let up = DVec3::new(-0.2, 0.7, 0.6).normalize();
    let ground = up * (terrain.planet().datum.radius(up) + terrain.surface_height(up));
    (ground + up * height, up)
}

/// A ship hovering on its engine, touching nothing, still pulls its planet by gravity: the
/// planet gains m·μ/r²·t towards it (a gravity tractor).
#[test]
fn a_hovering_ship_pulls_its_planet() {
    let mut u = universe();
    let (at, up) = above_ground(&u, 200.0);
    let mass = 5e4;
    let mut ship = Body::rigid(Shape::Sphere { radius: 2.0 }, mass, at, DT);
    let g = u.bubble.field(&u.bodies, 0.0).acceleration(at);
    ship.structure.load[0] = -g * mass; // hover
    let id = u.spawn("ship", ship);
    u.focus = Focus::Vessel(id);
    let before = u.bubble.pending_reaction();
    let seconds = 10.0;
    for _ in 0..600 {
        u.step();
    }
    let i = u.bubble_index(id).expect("still here");
    let x = u.bubble.world.bodies[i].structure.position[0];
    assert!(
        (x - at).length() < 0.5,
        "hovered {} m off",
        (x - at).length()
    );
    let pulled = u.bubble.pending_reaction() - before;
    let home = u.bodies.body(BodyRef::Major(u.home));
    let r = at.length();
    let expected = u.bubble_pose().rotation * (at / r) * (mass * home.mu / (r * r) * seconds);
    println!("pulled {pulled} N·s, expected {expected}");
    assert!((pulled - expected).length() < 2e-3 * expected.length());
    let _ = up;
}

#[test]
fn a_crate_on_the_ground_stays_put_through_time_warp() {
    let mut u = universe();
    let (at, up) = above_ground(&u, 0.6);
    let mut crate_ = Body::rigid(
        Shape::Box {
            half: DVec3::splat(0.5),
        },
        200.0,
        at,
        DT,
    );
    crate_.structure.orientation[0] = DQuat::from_rotation_arc(DVec3::Y, up);
    crate_.friction = 0.7;
    let id = u.spawn("crate", crate_);
    u.focus = Focus::Vessel(id);
    for _ in 0..600 {
        u.step();
        if u.bubble.world.bodies[0].sleep.asleep {
            break;
        }
    }
    let rest = u.bubble.world.bodies[0].structure.position[0];
    u.set_warp(10_000.0).expect("the crate rests");
    assert!(matches!(
        u.vessel(id).map(|v| &v.place),
        Some(Where::Landed { .. })
    ));
    for _ in 0..60 {
        u.step(); // 10,000 s: Home turns almost half way
    }
    u.set_warp(1.0).expect("back");
    u.focus = Focus::Point(rest);
    u.step();
    let i = u.bubble_index(id).expect("back in the bubble");
    let x = u.bubble.world.bodies[i].structure.position[0];
    assert!(
        (x - rest).length() < 0.01,
        "moved {} m",
        (x - rest).length()
    );
}

#[test]
fn every_generated_surface_has_terrain() {
    let u = universe();
    let surfaces: Vec<BodyRef> = u
        .bodies
        .all()
        .filter(|(_, b)| b.kind.has_surface())
        .map(|(r, _)| r)
        .take(40)
        .collect();
    for r in surfaces {
        let body = u.bodies.body(r).clone();
        let terrain = u
            .terrains
            .lock()
            .expect("terrains")
            .get(r, &body)
            .expect("loads");
        // The surface lies near the body's radius, with relief a fraction of it.
        let up = DVec3::new(0.3, 0.5, -0.8).normalize();
        let height = terrain.surface_height(up);
        let radius = terrain.planet().datum.radius(up);
        assert!(
            (radius / body.radius - 1.0).abs() < 0.3,
            "{}: datum {radius}",
            body.name
        );
        assert!(
            height.abs() < 0.4 * body.radius,
            "{}: height {height}",
            body.name
        );
    }
}

/// Cross-platform determinism of the universe: a ship in the bubble through a frame change,
/// physics warp, rails warp and back, with the small bodies on rails. Generated on Windows
/// x86-64; never regenerate it to make a failure go away.
#[test]
fn the_universe_runs_identically_on_every_platform() {
    let mut u = universe();
    let id = orbiting(&mut u, 250_000.0);
    for _ in 0..60 {
        u.step();
    }
    u.set_warp(4.0).expect("physics warp");
    for _ in 0..60 {
        u.step();
    }
    u.set_warp(1e4).expect("rails warp");
    for _ in 0..30 {
        u.step();
    }
    u.set_warp(1.0).expect("back");
    u.step();
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    let mut eat = |value: f64| hash = (hash ^ value.to_bits()).wrapping_mul(0x0100_0000_01b3);
    let (x, v) = inertial(&u, id);
    for value in [x.x, x.y, x.z, v.x, v.y, v.z, u.time()] {
        eat(value);
    }
    for minor in u.bodies.minors.iter().step_by(10) {
        let (x, _) = minor.group.state(0);
        eat(x.x);
        eat(x.z);
    }
    if std::env::var_os("CREEER_PRINT_GOLDEN").is_some() {
        println!("universe golden hash: {hash:#018x}");
    }
    assert_eq!(hash, GOLDEN_UNIVERSE_HASH, "got {hash:#018x}");
}

const GOLDEN_UNIVERSE_HASH: u64 = 0x78fe_74ac_ae5b_b155;
