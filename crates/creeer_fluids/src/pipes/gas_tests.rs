//! Gas pipelines against Weymouth: a 100 km, 0.5 m line from 70 to 50 bar, its flow from
//! the isothermal flow equation with Weymouth's friction factor f = 0.032/D^(1/3) (D in
//! inches); the same line in ten sections that store gas, run to its steady state.

use super::gas::{Gas, GasKind, GasNetwork};
use creeer_materials::species::R;
use creeer_math::det;

const BAR: f64 = 1e5;
const GAS: Gas = Gas::METHANE_15C;

fn weymouth(diameter: f64) -> f64 {
    0.032 / det::cbrt(diameter / 0.0254)
}

fn exact_flow(diameter: f64, length: f64, p1: f64, p2: f64) -> f64 {
    let area = 0.25 * std::f64::consts::PI * diameter * diameter;
    area * ((p1 * p1 - p2 * p2) * diameter * GAS.molar_mass
        / (weymouth(diameter) * length * R * GAS.temperature))
        .sqrt()
}

#[test]
fn a_pipeline_carries_what_weymouth_says() {
    let mut net = GasNetwork::new(GAS);
    let (a, b) = (
        net.node(GasKind::Supply, 70.0 * BAR),
        net.node(GasKind::Supply, 50.0 * BAR),
    );
    let p = net.pipe(a, b, 100_000.0, 0.5, 0.0, Some(weymouth(0.5)));
    assert!(net.step(1.0));
    let exact = exact_flow(0.5, 100_000.0, 70.0 * BAR, 50.0 * BAR);
    assert!(
        (net.pipes[p].flow / exact - 1.0).abs() < 1e-9,
        "{} kg/s, Weymouth {exact}",
        net.pipes[p].flow
    );
}

/// In ten sections, each storing its share of the line's volume, from empty-ish to steady:
/// the same flow in every section, p² falling linearly, line pack and books right.
#[test]
fn a_sectioned_pipeline_settles_to_weymouth() {
    let (d, l, sections) = (0.5, 100_000.0, 10);
    let mut net = GasNetwork::new(GAS);
    let inlet = net.node(GasKind::Supply, 70.0 * BAR);
    let volume = 0.25 * std::f64::consts::PI * d * d * l / sections as f64;
    let stores: Vec<usize> = (1..sections)
        .map(|_| {
            net.node(
                GasKind::Store {
                    volume,
                    demand: 0.0,
                },
                50.0 * BAR,
            )
        })
        .collect();
    let outlet = net.node(GasKind::Supply, 50.0 * BAR);
    let mut chain = vec![inlet];
    chain.extend(&stores);
    chain.push(outlet);
    for w in chain.windows(2) {
        net.pipe(w[0], w[1], l / sections as f64, d, 0.0, Some(weymouth(d)));
    }
    for _ in 0..2000 {
        assert!(net.step(120.0));
    }
    let exact = exact_flow(d, l, 70.0 * BAR, 50.0 * BAR);
    for pipe in &net.pipes {
        assert!(
            (pipe.flow / exact - 1.0).abs() < 1e-6,
            "{} kg/s, Weymouth {exact}",
            pipe.flow
        );
    }
    for (k, &s) in stores.iter().enumerate() {
        let x = (k + 1) as f64 / sections as f64;
        let p2 = (70.0 * BAR) * (70.0 * BAR) * (1.0 - x) + (50.0 * BAR) * (50.0 * BAR) * x;
        assert!(
            (net.nodes[s].pressure / p2.sqrt() - 1.0).abs() < 1e-6,
            "section {k}"
        );
    }
    assert!(
        net.ledger().relative_imbalance(net.stored()) < 1e-9,
        "{}",
        net.ledger().relative_imbalance(net.stored())
    );
}
