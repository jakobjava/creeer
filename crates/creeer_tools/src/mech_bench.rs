//! F6.4 drivetrains: 1,000 cars' powertrains of ten elements each (engine and clutch,
//! gearbox, propeller shaft, open differential, two braked wheels on the road, a belt to the
//! alternator), stepped at 60 Hz in parallel.

use crate::spikes::{Better, MIN_SPEC_CORE_FACTOR, Report, p, time_ticks};
use creeer_math::glam::DVec3;
use creeer_mech::{Drivetrain, Element, HOUSING, Node, belt_capacity};
use rayon::prelude::*;

/// Budget on the minimum-spec CPU (docs/f6-plan.md): 1,000 drivetrains of ten elements.
const BUDGET_MS: f64 = 0.3;

/// A car's powertrain, its wheels on the road (their loads), the `k`-th of a fleet.
pub(crate) struct Car {
    d: Drivetrain,
    engine: Node,
    wheels: [Node; 2],
    brakes: [usize; 2],
    k: usize,
}

pub(crate) fn car(k: usize) -> Car {
    let mut d = Drivetrain::new();
    let engine = d.part(0.25, DVec3::X);
    let input = d.part(0.01, DVec3::X);
    let output = d.part(0.02, DVec3::X);
    let cage = d.part(0.05, DVec3::Y);
    let left = d.part(1.2, DVec3::Y);
    let right = d.part(1.2, DVec3::Y);
    let alternator = d.part(0.004, DVec3::X);
    d.add(Element::Bearing {
        a: engine,
        viscous: 0.02,
        coulomb: 2.0,
    });
    d.add(Element::Clutch {
        a: engine,
        b: input,
        capacity: 400.0,
    });
    d.add(Element::Gear {
        a: input,
        b: output,
        ratio: 1.0 / (1.0 + (k % 5) as f64),
        efficiency: 0.97,
        backlash: 0.0,
    });
    // A propeller shaft: steel tube, G·J/L ≈ 2·10⁴ N·m/rad, lightly damped.
    d.add(Element::Shaft {
        a: output,
        b: cage,
        stiffness: 2.0e4,
        damping: 5.0,
    });
    d.add(Element::Planetary {
        sun: left,
        ring: right,
        carrier: cage,
        teeth: 1.0,
        efficiency: 0.98,
    });
    for wheel in [left, right] {
        d.add(Element::Bearing {
            a: wheel,
            viscous: 0.05,
            coulomb: 1.0,
        });
    }
    let brakes = [left, right].map(|w| {
        d.add(Element::Clutch {
            a: w,
            b: HOUSING,
            capacity: 0.0,
        })
    });
    let belt = belt_capacity(300.0, 0.07, 3.0, 0.5);
    d.add(Element::Belt {
        a: engine,
        b: alternator,
        ratio: 2.5,
        efficiency: 0.96,
        capacity: belt,
    });
    d.set_speed(engine, 100.0);
    Car {
        d,
        engine,
        wheels: [left, right],
        brakes,
        k,
    }
}

impl Car {
    /// One tick of driving: the throttle and the road's drag; every few seconds a brake.
    pub(crate) fn drive(&mut self, tick: u32) {
        let phase = (tick as usize + 37 * self.k) % 600;
        let braking = phase > 480;
        self.d
            .set_torque(self.engine, if braking { 0.0 } else { 150.0 });
        for (&wheel, &brake) in self.wheels.iter().zip(&self.brakes) {
            let w = self.d.speed(wheel);
            // Rolling resistance and air drag at the wheel (N·m): a car of 1.2 t.
            self.d
                .set_load(wheel, -(40.0 + 0.4 * w * w.abs()) * w.signum());
            self.d
                .set_capacity(brake, if braking { 900.0 } else { 0.0 });
        }
        self.d.step(1.0 / 60.0, DVec3::ZERO);
    }
}

/// Runs the drivetrain benchmark.
pub fn run(report: &mut Report) {
    println!("F6.4 drivetrains: 1,000 cars' powertrains (10 elements) at 60 Hz (ms, p50 / p99)");
    let mut cars: Vec<Car> = (0..1000).map(car).collect();
    let mut tick = 0u32;
    // Ten seconds in: some cruising, some braking.
    for _ in 0..600 {
        cars.par_iter_mut().for_each(|c| c.drive(tick));
        tick += 1;
    }
    let t = time_ticks(
        || {
            cars.par_iter_mut().for_each(|c| c.drive(tick));
            tick += 1;
        },
        300,
    );
    let worst = cars
        .iter()
        .map(|c| c.d.ledger().relative_imbalance(c.d.stored()))
        .fold(0.0, f64::max);
    println!(
        "  1000 drivetrains: {:.3} / {:.3} | worst ledger {worst:.1e}",
        p(&t, 50.0),
        p(&t, 99.0)
    );
    report.metric("mech_1000_drivetrains_p50_ms", p(&t, 50.0), Better::Lower);
    let projected = p(&t, 50.0) / MIN_SPEC_CORE_FACTOR;
    report.budget(
        projected <= BUDGET_MS,
        format!("1,000 drivetrains projected to min-spec: {projected:.2} ms <= {BUDGET_MS} ms"),
    );
    println!();
}
