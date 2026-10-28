//! A client's own work a frame after trees are felled (2026-10-28): *lumberjack* played in
//! lockstep (`creeer_server::play`), then its first player's frame-by-frame work timed on what
//! its connection knows, each call the best of a few (the machine is shared).

use crate::tree_view::TreeView;
use creeer_math::glam::DVec3;
use creeer_server::play::scenes::{scene, terrain};
use std::time::Instant;

/// The best of `runs` calls of `f` (ms).
fn best(runs: usize, mut f: impl FnMut()) -> f64 {
    (0..runs)
        .map(|_| {
            let start = Instant::now();
            f();
            start.elapsed().as_secs_f64() * 1e3
        })
        .fold(f64::INFINITY, f64::min)
}

#[test]
#[ignore = "times a client's frame work beside felled trees"]
fn frame_work_beside_felled_trees() {
    let s = scene("lumberjack").unwrap();
    let mut play =
        creeer_server::play::Play::new((s.build)(terrain()), s.name, s.players, &terrain());
    let terrain = terrain();
    let mut view = TreeView::new(&terrain).expect("trees");
    for t in 0..s.ticks.0 {
        let acts = (s.script)(t, &play);
        play.tick(&acts);
        if t % 300 != 299 {
            continue;
        }
        let c = play.client(0);
        let me = c.avatar().map_or(DVec3::ZERO, |a| a.position);
        let edits = (c.edits_generation, c.edits.log().len());
        let keep = best(1, || {
            view.update(&terrain, me, edits);
            view.keep_felled(c);
        });
        let parts = (c.kinds.values())
            .filter(|k| matches!(k, creeer_net::entity::Kind::Tree(_)))
            .count();
        let draw = best(5, || {
            let mut out = crate::physics_sandbox::PhysicsSnapshot::default();
            view.draw(&mut out, c);
        });
        let up = me.normalize();
        // Along each felled part, as a crosshair sweeping over them (its first aim builds what
        // it meets).
        let poses = c.poses();
        let rays: Vec<(DVec3, DVec3)> = (c.kinds.iter())
            .filter(|(_, k)| matches!(k, creeer_net::entity::Kind::Tree(_)))
            .filter_map(|(id, _)| poses.get(id))
            .map(|p| (me + up * 1.6, (p.position - me - up * 1.6).normalize()))
            .chain([(me + up * 1.6, (DVec3::X - up * 0.3).normalize())])
            .collect();
        let first = best(1, || {
            for (o, d) in &rays {
                let _ = c.aim(*o, *d, 30.0);
            }
        });
        let aim = best(5, || {
            for (o, d) in &rays {
                let _ = c.aim(*o, *d, 30.0);
            }
        }) / rays.len() as f64;
        let poses = best(5, || {
            let _ = c.poses();
        });
        println!(
            "tick {t}: {parts} felled parts known; trees kept {keep:.3} ms, drawn {draw:.3} ms; {} aims first {first:.3} ms, then {aim:.3} ms each; poses {poses:.3} ms",
            rays.len()
        );
    }
}
