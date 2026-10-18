//! Tanks running dry: never below their floor, empty when Torricelli says, stopped there,
//! filled again when the head beyond them rises.

use super::*;
use creeer_math::det;

const G: f64 = 9.806_65;
const WATER: Liquid = Liquid::WATER_20C;

/// A tank of 1 m² (floor at 0, 2 m of water) emptying through a 50 mm orifice into a
/// reservoir 3 m below its floor: the drop beyond the floor would draw water out for ever.
fn tank_over_a_drop() -> (Network, usize, usize, usize) {
    let mut net = Network::new(WATER, G);
    let tank = net.node(
        0.0,
        NodeKind::Tank {
            area: 1.0,
            floor: 0.0,
        },
        2.0,
    );
    let low = net.node(-3.0, NodeKind::Reservoir, -3.0);
    let orifice = net.link(
        tank,
        low,
        LinkKind::Valve {
            diameter: 0.05,
            open: 1.0,
            opening: 1.0,
        },
    );
    (net, tank, low, orifice)
}

#[test]
fn a_tank_runs_dry_at_its_floor_when_torricelli_says_and_stays_dry() {
    let (mut net, tank, _, orifice) = tank_over_a_drop();
    let dt = 0.01;
    let (mut t, mut empty) = (0.0, None);
    let start = net.stored();
    for _ in 0..20_000 {
        assert!(net.step(dt).converged);
        t += dt;
        let level = net.nodes[tank].head;
        assert!(level >= -1e-9, "below its floor: {level} m at {t} s");
        if empty.is_none() && level <= 1e-9 {
            empty = Some(t);
        }
    }
    // √(h + 3) falls linearly at (a/A)·√(g/2) from √5 to √3.
    let a = 0.25 * std::f64::consts::PI * 0.05 * 0.05;
    let exact = (det::pow(5.0, 0.5) - det::pow(3.0, 0.5)) / (a * det::pow(G / 2.0, 0.5));
    let empty = empty.expect("it runs dry");
    eprintln!("dry at {empty} s (Torricelli {exact} s)");
    assert!(
        (empty - exact).abs() < 0.01 * exact,
        "{empty} against {exact}"
    );
    // Dry: nothing flows out any more, and the books close.
    assert!(
        net.links[orifice].flow.abs() < 1e-12,
        "{}",
        net.links[orifice].flow
    );
    assert!(net.ledger().relative_imbalance(net.stored()) < 1e-9);
    assert!(net.stored() < start);
}

#[test]
fn a_dry_tank_fills_again_when_the_head_beyond_rises() {
    let (mut net, tank, low, _) = tank_over_a_drop();
    for _ in 0..15_000 {
        net.step(0.01);
    }
    assert!(net.nodes[tank].head.abs() <= 1e-9);
    // The reservoir rises 1 m above the tank's floor: water flows back in (60 s).
    net.nodes[low].head = 1.0;
    for _ in 0..6000 {
        net.step(0.01);
    }
    // √(1 − h) falls linearly to nought over T = 1/((a/A)·√(g/2)) (Torricelli again).
    let a = 0.25 * std::f64::consts::PI * 0.05 * 0.05;
    let span = 1.0 / (a * det::pow(G / 2.0, 0.5));
    let exact = 1.0 - (1.0 - 60.0 / span) * (1.0 - 60.0 / span);
    let level = net.nodes[tank].head;
    assert!(
        (level - exact).abs() < 0.01 * exact,
        "filled to {level} m, Torricelli {exact}"
    );
}
