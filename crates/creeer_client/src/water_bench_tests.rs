//! The pipe bench's rigs behave as their physics says, and keep their books.

use super::*;

fn run(bench: &mut WaterBench, seconds: u32) {
    for _ in 0..seconds * 60 {
        bench.tick();
    }
}

/// The waterworks: at full speed the pump fills the tank faster than the half-open valve
/// lets it out; shut down, the tank drains; dye put in the well reaches the taps only after
/// it has gone up the riser and through the tank, mixed there. Every joule and every gram of
/// dye booked. It steps every tick (a small network).
#[test]
fn the_waterworks_fills_drains_and_carries_its_dye() {
    let mut bench = WaterBench::new();
    assert!(bench.status().contains("60 Hz"));
    let start = bench.works().0.nodes[bench.works().2].head;
    bench.dyed = true;
    // Up the riser (0.6 m³, 20 s), mixed into the tank, down the main (2.4 m³, 2 min).
    run(&mut bench, 240);
    let (net, dye, tank) = bench.works();
    let filled = net.nodes[tank].head;
    assert!(filled > start, "the pump fills it: {start} → {filled} m");
    // Mixed in 20 m² of tank, the dye arrives diluted.
    let at_taps = dye.delivered(3, 4)[0];
    assert!(at_taps > 0.0 && at_taps < 1.0, "dye at the taps: {at_taps}");
    bench.speed = 0.0;
    run(&mut bench, 60);
    let (net, dye, tank) = bench.works();
    assert!(net.nodes[tank].head < filled, "stopped, it drains");
    assert!(net.ledger().relative_imbalance(net.stored()) < 1e-9);
    assert!(dye.ledger(1).relative_imbalance(dye.held()[1]) < 1e-9);
}

/// The gas line: when the city draws more, its gate pressure sags while the line pack
/// covers the difference; the books close.
#[test]
fn the_gas_line_holds_up_the_city_from_its_line_pack() {
    let mut bench = WaterBench::build(Rig::GasLine);
    run(&mut bench, 120);
    let before = bench.line.net.nodes[bench.line.gate].pressure;
    let pack = bench.line.net.stored();
    bench.demand = 50.0;
    run(&mut bench, 60);
    let net = &bench.line.net;
    assert!(
        net.nodes[bench.line.gate].pressure < before,
        "the gate pressure sags"
    );
    assert!(net.stored() < pack, "the line pack covers it");
    assert!(net.ledger().relative_imbalance(net.stored()) < 1e-9);
}
