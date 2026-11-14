//! Where a client's prediction spends its frames in a play scene (G2.11 A5, run by hand):
//! `SCENE=builders TICKS=600 cargo test --release -p creeer_server --lib client_frames --
//! --ignored --nocapture`. Per client and tick: the Newton iterations of its input's step and of
//! its replays, whether a snapshot was rewound or agreed, the bodies it predicts and how many of
//! them are awake.

use crate::play::Play;
use crate::play::scenes::{scene, terrain};

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

/// One player alone on bare ground, lossless: standing, walking, turning, stopping. How often
/// its own prediction disagrees with the server's (nothing but the avatar to predict).
#[test]
#[ignore = "prints how often a lone avatar is mispredicted"]
fn a_lone_avatar() {
    use crate::server::tests::{Game, still};
    use creeer_math::glam::DVec3;
    let mut game = Game::new(crate::world::tests::world());
    game.join("ada", None);
    game.run(120, still);
    let up = game.avatar(0).normalize();
    let (east, north) = creeer_terrain::geology::deposits::tangent_frame(up);
    let report = |game: &Game, what: &str| {
        let c = game.clients[0].0.corrections();
        println!(
            "{what}: set back {}, moved in place {}, rewound {} (avatar {}, bodies {}), off in {:?}, off by {:?}; over 1 cm {}, largest {:.4} m",
            c.count,
            c.nudged,
            c.rewinds,
            c.for_avatar,
            c.for_bodies,
            c.off_in[0],
            c.avatar_off,
            c.over_a_centimetre,
            c.largest
        );
    };
    report(&game, "joined and stood 2 s");
    game.run(300, still);
    report(&game, "stood 5 s more");
    game.run(300, |_| (east * 1.4, false));
    report(&game, "walked east 5 s");
    game.run(300, |_| (north * 3.0, false));
    report(&game, "ran north 5 s");
    game.run(300, still);
    report(&game, "stopped 5 s");
    let tick = std::cell::Cell::new(0u32);
    game.run(600, |_| {
        tick.set(tick.get() + 1);
        (east * 1.4, tick.get() % 180 == 0)
    });
    report(&game, "walked 10 s, jumping every 3 s");
    let crate_at = game.server.world.ground_under(game.avatar(0) + north * 2.0);
    game.clients[0]
        .0
        .command(creeer_net::message::Command::Crate(crate_at + up * 1.0));
    game.run(300, still);
    report(&game, "a crate dropped 2 m off, 5 s");
    game.run(300, |_| (north * 1.4, false));
    report(&game, "walked into it, 5 s");
    let _ = DVec3::ZERO;
}

#[test]
#[ignore = "prints where a client's prediction spends its frames"]
#[allow(clippy::cast_precision_loss)]
fn client_frames() {
    let name = std::env::var("SCENE").unwrap_or_else(|_| "builders".into());
    let s = scene(&name).expect("a scene");
    let ticks: u32 = (std::env::var("TICKS").ok())
        .and_then(|t| t.parse().ok())
        .unwrap_or(s.ticks.1);
    let mut play = Play::new((s.build)(terrain()), s.name, s.players, &terrain());
    let n = play.players();
    let mut before: Vec<_> = (0..n).map(|k| play.client(k).corrections()).collect();
    // Per client and tick: work, of it replayed, rewinds, agreed, bodies, awake.
    let mut rows: Vec<(u32, usize, u64, u64, u32, u32, usize, usize)> = Vec::new();
    let mut ms = Vec::new();
    let mut grouped = [0u32; 6];
    // `TRACE=a-b`: client 0's (`CLIENT`'s) bodies, tick by tick, from a to b.
    let trace: Option<(u32, u32)> = std::env::var("TRACE").ok().and_then(|t| {
        let (a, b) = t.split_once('-')?;
        Some((a.parse().ok()?, b.parse().ok()?))
    });
    for t in 0..ticks {
        let acts = (s.script)(t, &play);
        let cost = play.tick(&acts);
        ms.push(cost.client_ms);
        if trace.is_some_and(|(a, b)| (a..=b).contains(&t)) {
            let k = (std::env::var("CLIENT").ok().and_then(|c| c.parse().ok())).unwrap_or(0);
            let c = play.client(k);
            let p = c.predicted.as_ref().expect("predicted");
            let kinds: Vec<String> = (p.nearby_motions().iter())
                .map(|(id, v, w, asleep)| {
                    let kind = match c.kinds.get(id) {
                        Some(creeer_net::entity::Kind::Body(shape, m)) => {
                            format!("{shape:?}", shape = std::mem::discriminant(shape))
                                + &format!(" {:.1} kg", m.mass)
                        }
                        Some(k) => format!("{:?}", std::mem::discriminant(k)),
                        None => "unknown".into(),
                    };
                    format!("{id:?} {kind} v {v:.4} w {w:.4} asleep {asleep}")
                })
                .collect();
            let server: Vec<String> = (kinds.iter().zip(p.nearby_motions()))
                .map(|(_, (id, ..))| {
                    let w = &play.server.world;
                    w.index(id).map_or("gone".into(), |i| {
                        let b = &w.world.bodies[i];
                        format!(
                            "{id:?} elements {} asleep {} v {:.4}",
                            b.structure.element_count(),
                            b.sleep.asleep,
                            b.structure.velocity[0].length()
                        )
                    })
                })
                .collect();
            println!(
                "tick {t}: work {} group {}; {kinds:?}; server {server:?}",
                c.corrections().work,
                p.avatar_group()
            );
        }
        for (k, was) in before.iter_mut().enumerate() {
            let c = play.client(k).corrections();
            let p = play.client(k).predicted.as_ref().expect("predicted");
            let rewinds = c.rewinds - was.rewinds;
            grouped[p.avatar_group().min(5)] += 1;
            rows.push((
                t,
                k,
                c.work - was.work,
                c.replay_work - was.replay_work,
                rewinds,
                (c.count - was.count) - rewinds,
                p.nearby_count(),
                p.awake(),
            ));
            *was = c;
        }
    }
    let work: Vec<f64> = rows.iter().map(|r| r.2 as f64).collect();
    let replay: Vec<f64> = rows.iter().map(|r| r.3 as f64).collect();
    let input: Vec<f64> = rows.iter().map(|r| (r.2 - r.3) as f64).collect();
    let line = |what: &str, v: &[f64]| {
        println!(
            "{what:24} p50 {:8.1} p99 {:8.1} worst {:8.1} mean {:8.2}",
            percentile(v.to_vec(), 50.0),
            percentile(v.to_vec(), 99.0),
            percentile(v.to_vec(), 100.0),
            v.iter().sum::<f64>() / v.len().max(1) as f64
        );
    };
    println!("{name}, {ticks} ticks, {n} clients (per client and tick):");
    line("work (iterations)", &work);
    line("  of it replaying", &replay);
    line("  of it the input's step", &input);
    line("a client's time (ms, most)", &ms);
    let (rewound, agreed): (u32, u32) = rows.iter().fold((0, 0), |a, r| (a.0 + r.4, a.1 + r.5));
    let bodies: f64 = rows.iter().map(|r| r.6 as f64).sum::<f64>() / rows.len() as f64;
    let awake: f64 = rows.iter().map(|r| r.7 as f64).sum::<f64>() / rows.len() as f64;
    println!(
        "snapshots rewound {rewound}, agreed {agreed}; bodies predicted {bodies:.2} on average, awake {awake:.2} (the avatar's included)"
    );
    let why = (0..n).fold((0, 0, 0), |a, k| {
        let c = play.client(k).corrections();
        (a.0 + c.for_avatar, a.1 + c.for_bodies, a.2 + c.for_choice)
    });
    let (mut me, mut them, mut which) = ([0u32; 6], [0u32; 6], [[0u32; 4]; 2]);
    for k in 0..n {
        let c = play.client(k).corrections();
        for b in 0..6 {
            (me[b], them[b]) = (me[b] + c.avatar_off[b], them[b] + c.bodies_off[b]);
        }
        for (w, o) in which.iter_mut().zip(c.off_in) {
            for (a, b) in w.iter_mut().zip(o) {
                *a += b;
            }
        }
    }
    println!(
        "at the rewinds, off by (under 2, 5, 10, 30, 100 mm, beyond): the avatar {me:?}, the bodies {them:?}"
    );
    println!(
        "off in (position, velocity, spin, orientation): the avatar {:?}, the bodies {:?}",
        which[0], which[1]
    );
    println!(
        "rewound for the avatar {}, for a body near it {}, for the bodies chosen {}; the avatar's group (0: asleep, 1: alone, 2.., 5+) {grouped:?}",
        why.0, why.1, why.2
    );
    let (nudged, over, largest) = (0..n).fold((0, 0, 0.0f64), |a, k| {
        let c = play.client(k).corrections();
        (
            a.0 + c.nudged,
            a.1 + c.over_a_centimetre,
            a.2.max(c.largest),
        )
    });
    println!(
        "moved in place {nudged}; avatar corrections over 1 cm {over}, the largest {largest:.4} m"
    );
    let mut worst = rows.clone();
    worst.sort_by(|a, b| b.2.cmp(&a.2));
    for r in worst.iter().take(12) {
        println!(
            "tick {} client {}: {} iterations ({} replaying, {} rewound), {} bodies, {} awake",
            r.0, r.1, r.2, r.3, r.4, r.6, r.7
        );
    }
    // Per hundred ticks: the most work and the mean (when the cost comes).
    for chunk in rows.chunks(100 * n) {
        let most = chunk.iter().map(|r| r.2).max().unwrap_or(0);
        let mean = chunk.iter().map(|r| r.2 as f64).sum::<f64>() / chunk.len() as f64;
        let rew: u32 = chunk.iter().map(|r| r.4).sum();
        println!(
            "ticks {:4}..: most {most:5}, mean {mean:7.1}, rewinds {rew}",
            chunk[0].0
        );
    }
}
