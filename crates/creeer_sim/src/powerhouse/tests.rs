//! The powerhouse over ten minutes: its controller holds the shaft's speed, and every joule
//! the water gave, every cubic metre of water, every kilogram in the reactor and the angular
//! momentum of platform and shaft are accounted for: with the pipes stepping every tick, and
//! every sixth (10 Hz, their step beside the ticks).

use super::*;
use creeer_chem::electrolysis::FARADAY;
use creeer_fluids::pipes::LinkKind;

/// Ten minutes of ticks, the pipes held at a step every `every` ticks; the shaft's worst
/// distance from its setpoint over the last eight minutes (rad/s) and the charge through the
/// stack (C).
fn run(every: Option<u32>) -> (Powerhouse, f64, f64) {
    let mut house = Powerhouse::new();
    house.pipes.hold_rate(every);
    let (mut worst, mut charge) = (0.0f64, 0.0);
    for k in 0..36_000 {
        house.tick();
        charge += house.circuit.current(house.cells[0].cell) * DT;
        if k >= 7200 {
            worst = worst.max((house.trains[0].speed(house.runner) - SPEED).abs());
        }
    }
    (house, worst, charge)
}

#[test]
fn ten_minutes_of_the_powerhouse_balance_their_books() {
    for every in [None, Some(6)] {
        let (house, worst, charge) = run(every);
        let (gave, went) = house.books();
        let now = house.stores();
        eprintln!("pipes every {every:?}: the water gave {gave:.1} J; to {went:.1?}");
        // The controller holds 150 rad/s within 0.1 % with the valve part open.
        assert!(worst < 0.15, "{every:?}: {worst} rad/s off");
        let LinkKind::Valve { opening, .. } = house.pipes.links[house.valve].kind else {
            unreachable!("a valve")
        };
        assert!(
            opening > 0.2 && opening < 0.6,
            "{every:?}: valve at {opening}"
        );
        // Energy: the water's, every joule (two megajoules in ten minutes) to rounding.
        let total: f64 = went.iter().sum();
        assert!(gave > 1.5e6, "{gave} J");
        assert!(
            (gave - total).abs() < 1e-9 * gave,
            "{every:?}: {gave} J against {total}"
        );
        // Most to hydrogen (a quarter is the valve's throttling and the pipes' friction); the losses no network takes up are the shaft's twist damping
        // in the first jolt (`creeer_mech` books it, but not as anyone's heat).
        assert!(
            went[2] > 0.5 * gave,
            "hydrogen and the reactor's warmth: {}",
            went[2]
        );
        assert!(went[5].abs() < 1e-6 * gave, "unrouted {} J", went[5]);
        // Every coupling the same joules on both sides: the turbine's.
        let (_, _, exported, imported) = house.pipes.ledger().totals();
        let turbine = exported - imported;
        assert!((turbine - house.moved.turbine).abs() < 1e-9 * turbine);
        let (_, _, shaft_out, shaft_in) = house.trains[0].ledger().totals();
        assert!(
            (shaft_in - turbine).abs() < 1e-9 * turbine,
            "{shaft_in} against {turbine}"
        );
        let (_, _, _, circuit_in) = house.circuit.ledger().totals();
        assert!((circuit_in - shaft_out).abs() < 1e-9 * shaft_out);
        // Each domain's books close.
        let closes = [
            house.pipes.ledger().relative_imbalance(now.water),
            house.trains[0].ledger().relative_imbalance(now.shaft),
            house.circuit.ledger().relative_imbalance(now.circuit),
            house.reactor.ledger().relative_imbalance(now.reactor),
            house.heat.ledger().relative_imbalance(now.heat),
        ];
        assert!(closes.iter().all(|&c| c < 1e-9), "{closes:?}");
        // Mass: the water between the tanks, the reactor's; Faraday's hydrogen.
        assert!((now.volume - house.start.volume).abs() < 1e-12 * now.volume);
        assert!((now.mass - house.start.mass).abs() < 1e-12 * now.mass);
        let at = |name: &str| house.reactor.moles[house.sys.index(name).expect(name)];
        let hydrogen = at("H2");
        let faraday = 12.0 * charge / (2.0 * FARADAY);
        assert!(
            (hydrogen / faraday - 1.0).abs() < 1e-3,
            "{hydrogen} mol H₂, {faraday}"
        );
        assert!((at("O2") - hydrogen / 2.0).abs() < 1e-12 * hydrogen);
        // Angular momentum: the shaft's 110 kg·m²/s the platform's, opposite, to its solver's
        // tolerance.
        let shaft = house.trains[0].momentum().length();
        let drift = (now.momentum - house.start.momentum).length();
        assert!(
            drift < 1e-7 * shaft,
            "{every:?}: {drift} kg·m²/s of {shaft}"
        );
        let spin = house.world.bodies[0].structure.angular_velocity[0].z;
        assert!(
            spin < -0.03,
            "the platform turns the other way: {spin} rad/s"
        );
        // And everything warmed.
        let t = house.thermals;
        for node in [t.generator, t.frame, t.coolant] {
            assert!(house.heat.temperature(node) > 293.16);
        }
        assert!(
            house.reactor.t > 296.0,
            "the stack at {} K",
            house.reactor.t
        );
    }
}
