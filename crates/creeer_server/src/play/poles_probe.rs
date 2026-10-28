//! *builders*' loose poles settling (G2.10; run by hand): each tick's cost and work while its
//! sixteen poles, crossed in pairs, settle on the terrain, the players standing by. `POLES`:
//! `both` (the scene), `lower` (each pair's lower pole alone, on the ground) or `upper` (the
//! upper alone, dropped 9 cm).
//!
//! `POLES=lower cargo test -p creeer_server --release poles_settling -- --ignored --nocapture`

use super::Play;
use super::record::Act;
use super::scenes::{scene, terrain, world};
use creeer_math::det;
use creeer_math::glam::DQuat;
use creeer_sim::items::{Form, Item, Piece};
use creeer_terrain::geology::deposits::tangent_frame;

#[test]
#[ignore = "a measurement, run by hand"]
fn poles_settling() {
    let s = scene("builders").expect("the scene");
    let terrain = terrain();
    let which = std::env::var("POLES").unwrap_or_else(|_| "both".into());
    let mut w = world(terrain.clone());
    let spawn = w.spawn();
    let (east, north) = tangent_frame(spawn.normalize());
    let pole = || {
        let rod = Form::Rod {
            radius: 0.025,
            length: 1.6,
        };
        Item::Piece(Piece::solid("spruce_green", 800.0, rod), Vec::new())
    };
    let count: u32 = std::env::var("COUNT")
        .ok()
        .and_then(|c| c.parse().ok())
        .unwrap_or(8);
    for k in 0..count {
        let (s, c) = det::sin_cos(f64::from(k) * std::f64::consts::FRAC_PI_4);
        let ground = w.ground_under(spawn + (east * c + north * s) * 4.0);
        let up = ground.normalize();
        if which != "upper" {
            w.add_item(pole(), ground + up * 0.03, true);
        }
        if which != "lower" {
            let above = w.add_item(pole(), ground + up * 0.09, true);
            if let Some(i) = w.index(above) {
                let s = &mut w.world.bodies[i].structure;
                s.orientation[0] =
                    DQuat::from_axis_angle(up, std::f64::consts::FRAC_PI_2) * s.orientation[0];
            }
        }
    }
    let mut play = Play::new(w, s.name, s.players, &terrain);
    let idle = vec![Act::default(); s.players.len()];
    let (mut ms, mut work) = (0.0, 0);
    let _ = creeer_physics::profile::take();
    let mut sums = [0u64; creeer_physics::profile::SLOTS];
    for tick in 0..240 {
        let t = play.tick(&idle);
        (ms, work) = (ms + t.ms, work + t.work);
        for (s, v) in sums.iter_mut().zip(t.phases) {
            *s += v;
        }
        let awake = (play.server.world.world.bodies.iter())
            .filter(|b| !b.sleep.asleep)
            .count();
        if std::env::var("TRACE").is_ok() && tick < 120 {
            let b = &play.server.world.world.bodies[play
                .server
                .world
                .index(creeer_net::entity::EntityId(1))
                .expect("pole")];
            let (s, q) = (&b.structure, b.structure.orientation[0]);
            let axis = q * creeer_math::glam::DVec3::X;
            let w = s.angular_velocity[0];
            println!(
                "  {tick}: v {:.3} m/s, spin {:.2} rad/s ({:.2} about its axis), {:.2} ms",
                s.velocity[0].length(),
                w.length(),
                w.dot(axis).abs(),
                t.ms
            );
        }
        if tick % 20 == 0 {
            println!(
                "tick {tick}: {:.2} ms, {} iterations, {awake} awake",
                t.ms, t.work
            );
        }
    }
    println!("{which}: {ms:.0} ms, {work} iterations in all");
    for (slot, name) in creeer_physics::profile::NAMES {
        let v = sums[slot as usize];
        if creeer_physics::profile::is_count(slot) {
            println!("  {name}: {v}");
        } else {
            #[allow(clippy::cast_precision_loss)]
            let at = v as f64 * 1e-6;
            println!("  {name}: {at:.1} ms");
        }
    }
}

/// One pole laid on the terrain 4 m from the spawn, no one about: the world stepped alone,
/// where its steps' work goes (with `--features creeer_physics/profile`).
#[test]
#[ignore = "a measurement, run by hand"]
fn a_pole_settling() {
    let mut w = world(terrain());
    let spawn = w.spawn();
    let (east, _) = tangent_frame(spawn.normalize());
    let ground = w.ground_under(spawn + east * 4.0);
    let rod = Form::Rod {
        radius: 0.025,
        length: 1.6,
    };
    let id = w.add_item(
        Item::Piece(Piece::solid("spruce_green", 800.0, rod), Vec::new()),
        ground + ground.normalize() * 0.03,
        true,
    );
    let _ = creeer_physics::profile::take();
    let mut sums = [0u64; creeer_physics::profile::SLOTS];
    for tick in 0..240 {
        let t = std::time::Instant::now();
        w.step();
        let ms = t.elapsed().as_secs_f64() * 1e3;
        for (s, v) in sums.iter_mut().zip(creeer_physics::profile::take()) {
            *s += v;
        }
        if tick % 10 == 0 {
            let b = &w.world.bodies[w.index(id).expect("there")];
            let s = &b.structure;
            println!(
                "tick {tick}: {ms:.2} ms, v {:.4} m/s, spin {:.3} rad/s, asleep {}",
                s.velocity[0].length(),
                s.angular_velocity[0].length(),
                b.sleep.asleep
            );
        }
    }
    for (slot, name) in creeer_physics::profile::NAMES {
        let v = sums[slot as usize];
        if creeer_physics::profile::is_count(slot) {
            println!("  {name}: {v}");
        } else {
            #[allow(clippy::cast_precision_loss)]
            let at = v as f64 * 1e-6;
            println!("  {name}: {at:.1} ms");
        }
    }
}
