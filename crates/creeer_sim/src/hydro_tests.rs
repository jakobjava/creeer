//! Turbines on drivetrains: a runner on a flywheel against its bearing settles where Euler's
//! torque meets the bearing's, its pipes stepping every tick or every sixth; braked, it holds
//! the stall torque; the joules the water gives are the joules the shaft takes, every tick.

use crate::hydro::{self, TurbineLink};
use creeer_fluids::pipes::{LinkKind, Liquid, Network as Pipes, NodeKind};
use creeer_math::glam::DVec3;
use creeer_mech::{Drivetrain, Element, HOUSING, Node};

const DT: f64 = 1.0 / 60.0;
const G: f64 = 9.806_65;
/// The drop (m), the turbine's swirl (1/m), runner radius (m) and passage loss (s²/m⁵).
const DROP: f64 = 20.0;
const SWIRL: f64 = 400.0;
const RADIUS: f64 = 0.15;
const LOSS: f64 = 2.0e4;
/// The bearing's viscous friction (N·m·s/rad).
const BEARING: f64 = 1.0;

/// A turbine between two reservoirs `DROP` apart, its runner a 2 kg·m² flywheel on a viscous
/// bearing (and, with `brake`, a brake of that capacity to the housing).
fn plant(brake: f64) -> (Pipes, Vec<Drivetrain>, [TurbineLink; 1], Node) {
    let mut pipes = Pipes::new(Liquid::WATER_20C, G);
    let upper = pipes.node(DROP, NodeKind::Reservoir, DROP);
    let lower = pipes.node(0.0, NodeKind::Reservoir, 0.0);
    let turbine = pipes.link(
        upper,
        lower,
        LinkKind::Turbine {
            swirl: SWIRL,
            radius: RADIUS,
            loss: LOSS,
            speed: 0.0,
        },
    );
    let mut d = Drivetrain::new();
    let runner = d.part(2.0, DVec3::Z);
    d.add(Element::Bearing {
        a: runner,
        viscous: BEARING,
        coulomb: 0.0,
    });
    if brake > 0.0 {
        d.add(Element::Clutch {
            a: runner,
            b: HOUSING,
            capacity: brake,
        });
    }
    let link = TurbineLink {
        turbine,
        drivetrain: 0,
        node: runner,
    };
    (pipes, vec![d], [link], runner)
}

/// The flow through the turbine at runner speed `omega`: the drop is Euler's head and the
/// passages' loss, H = ω·(a·Q − r²·ω)/g + k·Q², a quadratic in Q.
fn flow_at(omega: f64) -> f64 {
    let (b, c) = (
        omega * SWIRL / G,
        DROP + RADIUS * RADIUS * omega * omega / G,
    );
    (-b + (b * b + 4.0 * LOSS * c).sqrt()) / (2.0 * LOSS)
}

/// Where Euler's torque ρ·Q·(a·Q − r²·ω) meets the bearing's B·ω (bisection).
fn settled_speed(rho: f64) -> f64 {
    let excess = |w: f64| {
        let q = flow_at(w);
        rho * q * (SWIRL * q - RADIUS * RADIUS * w) - BEARING * w
    };
    let (mut low, mut high) = (0.0, 1.0e3);
    for _ in 0..200 {
        let mid = 0.5 * (low + high);
        if excess(mid) > 0.0 {
            low = mid
        } else {
            high = mid
        }
    }
    0.5 * (low + high)
}

/// Runs `seconds` of ticks, the pipes held at a step every `every` ticks: checks each tick that
/// the water's exported joules are the shaft's imported ones and both books close.
fn run(pipes: &mut Pipes, trains: &mut [Drivetrain], links: &[TurbineLink], seconds: usize) {
    for k in 0..seconds * 60 {
        let started = hydro::prepare(pipes, trains, links, DT);
        trains[0].step(DT, DVec3::ZERO);
        hydro::finish(pipes, trains, links, &started, DT);
        pipes.tick(DT);
        let exported = pipes.ledger().totals().2;
        let imported = trains[0].ledger().totals().3;
        assert!(
            (exported - imported).abs() <= 1e-12 * imported.max(1.0),
            "tick {k}: water gave {exported} J, shaft took {imported} J"
        );
        assert!(
            pipes.ledger().relative_imbalance(pipes.stored()) < 1e-12,
            "tick {k}"
        );
        let d = &trains[0];
        assert!(
            d.ledger().relative_imbalance(d.stored()) < 1e-12,
            "tick {k}"
        );
    }
}

/// Every tick and every sixth (10 Hz), the runner settles at the speed where the turbine's
/// torque at its flow meets the bearing's, within 10⁻⁶.
#[test]
fn a_turbine_spins_a_flywheel_up_to_where_its_torque_meets_the_bearings() {
    for every in [None, Some(6)] {
        let (mut pipes, mut trains, links, runner) = plant(0.0);
        pipes.hold_rate(every);
        run(&mut pipes, &mut trains, &links, 40);
        let expected = settled_speed(pipes.liquid.density);
        let w = trains[0].speed(runner);
        assert!(
            (w / expected - 1.0).abs() < 1e-6,
            "{every:?}: {w} against {expected} rad/s"
        );
        let q = pipes.links[links[0].turbine].flow;
        assert!((q / flow_at(w) - 1.0).abs() < 1e-6, "{every:?}: {q} m³/s");
    }
}

/// Braked, the runner stays still and the brake holds the stall torque ρ·a·Q² (Q the nozzle's
/// flow, √(H/k)); the shaft takes no work.
#[test]
fn a_braked_turbine_holds_its_stall_torque() {
    let (mut pipes, mut trains, links, runner) = plant(1.0e4);
    run(&mut pipes, &mut trains, &links, 5);
    assert_eq!(trains[0].speed(runner), 0.0);
    let q = (DROP / LOSS).sqrt();
    let stall = pipes.liquid.density * SWIRL * q * q;
    let held = trains[0].carried(1);
    assert!(
        (held / stall - 1.0).abs() < 1e-9,
        "brake holds {held} N·m of {stall}"
    );
    assert_eq!(pipes.ledger().totals().2, 0.0);
}
