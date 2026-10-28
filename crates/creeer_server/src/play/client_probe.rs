//! Probes of the clients' own work in the play scenes (2026-10-28, the user's "30 ms frames"
//! knapping): where it goes, tick by tick (`cargo test --release -p creeer_server --lib
//! client_work -- --ignored --nocapture`; `--features creeer_physics/profile` for the phases).

use crate::play::Play;
use crate::play::record::Act;
use crate::play::scenes::{scene, terrain};
use creeer_math::glam::DVec3;
use creeer_net::message::Command;
use std::time::Duration;

/// A scene's slow ticks of the clients' own work (`SCENE`, *toolmaking* if unset; `TICKS`): the
/// physics' phases in the clients' predictions and in the server's tick, and each client's time
/// setting back and replaying, the bodies it predicts and the inputs it replays.
#[test]
#[ignore = "prints each slow tick of the clients' own work"]
fn client_work() {
    let name = std::env::var("SCENE").unwrap_or_else(|_| "toolmaking".into());
    let s = scene(&name).unwrap();
    let mut play = Play::new((s.build)(terrain()), s.name, s.players, &terrain());
    let ticks: u32 = (std::env::var("TICKS").ok())
        .and_then(|t| t.parse().ok())
        .unwrap_or(s.ticks.0);
    let phases = |spent: &[u64; creeer_physics::profile::SLOTS]| {
        (creeer_physics::profile::NAMES.iter())
            .filter(|(s, _)| spent[*s as usize] > 0)
            .map(|(s, n)| format!("{n} {}", spent[*s as usize]))
            .collect::<Vec<_>>()
            .join(", ")
    };
    let mut replayed = vec![Duration::ZERO; play.players()];
    let mut most = (0.0, 0.0, 0);
    for t in 0..ticks {
        let acts = (s.script)(t, &play);
        let c = play.tick(&acts);
        most = (
            c.ms.max(most.0),
            c.client_ms.max(most.1),
            c.client_work.max(most.2),
        );
        if t + 1 == ticks {
            println!(
                "{ticks} ticks: the server's worst {:.2} ms; a client's worst {:.2} ms, its most work {}",
                most.0, most.1, most.2
            );
        }
        if c.client_ms > 2.0 {
            println!("tick {t}: clients' phases {}", phases(&c.client_phases));
            println!("tick {t}: server's phases {}", phases(&c.phases));
        }
        for (k, was) in replayed.iter_mut().enumerate() {
            let client = play.client(k);
            let r = client.corrections().replaying;
            let spent = r - *was;
            *was = r;
            let p = client.predicted.as_ref().expect("predicted");
            if spent.as_secs_f64() > 2e-3 {
                println!(
                    "tick {t} player {k}: a client {:.2} ms at most, this one's reconciliation {:.2} ms; {} bodies, {} inputs pending; server {:.2} ms",
                    c.client_ms,
                    spent.as_secs_f64() * 1e3,
                    p.nearby_count(),
                    p.pending(),
                    c.ms
                );
            }
        }
    }
}

/// One player knapping at *toolmaking*'s first station, in lockstep: each tick, the struck
/// core's body on the server (its speed and spin, asleep or not, its Newton iterations so far),
/// and the client's prediction (its time setting back and replaying, the bodies in it).
#[test]
#[ignore = "prints the struck core's body and the client's prediction, tick by tick"]
fn knapping_one_player() {
    let s = scene("toolmaking").unwrap();
    let mut play = Play::new((s.build)(terrain()), s.name, &["ada"], &terrain());
    let start = play.avatar(0);
    let mut replayed = Duration::ZERO;
    let mut target = None;
    for t in 0..900u32 {
        let c = play.client(0);
        let me = play.avatar(0);
        let mut stones: Vec<_> = (c.laid.values())
            .filter(|l| l.word.contains("stone"))
            .map(|l| (l.id, l.centre, l.mass))
            .collect();
        stones.sort_by(|a, b| (a.1 - start).length().total_cmp(&(b.1 - start).length()));
        let centre = *target.get_or_insert(stones[0].1);
        let near: Vec<_> = (stones.iter())
            .filter(|s| (s.1 - centre).length() < 0.8)
            .copied()
            .collect();
        let up: DVec3 = centre.normalize();
        let stand = centre - (centre - start).reject_from(up).normalize() * 0.7;
        let mut act = Act::walk(super::work::towards(me, stand, 0.1));
        if t == 200 {
            let h = near.iter().min_by(|a, b| a.2.total_cmp(&b.2)).unwrap();
            act.commands.push(Command::Pick {
                loose: h.0,
                pocket: false,
            });
        }
        if [300, 450, 600, 750].contains(&t) {
            let core = near.iter().max_by(|a, b| a.2.total_cmp(&b.2)).unwrap();
            act.commands.push(c.strike_loose_command(core.0));
        }
        let cost = play.tick(&[act]);
        let c = play.client(0);
        let r = c.corrections().replaying;
        let p = c.predicted.as_ref().expect("predicted");
        let w = &play.server.world;
        let bodies: Vec<String> = (w.ids().iter().zip(&w.world.bodies))
            .filter(|(id, _)| w.item_look(**id).is_some() && !w.in_hands(**id))
            .map(|(id, b)| {
                let s = &b.structure;
                format!(
                    "{id:?} v {:.4} w {:.4} asleep {} iterations {}",
                    s.velocity[0].length(),
                    s.angular_velocity[0].length(),
                    b.sleep.asleep,
                    b.solver.iterations_total
                )
            })
            .collect();
        if t >= 300 && (!bodies.is_empty() || cost.client_ms > 1.0) {
            println!(
                "tick {t}: server {:.2} ms, client {:.2} ms (reconciliation {:.2}; {} bodies, {} inputs pending); {bodies:?}",
                cost.ms,
                cost.client_ms,
                (r - replayed).as_secs_f64() * 1e3,
                p.nearby_count(),
                p.pending()
            );
        }
        replayed = r;
    }
}
