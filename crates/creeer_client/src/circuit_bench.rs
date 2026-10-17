//! F6.3 circuit bench: small circuits stepped live, with an oscilloscope (scrolling traces)
//! and their energy ledger. A capacitor charging, a motor run from a battery, a solar module
//! charging a battery through a day, a transformer feeding a load and an induction motor.

use creeer_electric::{
    Battery, Branch, Circuit, ComponentId, DcMachine, Diode, GROUND, Grid, InductionMotor, Kind,
    SolarModule,
};
use creeer_math::sparse::Complex;
use creeer_thermal::environment::{EARTH_CLEAR_SKY, SOLAR_CONSTANT, direct_sunlight};
use creeer_ui::egui;
use std::collections::VecDeque;

/// The step (s) and how much of the trace is kept (s of bench time).
const TICK: f64 = 1.0 / 60.0;
const WINDOW: f64 = 12.0;

/// The benches on offer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Bench {
    Capacitor,
    Motor,
    Solar,
    Transformer,
}

impl Bench {
    const ALL: [(Self, &'static str); 4] = [
        (Self::Capacitor, "capacitor"),
        (Self::Motor, "motor on a battery"),
        (Self::Solar, "solar through a day"),
        (Self::Transformer, "transformer and motor"),
    ];
}

/// One trace of the oscilloscope.
struct Trace {
    name: &'static str,
    points: VecDeque<[f64; 2]>,
}

/// What the bench is running, and the handles it reads.
enum Rig {
    Dc {
        circuit: Box<Circuit>,
        parts: Vec<ComponentId>,
        nodes: Vec<usize>,
    },
    Ac {
        grid: Box<Grid>,
        parts: Vec<usize>,
        buses: Vec<usize>,
    },
}

/// The circuit bench.
pub struct CircuitBench {
    bench: Bench,
    rig: Rig,
    /// Bench time (s), and how much faster than real time it runs.
    time: f64,
    lapse: f64,
    carry: f64,
    traces: Vec<Trace>,
    /// The controls: a switch, a load (N·m or kW).
    on: bool,
    load: f64,
}

impl std::fmt::Debug for CircuitBench {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CircuitBench")
            .field("bench", &self.bench)
            .field("time", &self.time)
            .finish_non_exhaustive()
    }
}

impl CircuitBench {
    /// The capacitor bench.
    pub fn new() -> Self {
        Self::build(Bench::Capacitor)
    }

    fn build(bench: Bench) -> Self {
        let (rig, lapse, names, load): (Rig, f64, &[&'static str], f64) = match bench {
            Bench::Capacitor => {
                // 12 V through 1 kΩ into 1 mF (τ = 1 s); the switch sends it to the source or
                // to a 2 kΩ discharge resistor.
                let mut c = Circuit::new();
                let (top, mid, cap, drain) = (c.node(), c.node(), c.node(), c.node());
                c.voltage_source(top, GROUND, 12.0);
                let charge = c.add(Kind::Switch, top, mid);
                let r = c.resistor(mid, cap, 1000.0);
                c.capacitor(cap, GROUND, 1e-3);
                let discharge = c.add(Kind::Switch, cap, drain);
                c.resistor(drain, GROUND, 2000.0);
                c.set_closed(charge, false);
                let rig = Rig::Dc {
                    circuit: Box::new(c),
                    parts: vec![charge, discharge, r],
                    nodes: vec![cap],
                };
                (rig, 1.0, &["capacitor (V)", "current (mA)"], 0.0)
            }
            Bench::Motor => {
                let mut c = Circuit::new();
                let (mid, top, fed) = (c.node(), c.node(), c.node());
                c.battery(mid, GROUND, Battery::lead_acid_12v_100ah());
                c.battery(top, mid, Battery::lead_acid_12v_100ah());
                let switch = c.add(Kind::Switch, top, fed);
                let motor = c.machine(fed, GROUND, DcMachine::SCOOTER_24V);
                let rig = Rig::Dc {
                    circuit: Box::new(c),
                    parts: vec![switch, motor],
                    nodes: vec![fed],
                };
                (
                    rig,
                    1.0,
                    &["speed (100 rpm)", "current (A)", "battery (V)"],
                    1.0,
                )
            }
            Bench::Solar => {
                let mut c = Circuit::new();
                let (panel, bank, mid) = (c.node(), c.node(), c.node());
                let module = c.solar(panel, GROUND, SolarModule::KC200GT);
                c.diode(panel, bank, Diode::N4007);
                c.battery(mid, GROUND, Battery::lead_acid_12v_100ah());
                let upper = c.battery(bank, mid, Battery::lead_acid_12v_100ah());
                // A small load: 20 W through the day and the night.
                c.resistor(bank, GROUND, 28.0);
                let rig = Rig::Dc {
                    circuit: Box::new(c),
                    parts: vec![module, upper],
                    nodes: vec![bank],
                };
                // A day in four minutes.
                (
                    rig,
                    360.0,
                    &["sunlight (100 W/m²)", "charging (A)", "bank (V)"],
                    0.0,
                )
            }
            Bench::Transformer => {
                let mut g = Grid::new(50.0);
                let (hv, lv) = (g.bus(), g.bus());
                g.add(Branch::Source {
                    a: hv,
                    emf: Complex::new(11e3, 0.0),
                    z: Complex::new(0.5, 4.0),
                });
                let base = 11e3 * 11e3 / 250e3;
                g.add(Branch::Transformer {
                    a: hv,
                    b: lv,
                    ratio: 11e3 / 400.0,
                    z: Complex::new(0.0097, 0.0388) * Complex::from(base),
                    y: Complex::new(0.002, -0.015) / Complex::from(base),
                });
                let load = g.add(Branch::Load {
                    a: lv,
                    power: Complex::new(50e3, 15e3),
                });
                let rig = Rig::Ac {
                    grid: Box::new(g),
                    parts: vec![load],
                    buses: vec![lv],
                };
                (
                    rig,
                    1.0,
                    &["motor (100 rpm)", "LV bus (10 V)", "motor (kW)"],
                    20.0,
                )
            }
        };
        let traces = names
            .iter()
            .map(|&name| Trace {
                name,
                points: VecDeque::new(),
            })
            .collect();
        Self {
            bench,
            rig,
            time: 0.0,
            lapse,
            carry: 0.0,
            traces,
            on: false,
            load,
        }
    }

    /// Steps the bench by `real` seconds of real time (at most a quarter of a second).
    pub fn advance(&mut self, real: f64) {
        self.carry += real.min(0.25);
        while self.carry >= TICK {
            self.carry -= TICK;
            self.tick(TICK * self.lapse);
        }
    }

    fn tick(&mut self, dt: f64) {
        self.time += dt;
        let t = self.time;
        let values: Vec<f64> = match &mut self.rig {
            Rig::Dc {
                circuit,
                parts,
                nodes,
            } => {
                match self.bench {
                    Bench::Capacitor => {
                        circuit.set_closed(parts[0], self.on);
                        circuit.set_closed(parts[1], !self.on);
                    }
                    Bench::Motor => {
                        circuit.set_closed(parts[0], self.on);
                        circuit.set_load(parts[1], self.load);
                    }
                    Bench::Solar => {
                        // The Sun's height over a 12 h day (60° at noon), through clear air.
                        let hour = (t / 3600.0) % 24.0;
                        let height = (60f64.to_radians()
                            * creeer_math::det::sin(std::f64::consts::PI * (hour - 6.0) / 12.0))
                        .max(-0.1);
                        let zenith = std::f64::consts::FRAC_PI_2 - height;
                        let g = if height > 0.0 {
                            direct_sunlight(SOLAR_CONSTANT, zenith, Some(EARTH_CLEAR_SKY))
                                * creeer_math::det::sin(height)
                        } else {
                            0.0
                        };
                        circuit.set_irradiance(parts[0], g);
                    }
                    Bench::Transformer => {}
                }
                circuit.step(dt);
                match self.bench {
                    Bench::Capacitor => {
                        vec![circuit.voltage(nodes[0]), circuit.current(parts[2]) * 1e3]
                    }
                    Bench::Motor => vec![
                        circuit.level(parts[1]) * 60.0 / (2.0 * std::f64::consts::PI) / 100.0,
                        circuit.current(parts[1]),
                        circuit.voltage(nodes[0]),
                    ],
                    // The upper battery's current from + to − through it: charging.
                    _ => vec![
                        circuit.irradiance(parts[0]) / 100.0,
                        circuit.current(parts[1]),
                        circuit.voltage(nodes[0]),
                    ],
                }
            }
            Rig::Ac { grid, parts, buses } => {
                if self.on && parts.len() == 1 {
                    parts.push(grid.add(Branch::Motor {
                        a: buses[0],
                        model: InductionMotor::KW7_5,
                        speed: 0.0,
                        load: 0.0,
                    }));
                }
                if let Some(&motor) = parts.get(1) {
                    grid.set_load(motor, self.load);
                }
                grid.step(dt);
                let (speed, power) = parts
                    .get(1)
                    .map_or((0.0, 0.0), |&m| (grid.speed(m), grid.power(m).re));
                vec![
                    speed * 60.0 / (2.0 * std::f64::consts::PI) / 100.0,
                    grid.voltage(buses[0]).0 / 10.0,
                    power / 1e3,
                ]
            }
        };
        for (trace, v) in self.traces.iter_mut().zip(values) {
            trace.points.push_back([t, v]);
            while trace
                .points
                .front()
                .is_some_and(|p| p[0] < t - WINDOW * self.lapse)
            {
                trace.points.pop_front();
            }
        }
    }

    /// The bench's window.
    pub fn window(&mut self, ctx: &egui::Context) {
        egui::Window::new("Circuit bench (F6.3)")
            .default_pos([900.0, 12.0])
            .default_open(false)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    for (bench, name) in Bench::ALL {
                        if ui.radio(self.bench == bench, name).clicked() {
                            *self = Self::build(bench);
                        }
                    }
                });
                ui.horizontal(|ui| {
                    let label = match self.bench {
                        Bench::Capacitor => "charge",
                        Bench::Motor => "motor on",
                        Bench::Solar => "",
                        Bench::Transformer => "start the motor",
                    };
                    if !label.is_empty() {
                        ui.checkbox(&mut self.on, label);
                    }
                    match self.bench {
                        Bench::Motor => {
                            ui.add(egui::Slider::new(&mut self.load, 0.0..=6.0).text("load (N·m)"));
                        }
                        Bench::Transformer => {
                            ui.add(
                                egui::Slider::new(&mut self.load, 0.0..=60.0)
                                    .text("motor load (N·m)"),
                            );
                        }
                        _ => {}
                    }
                    if ui.button("restart").clicked() {
                        *self = Self::build(self.bench);
                    }
                });
                self.plot(ui);
                ui.label(self.ledger());
            });
    }

    fn plot(&self, ui: &mut egui::Ui) {
        use creeer_ui::egui_plot::{Legend, Line, Plot, PlotPoints};
        let hours = self.bench == Bench::Solar;
        Plot::new("bench")
            .height(220.0)
            .legend(Legend::default())
            .x_axis_label(if hours { "hours" } else { "seconds" })
            .show(ui, |plot| {
                for trace in &self.traces {
                    let scale = if hours { 1.0 / 3600.0 } else { 1.0 };
                    let points: PlotPoints<'_> =
                        trace.points.iter().map(|p| [p[0] * scale, p[1]]).collect();
                    plot.line(Line::new(trace.name, points));
                }
            });
    }

    fn ledger(&self) -> String {
        let (ledger, stored) = match &self.rig {
            Rig::Dc { circuit, .. } => (*circuit.ledger(), circuit.stored()),
            Rig::Ac { grid, .. } => (*grid.ledger(), grid.stored()),
        };
        let (supplied, dissipated, passed, received) = ledger.totals();
        format!(
            "t = {:.1} s · supplied {supplied:.1} J · heat {dissipated:.1} J · passed on {passed:.1} J · received {received:.1} J · held {stored:.1} J · unaccounted {:.1e}",
            self.time,
            ledger.relative_imbalance(stored)
        )
    }
}

#[cfg(test)]
#[path = "circuit_bench_tests.rs"]
mod tests;
