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
use creeer_math::glam::{DQuat, DVec3};
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
    // `FIRST`: the place round the spawn to start at (0: 4 m east, where the third player
    // stands).
    let first: u32 = std::env::var("FIRST")
        .ok()
        .and_then(|c| c.parse().ok())
        .unwrap_or(0);
    for k in first..first + count {
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
            use creeer_physics::profile::Slot as P;
            let c = |slot: P| t.phases[slot as usize];
            println!(
                "  {tick}: v {:.3} m/s, spin {:.2} rad/s ({:.2} about its axis), {:.2} ms; \
                 {} steps, {} redos, {} groups, {} its, {} trials, {} searches",
                s.velocity[0].length(),
                w.length(),
                w.dot(axis).abs(),
                t.ms,
                c(P::WorldSteps),
                c(P::TurnRedos),
                c(P::GroupSteps),
                c(P::Iterations),
                c(P::Trials),
                c(P::FaceSearches),
            );
        }
        if tick % 20 == 0 {
            println!(
                "tick {tick}: {:.2} ms, {} iterations, {awake} awake",
                t.ms, t.work
            );
            // `AWAKE`: every awake body other than the players', its speed and spin.
            if std::env::var("AWAKE").is_ok() {
                let w = &play.server.world;
                for (id, b) in w.ids().iter().zip(&w.world.bodies) {
                    let s = &b.structure;
                    if !b.sleep.asleep && s.masses()[0] < 50.0 {
                        println!(
                            "    {id:?}: {:.4} m/s, {:.4} rad/s",
                            s.velocity[0].length(),
                            s.angular_velocity[0].length()
                        );
                    }
                }
            }
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

/// *builders*' heavy ticks: on each over `HEAVY` Newton iterations (800), the last step's
/// groups of more than one body, by entity and mass. `TICKS` (600).
#[test]
#[ignore = "a measurement, run by hand"]
fn builders_heavy_ticks() {
    let name = std::env::var("SCENE").unwrap_or_else(|_| "builders".into());
    let s = scene(&name).expect("the scene");
    let get = |name: &str, or: u64| {
        std::env::var(name)
            .ok()
            .and_then(|c| c.parse().ok())
            .unwrap_or(or)
    };
    let (ticks, heavy) = (get("TICKS", 600), get("HEAVY", 800));
    let mut play = Play::new((s.build)(terrain()), s.name, s.players, &terrain());
    // The fastest a pole spun, and the ticks one spun faster than 20 rad/s.
    let (mut fastest, mut fast) = ((0.0f64, 0), 0);
    for tick in 0..u32::try_from(ticks).expect("ticks") {
        let acts = (s.script)(tick, &play);
        // `FALLS`: each player's lean and speed the tick before it falls.
        let before: Vec<_> = (0..play.players())
            .map(|k| {
                let w = &play.server.world;
                let id = play.client(k).welcome.as_ref().map(|w| w.avatar);
                let fallen = id.and_then(|id| w.person(id)).is_some_and(|p| p.fallen());
                let pose = id.and_then(|id| w.index(id)).map(|i| {
                    let s = &w.world.bodies[i].structure;
                    let up = s.position[0].normalize();
                    let lean = det::acos((s.orientation[0] * DVec3::Y).dot(up).clamp(-1.0, 1.0));
                    (
                        lean.to_degrees(),
                        s.velocity[0].length(),
                        w.world.touching(i, 0.005).len(),
                    )
                });
                (fallen, pose)
            })
            .collect();
        let t = play.tick(&acts);
        // `WATCH=k`: player k's speed and lean each tick from `FROM`.
        if let (Ok(k), Ok(from)) = (std::env::var("WATCH"), std::env::var("FROM")) {
            let (k, from): (usize, u32) = (k.parse().expect("k"), from.parse().expect("from"));
            if tick >= from {
                let w = &play.server.world;
                let id = play.client(k).welcome.as_ref().map(|w| w.avatar);
                let what = id.and_then(|id| w.index(id)).map(|i| {
                    let x = w.world.bodies[i].structure.position[0];
                    let height = (x - w.ground_under(x)).dot(x.normalize());
                    let touching: Vec<String> = (w.world.touching(i, 0.005).iter())
                        .map(|t| {
                            t.other.map_or("the ground".into(), |(j, _)| {
                                let m: f64 = w.world.bodies[j].structure.masses().iter().sum();
                                format!("{:?} {m:.0} kg", w.ids()[j])
                            })
                        })
                        .collect();
                    let s = &w.world.bodies[i].structure;
                    let radial = s.velocity[0].dot(x.normalize());
                    let kinetic = s.node_energy().0;
                    (
                        height,
                        x.length() - 1.0e6 * (x.length() / 1.0e6).floor(),
                        radial,
                        kinetic,
                        touching,
                    )
                });
                println!("  {tick}: {:?} {what:.2?}", before[k].1);
            }
        }
        for (k, (was, pose)) in before.iter().enumerate() {
            let w = &play.server.world;
            let now = (play
                .client(k)
                .welcome
                .as_ref()
                .map(|w| w.avatar)
                .and_then(|id| w.person(id)))
            .is_some_and(|p| p.fallen());
            if now && !was && std::env::var("FALLS").is_ok() {
                println!("player {k} fell at tick {tick}: before it leant, sped, touched {pose:?}");
            }
        }
        let spin = (play.server.world.world.bodies.iter())
            .filter(|b| b.structure.masses().iter().sum::<f64>() < 10.0)
            .map(|b| b.structure.angular_velocity[0].length())
            .fold(0.0, f64::max);
        fast += usize::from(spin > 20.0);
        if spin > 20.0 && std::env::var("SPINS").is_ok() {
            println!("spin {spin:.1} rad/s at tick {tick}: {} iterations", t.work);
        }
        if spin > fastest.0 {
            fastest = (spin, tick);
        }
        if t.work < heavy {
            continue;
        }
        let w = &play.server.world;
        println!("tick {tick}: {:.2} ms, {} iterations", t.ms, t.work);
        for group in w.world.groups().filter(|g| g.len() > 1) {
            let parts: Vec<String> = (group.iter())
                .map(|&i| {
                    let s = &w.world.bodies[i].structure;
                    format!(
                        "{:?} {:.0} kg {:.3} rad/s",
                        w.ids()[i],
                        s.masses().iter().sum::<f64>(),
                        s.angular_velocity[0].length()
                    )
                })
                .collect();
            println!("    {}", parts.join(", "));
        }
    }
    println!(
        "fastest pole {:.1} rad/s at tick {}; {fast} ticks with one over 20 rad/s",
        fastest.0, fastest.1
    );
}

/// *builders* itself: its server's and its clients' ticks while the players walk to their
/// poles (the third spawns on the first pair), and the first 100 ticks' worst. `TICKS` (300).
#[test]
#[ignore = "a measurement, run by hand"]
fn builders_first_ticks() {
    let name = std::env::var("SCENE").unwrap_or_else(|_| "builders".into());
    let s = scene(&name).expect("the scene");
    let ticks: u32 = std::env::var("TICKS")
        .ok()
        .and_then(|c| c.parse().ok())
        .unwrap_or(300);
    let report = super::scenes::run_with(s, ticks, |tick, t| {
        if tick % 10 == 0 || t.ms > 5.0 {
            use creeer_physics::profile::Slot as P;
            let c = |slot: P| t.phases[slot as usize];
            println!(
                "tick {tick}: server {:.2} ms, {} iterations; a client {:.2} ms, {} iterations; \
                 {} steps, {} redos, {} groups, {} its, {} searches",
                t.ms,
                t.work,
                t.client_ms,
                t.client_work,
                c(P::WorldSteps),
                c(P::TurnRedos),
                c(P::GroupSteps),
                c(P::Iterations),
                c(P::FaceSearches),
            );
        }
    });
    let first = &report.ticks[..100.min(report.ticks.len())];
    let worst = |f: fn(&super::Tick) -> f64| first.iter().map(f).fold(0.0, f64::max);
    #[allow(clippy::cast_precision_loss)]
    let mean = first.iter().map(|t| t.ms).sum::<f64>() / first.len() as f64;
    let most = |f: fn(&super::Tick) -> u64| first.iter().map(f).max().unwrap_or(0);
    println!(
        "first 100: server worst {:.2} ms (mean {mean:.2}), {} iterations; a client worst \
         {:.2} ms, {} iterations",
        worst(|t| t.ms),
        most(|t| t.work),
        worst(|t| t.client_ms),
        most(|t| t.client_work),
    );
    let (tick, most) = report.most_work();
    let (client_tick, client_most) = report.most_client_work();
    println!(
        "all {ticks}: most {most} at tick {tick}, a client's most {client_most} at tick \
         {client_tick}; worst {:.2} ms, a client's {:.2} ms",
        report.percentile_ms(100.0),
        report.client_percentile_ms(100.0)
    );
}
