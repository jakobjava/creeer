//! The bridge site's builder played (`bridge_bench.rs`), her client stepped in lockstep with the
//! server as a game's frames step it (a tick each): what her prediction costs a tick, in Newton
//! iterations (gated in CI) and in time (printed: `cargo test --release -p creeer_server --lib
//! measure_the_bridge_site -- --ignored --nocapture`).

use super::{Builder, Doing, site};
use crate::server::tests::Game;
use creeer_math::glam::DVec3;
use creeer_net::link::Conditions;
use std::time::{Duration, Instant};

const STEP: Duration = Duration::from_nanos(16_666_667);
/// The ticks played: to the post put down and a second after.
const TICKS: u32 = 720;

/// 100–130 ms each way, one packet in twenty lost (ADR-0025's slow link).
fn slow() -> Conditions {
    Conditions {
        latency: Duration::from_millis(100),
        jitter: Duration::from_millis(30),
        loss: 0.05,
    }
}

/// One tick's cost to the client: its prediction's Newton iterations, the inputs it replayed,
/// and the time (ms) of its session (snapshots taken in, its input predicted and sent), of its
/// scripted acts (the crosshair's aims, the hands' placing), and of every pose drawn.
#[derive(Clone, Copy, Debug, Default)]
struct Cost {
    work: u64,
    replayed: u32,
    session: f64,
    acts: f64,
    poses: f64,
}

/// What the builder got done (the post taken, held beside the bridge), each tick's cost to her
/// client, and what she was doing.
struct Played {
    costs: Vec<Cost>,
    done: (bool, bool),
    doing: Vec<Doing>,
}

/// The site played for `ticks` over `link` (none: lossless).
fn play(link: Option<Conditions>, ticks: u32) -> Played {
    let (world, site) = site(crate::play::scenes::terrain());
    let mut game = Game::new(world);
    game.join("ada", link);
    for _ in 0..600 {
        if game.clients[0].0.avatar().is_some() {
            break;
        }
        game.tick(crate::server::tests::still);
    }
    game.run(60, crate::server::tests::still);
    let mut builder = Builder::new(site);
    let (mut costs, mut doing) = (Vec::new(), Vec::new());
    for _ in 0..ticks {
        let (client, relay) = &mut game.clients[0];
        if let Some(r) = relay.as_mut() {
            r.pump(game.now);
        }
        let was = client.corrections();
        let started = Instant::now();
        client.update(STEP);
        let session = started.elapsed();
        let started = Instant::now();
        let (walk, _view) = builder.act(client);
        let acts = started.elapsed();
        let started = Instant::now();
        client.input(walk, false, DVec3::ZERO);
        client.flush();
        let session = session + started.elapsed();
        let started = Instant::now();
        let _ = client.poses();
        let poses = started.elapsed();
        let now = client.corrections();
        costs.push(Cost {
            work: now.work - was.work,
            replayed: now.replayed - was.replayed,
            session: session.as_secs_f64() * 1e3,
            acts: acts.as_secs_f64() * 1e3,
            poses: poses.as_secs_f64() * 1e3,
        });
        if let Some(r) = relay.as_mut() {
            r.pump(game.now);
        }
        game.server.tick();
        if let Some(r) = game.clients[0].1.as_mut() {
            r.pump(game.now);
        }
        game.now += STEP;
        doing.push(builder.doing);
    }
    Played {
        costs,
        done: builder.done(),
        doing,
    }
}

/// The value at percentile `q` (0–100) of `v`.
fn percentile(mut v: Vec<f64>, q: f64) -> f64 {
    v.sort_by(f64::total_cmp);
    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        clippy::cast_precision_loss
    )]
    let i = ((q / 100.0) * (v.len().max(1) - 1) as f64).round() as usize;
    v.get(i).copied().unwrap_or(0.0)
}

/// The builder's prediction, its most Newton iterations in a tick and at the 99th percentile,
/// lossless and over the slow link (ADR-0043): a tenth above what was measured (2026-11-14:
/// lossless 47 and 47, slow 48 and 44; walking among the timber at the bridge site's start costs
/// the most, the loose parts at her feet).
const GATES: [(f64, f64); 2] = [(52.0, 52.0), (53.0, 49.0)];

/// The builder takes the post and holds it beside the bridge, and her client keeps within its
/// work, lossless and over the slow link.
#[test]
#[allow(clippy::cast_precision_loss)]
fn a_builder_at_the_bridge_keeps_her_client_within_its_work() {
    for (link, (most, p99)) in [None, Some(slow())].into_iter().zip(GATES) {
        let p = play(link, TICKS);
        assert_eq!(p.done, (true, true), "{link:?}: {:?}", p.doing.last());
        let work: Vec<f64> = p.costs.iter().map(|c| c.work as f64).collect();
        let (w99, worst) = (percentile(work.clone(), 99.0), percentile(work, 100.0));
        assert!(
            worst <= most && w99 <= p99,
            "{link:?}: worst {worst}, p99 {w99} Newton iterations"
        );
    }
}

#[test]
#[ignore = "prints what the bridge site costs a client, tick by tick"]
#[allow(clippy::cast_precision_loss)]
fn measure_the_bridge_site() {
    for (name, link) in [("lossless", None), ("slow", Some(slow()))] {
        let p = play(link, TICKS);
        println!(
            "{name}: took the post, held it beside the bridge: {:?}",
            p.done
        );
        let line = |what: &str, v: Vec<f64>| {
            println!(
                "  {what:22} p50 {:8.2} p99 {:8.2} worst {:8.2}",
                percentile(v.clone(), 50.0),
                percentile(v.clone(), 99.0),
                percentile(v, 100.0)
            );
        };
        line(
            "work (iterations)",
            p.costs.iter().map(|c| c.work as f64).collect(),
        );
        line(
            "inputs replayed",
            p.costs.iter().map(|c| f64::from(c.replayed)).collect(),
        );
        line("session (ms)", p.costs.iter().map(|c| c.session).collect());
        line("acts (ms)", p.costs.iter().map(|c| c.acts).collect());
        line("poses (ms)", p.costs.iter().map(|c| c.poses).collect());
        // Where the work went: each stage's ticks, its most and its sum.
        let mut stages: Vec<(Doing, u64, u64, usize)> = Vec::new();
        for (c, d) in p.costs.iter().zip(&p.doing) {
            let key = std::mem::discriminant(d);
            match stages.last_mut() {
                Some(s) if std::mem::discriminant(&s.0) == key => {
                    (s.1, s.2, s.3) = (s.1.max(c.work), s.2 + c.work, s.3 + 1);
                }
                _ => stages.push((*d, c.work, c.work, 1)),
            }
        }
        for (d, most, sum, n) in stages {
            println!("  {d:?}: {n} ticks, most {most}, all {sum}");
        }
    }
}
