//! People on *bridge_demo*'s deck, taken apart (G2.11 A6; run by hand): the scene as
//! `what_the_bridge_costs` plays it (standing by 5 s, then walking over and back), and for every
//! tick from `FROM` (300) to `TO` (420) whose work passes `OVER` (2,000) Newton iterations: its
//! physics phases (with `--features creeer_physics/profile`) and each group of more than one
//! body awake: its bodies (nodes, members, mass) and the Newton iterations each took.
//!
//! `cargo test -p creeer_server --release --features creeer_physics/profile deck_probe --
//! --ignored --nocapture`

use super::Play;
use super::bridge_demo::OVER as SCRIPT_OVER;
use super::scenes::{scene, terrain};

fn env(name: &str, default: u32) -> u32 {
    std::env::var(name)
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(default)
}

#[test]
#[ignore = "a measurement, run by hand"]
fn deck_probe() {
    let s = scene("bridge_demo").expect("the scene");
    let terrain = terrain();
    let mut play = Play::new((s.build)(terrain.clone()), s.name, s.players, &terrain);
    let (from, to, over) = (env("FROM", 300), env("TO", 420), env("OVER", 2_000));
    for tick in 0..to {
        let acts = if tick < 300 {
            vec![super::record::Act::default(); play.players()]
        } else {
            (s.script)(tick - 300 + SCRIPT_OVER, &play)
        };
        let cost = play.tick(&acts);
        if tick < from || cost.work < u64::from(over) {
            continue;
        }
        let phases: Vec<String> = (creeer_physics::profile::NAMES.iter())
            .filter(|(s, _)| cost.phases[*s as usize] > 0)
            .filter(|(s, _)| creeer_physics::profile::is_count(*s))
            .map(|(s, n)| format!("{n} {}", cost.phases[*s as usize]))
            .collect();
        let mut times: Vec<(u64, &str)> = (creeer_physics::profile::NAMES.iter())
            .filter(|(s, _)| !creeer_physics::profile::is_count(*s))
            .map(|(s, n)| (cost.phases[*s as usize], *n))
            .collect();
        times.sort_by(|a, b| b.0.cmp(&a.0));
        let times: Vec<String> = (times.iter().take(6))
            .map(|(ns, n)| format!("{n} {:.1} ms", *ns as f64 / 1e6))
            .collect();
        println!(
            "tick {tick}: {:.1} ms, {} work; {}",
            cost.ms,
            cost.work,
            phases.join(", ")
        );
        println!("  time: {}", times.join(", "));
        let w = &play.server.world.world;
        for group in w.groups().filter(|g| g.len() > 1) {
            let bodies: Vec<String> = (group.iter())
                .map(|&b| {
                    let body = &w.bodies[b];
                    let st = &body.structure;
                    format!(
                        "{}n{}e {:.0}kg {}it{}",
                        st.node_count(),
                        st.element_count(),
                        st.total_mass(),
                        body.solver.last_iterations,
                        if body.held { " held" } else { "" }
                    )
                })
                .collect();
            println!("  group of {}: {}", group.len(), bodies.join(" | "));
        }
    }
}
