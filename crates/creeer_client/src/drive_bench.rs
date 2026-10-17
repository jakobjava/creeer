//! F6.4 drivetrain bench: a battery-fed DC motor on a drivetrain, circuit and shaft solved
//! as one (`creeer_sim::drive`), with an oscilloscope and both ledgers. A gearbox driving a
//! flywheel with a brake on it; a belt drive with a load that can pull past what the belt
//! grips (it slips).

use creeer_electric::{Battery, Circuit, ComponentId, DcMachine, GROUND, Kind};
use creeer_math::glam::DVec3;
use creeer_mech::{Drivetrain, Element, Grip, HOUSING, Node, belt_capacity};
use creeer_sim::drive::{self, MachineLink};
use creeer_ui::egui;
use std::collections::VecDeque;

const TICK: f64 = 1.0 / 60.0;
/// Seconds of trace kept.
const WINDOW: f64 = 12.0;
const RPM: f64 = 60.0 / std::f64::consts::TAU;

/// The rigs on offer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Rig {
    Gearbox,
    Belt,
}

impl Rig {
    const ALL: [(Self, &'static str); 2] = [
        (Self::Gearbox, "gearbox, flywheel and brake"),
        (Self::Belt, "belt drive under load"),
    ];
}

/// The bench.
pub struct DriveBench {
    rig: Rig,
    circuit: Box<Circuit>,
    trains: Vec<Drivetrain>,
    links: [MachineLink; 1],
    switch: ComponentId,
    /// The output part, and the element whose heat is shown (brake or belt).
    output: Node,
    lossy: usize,
    time: f64,
    carry: f64,
    traces: Vec<(&'static str, VecDeque<[f64; 2]>)>,
    /// The controls: the motor's switch; the brake's grip or the load (N·m).
    on: bool,
    setting: f64,
}

impl std::fmt::Debug for DriveBench {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DriveBench")
            .field("rig", &self.rig)
            .field("time", &self.time)
            .finish_non_exhaustive()
    }
}

impl DriveBench {
    /// The gearbox rig.
    pub fn new() -> Self {
        Self::build(Rig::Gearbox)
    }

    fn build(rig: Rig) -> Self {
        // Two 12 V lead-acid batteries and a switch to a scooter motor whose rotor is the
        // drivetrain's first part (its inertia and friction the drivetrain's).
        let mut c = Circuit::new();
        let (mid, top, fed) = (c.node(), c.node(), c.node());
        c.battery(mid, GROUND, Battery::lead_acid_12v_100ah());
        c.battery(top, mid, Battery::lead_acid_12v_100ah());
        let switch = c.add(Kind::Switch, top, fed);
        let scooter = DcMachine::SCOOTER_24V;
        let machine = c.machine(
            fed,
            GROUND,
            DcMachine {
                inertia: 0.0,
                friction: 0.0,
                ..scooter
            },
        );
        let mut d = Drivetrain::new();
        let rotor = d.part(scooter.inertia, DVec3::X);
        d.add(Element::Bearing {
            a: rotor,
            viscous: scooter.friction,
            coulomb: 0.0,
        });
        let (output, lossy, names, setting): (Node, usize, [&'static str; 4], f64) = match rig {
            Rig::Gearbox => {
                // 8 : 1 through two meshes (97 % each), onto a 0.6 kg·m² steel flywheel.
                let wheel = d.part(0.6, DVec3::X);
                d.add(Element::Gear {
                    a: rotor,
                    b: wheel,
                    ratio: 1.0 / 8.0,
                    efficiency: 0.94,
                    backlash: 0.0,
                });
                d.add(Element::Bearing {
                    a: wheel,
                    viscous: 0.002,
                    coulomb: 0.1,
                });
                let brake = d.add(Element::Clutch {
                    a: wheel,
                    b: HOUSING,
                    capacity: 0.0,
                });
                (
                    wheel,
                    brake,
                    [
                        "motor (100 rpm)",
                        "current (A)",
                        "flywheel (10 rpm)",
                        "brake heat (W)",
                    ],
                    0.0,
                )
            }
            Rig::Belt => {
                // A V-belt pretensioned to 150 N on a 30 mm pulley (half a turn, μ 0.35),
                // onto a 75 mm pulley driving a fan-like load.
                let capacity = belt_capacity(150.0, 0.03, std::f64::consts::PI, 0.35);
                let pulley = d.part(0.02, DVec3::X);
                let belt = d.add(Element::Belt {
                    a: rotor,
                    b: pulley,
                    ratio: 0.4,
                    efficiency: 0.96,
                    capacity,
                });
                d.add(Element::Bearing {
                    a: pulley,
                    viscous: 0.004,
                    coulomb: 0.05,
                });
                (
                    pulley,
                    belt,
                    [
                        "motor (100 rpm)",
                        "current (A)",
                        "pulley ÷ ratio (100 rpm)",
                        "belt heat (W)",
                    ],
                    1.0,
                )
            }
        };
        Self {
            rig,
            circuit: Box::new(c),
            trains: vec![d],
            links: [MachineLink {
                machine,
                drivetrain: 0,
                node: rotor,
            }],
            switch,
            output,
            lossy,
            time: 0.0,
            carry: 0.0,
            traces: names.iter().map(|&n| (n, VecDeque::new())).collect(),
            on: false,
            setting,
        }
    }

    /// Steps the bench by `real` seconds of real time (at most a quarter of a second).
    pub fn advance(&mut self, real: f64) {
        self.carry += real.min(0.25);
        while self.carry >= TICK {
            self.carry -= TICK;
            self.tick();
        }
    }

    fn tick(&mut self) {
        self.time += TICK;
        self.circuit.set_closed(self.switch, self.on);
        let d = &mut self.trains[0];
        match self.rig {
            Rig::Gearbox => d.set_capacity(self.lossy, self.setting),
            Rig::Belt => {
                // The load's torque against its turning (N·m).
                let w = d.speed(self.output);
                d.set_load(self.output, -self.setting * w.clamp(-1.0, 1.0));
            }
        }
        drive::step(
            &mut self.circuit,
            &mut self.trains,
            &self.links,
            TICK,
            &[DVec3::ZERO],
        );
        let d = &mut self.trains[0];
        let heat = d.take_heat(self.lossy) / TICK;
        let motor = d.speed(self.links[0].node);
        let output = match self.rig {
            Rig::Gearbox => d.speed(self.output) * RPM / 10.0,
            Rig::Belt => d.speed(self.output) / 0.4 * RPM / 100.0,
        };
        let values = [
            motor * RPM / 100.0,
            self.circuit.current(self.links[0].machine),
            output,
            heat,
        ];
        let t = self.time;
        for ((_, points), v) in self.traces.iter_mut().zip(values) {
            points.push_back([t, v]);
            while points.front().is_some_and(|p| p[0] < t - WINDOW) {
                points.pop_front();
            }
        }
    }

    /// The bench's window.
    pub fn window(&mut self, ctx: &egui::Context) {
        egui::Window::new("Drivetrain bench (F6.4)")
            .default_pos([900.0, 340.0])
            .default_open(false)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    for (rig, name) in Rig::ALL {
                        if ui.radio(self.rig == rig, name).clicked() {
                            *self = Self::build(rig);
                        }
                    }
                });
                ui.horizontal(|ui| {
                    ui.checkbox(&mut self.on, "motor on");
                    let (range, text) = match self.rig {
                        Rig::Gearbox => (0.0..=40.0, "brake (N·m)"),
                        Rig::Belt => (0.0..=16.0, "load (N·m)"),
                    };
                    ui.add(egui::Slider::new(&mut self.setting, range).text(text));
                    if ui.button("restart").clicked() {
                        *self = Self::build(self.rig);
                    }
                });
                self.plot(ui);
                ui.label(self.status());
            });
    }

    fn plot(&self, ui: &mut egui::Ui) {
        use creeer_ui::egui_plot::{Legend, Line, Plot, PlotPoints};
        Plot::new("drive bench")
            .height(220.0)
            .legend(Legend::default())
            .x_axis_label("seconds")
            .show(ui, |plot| {
                for (name, points) in &self.traces {
                    let points: PlotPoints<'_> = points.iter().copied().collect();
                    plot.line(Line::new(*name, points));
                }
            });
    }

    /// What the drivetrain is doing and where both domains' energy went.
    pub fn status(&self) -> String {
        let d = &self.trains[0];
        let grip = match d.grip(self.lossy) {
            Grip::Stuck => "gripping",
            _ => "slipping",
        };
        let (_, heat, _, received) = d.ledger().totals();
        let (supplied, _, passed, _) = self.circuit.ledger().totals();
        format!(
            "t = {:.1} s · {} {grip} · batteries gave {supplied:.0} J, of which the motor passed on {passed:.0} J · drivetrain received {received:.0} J, heat {heat:.0} J, holds {:.0} J · unaccounted {:.1e} and {:.1e}",
            self.time,
            match self.rig {
                Rig::Gearbox => "brake",
                Rig::Belt => "belt",
            },
            d.stored(),
            self.circuit
                .ledger()
                .relative_imbalance(self.circuit.stored()),
            d.ledger().relative_imbalance(d.stored())
        )
    }

    #[cfg(test)]
    pub(crate) fn set(&mut self, on: bool, setting: f64) {
        (self.on, self.setting) = (on, setting);
    }

    #[cfg(test)]
    pub(crate) fn train(&self) -> (&Drivetrain, &Circuit, Node, usize) {
        (&self.trains[0], &self.circuit, self.output, self.lossy)
    }
}

#[cfg(test)]
#[path = "drive_bench_tests.rs"]
mod tests;
