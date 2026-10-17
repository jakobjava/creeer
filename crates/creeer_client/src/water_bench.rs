//! F6.6 pipe bench: a waterworks (a pump lifting well water to a tank, a valve letting it
//! out to the taps, dye that can be put in the well to watch it go through the pipes and
//! mix in the tank) and a gas transmission line (its line pack holding up the city's demand
//! while the supply catches up). Every network at the rate it picks (ADR-0014).

use creeer_fluids::pipes::content::Tracer;
use creeer_fluids::pipes::gas::{Gas, GasKind, GasNetwork};
use creeer_fluids::pipes::{LinkKind, Liquid, Network, NodeKind};
use creeer_ui::egui;
use std::collections::VecDeque;

const TICK: f64 = 1.0 / 60.0;
/// Seconds of trace kept.
const WINDOW: f64 = 120.0;
const BAR: f64 = 1e5;

/// The rigs on offer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Rig {
    Waterworks,
    GasLine,
}

impl Rig {
    const ALL: [(Self, &'static str); 2] = [
        (Self::Waterworks, "waterworks"),
        (Self::GasLine, "gas transmission line"),
    ];
}

/// The waterworks: its network, the dye it carries, its parts.
struct Works {
    net: Network,
    dye: Tracer,
    well: usize,
    tank: usize,
    taps: usize,
    pump: usize,
    valve: usize,
}

/// The gas line: its network, its supply and its city gate.
struct Line {
    net: GasNetwork,
    gate: usize,
}

/// The bench.
pub struct WaterBench {
    rig: Rig,
    works: Works,
    line: Line,
    time: f64,
    carry: f64,
    traces: Vec<(&'static str, VecDeque<[f64; 2]>)>,
    /// The controls: pump speed and valve opening (0–1), dye in the well; the city's demand.
    pub speed: f64,
    pub opening: f64,
    pub dyed: bool,
    pub demand: f64,
}

impl std::fmt::Debug for WaterBench {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WaterBench")
            .field("rig", &self.rig)
            .field("time", &self.time)
            .finish_non_exhaustive()
    }
}

fn pipe(length: f64, diameter: f64) -> LinkKind {
    LinkKind::Pipe {
        length,
        diameter,
        roughness: 5e-5,
        minor: 2.0,
        check: false,
    }
}

fn works() -> Works {
    let mut net = Network::new(Liquid::WATER_20C, 9.806_65);
    let well = net.node(-5.0, NodeKind::Reservoir, -5.0);
    let riser = net.node(0.0, NodeKind::Junction { demand: 0.0 }, 0.0);
    let tank = net.node(
        20.0,
        NodeKind::Tank {
            area: 20.0,
            floor: 20.0,
        },
        22.0,
    );
    let down = net.node(0.0, NodeKind::Junction { demand: 0.0 }, 0.0);
    let taps = net.node(0.0, NodeKind::Reservoir, 0.0);
    let pump = net.link(
        well,
        riser,
        LinkKind::Pump {
            shutoff: 45.0,
            resistance: 1.5e4,
            speed: 1.0,
            best_flow: 0.03,
            best_efficiency: 0.75,
        },
    );
    net.link(riser, tank, pipe(80.0, 0.1));
    net.link(tank, down, pipe(300.0, 0.1));
    let valve = net.link(
        down,
        taps,
        LinkKind::Valve {
            diameter: 0.1,
            open: 0.2,
            opening: 0.5,
        },
    );
    let dye = net.tracer(&[0.0]);
    Works {
        net,
        dye,
        well,
        tank,
        taps,
        pump,
        valve,
    }
}

fn line() -> Line {
    let mut net = GasNetwork::new(Gas::METHANE_15C);
    let (d, l, sections) = (0.3, 50_000.0, 10);
    let volume = 0.25 * std::f64::consts::PI * d * d * l / f64::from(sections);
    let mut last = net.node(GasKind::Supply, 60.0 * BAR);
    let mut gate = last;
    for k in 1..=sections {
        let p = (60.0 - 2.0 * f64::from(k)) * BAR;
        let store = net.node(
            GasKind::Store {
                volume,
                demand: 0.0,
            },
            p,
        );
        net.pipe(last, store, l / f64::from(sections), d, 2e-5, None);
        (last, gate) = (store, store);
    }
    Line { net, gate }
}

impl WaterBench {
    /// The waterworks rig.
    pub fn new() -> Self {
        Self::build(Rig::Waterworks)
    }

    fn build(rig: Rig) -> Self {
        let names: &[&'static str] = match rig {
            Rig::Waterworks => &[
                "tank level (m)",
                "pumped (l/s)",
                "to the taps (l/s)",
                "dye at the taps (%)",
                "pump power (kW)",
            ],
            Rig::GasLine => &[
                "supplied (kg/s)",
                "drawn at the gate (kg/s)",
                "gate pressure (bar)",
                "line pack (t)",
            ],
        };
        Self {
            rig,
            works: works(),
            line: line(),
            time: 0.0,
            carry: 0.0,
            traces: names.iter().map(|&n| (n, VecDeque::new())).collect(),
            speed: 1.0,
            opening: 0.5,
            dyed: false,
            demand: 20.0,
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

    pub(crate) fn tick(&mut self) {
        self.time += TICK;
        let values: Vec<f64> = match self.rig {
            Rig::Waterworks => self.tick_works(),
            Rig::GasLine => self.tick_line(),
        };
        let t = self.time;
        for ((_, points), v) in self.traces.iter_mut().zip(values) {
            points.push_back([t, v]);
            while points.front().is_some_and(|p| p[0] < t - WINDOW) {
                points.pop_front();
            }
        }
    }

    fn tick_works(&mut self) -> Vec<f64> {
        let w = &mut self.works;
        if let LinkKind::Pump { speed, .. } = &mut w.net.links[w.pump].kind {
            *speed = self.speed;
        }
        if let LinkKind::Valve { opening, .. } = &mut w.net.links[w.valve].kind {
            *opening = self.opening;
        }
        w.dye.set(w.well, &[if self.dyed { 1.0 } else { 0.0 }]);
        if let Some((_, h)) = w.net.tick(TICK) {
            w.net.carry(&mut w.dye, h);
        }
        let out = w.net.links[w.valve].flow;
        vec![
            w.net.nodes[w.tank].head - 20.0,
            w.net.links[w.pump].flow * 1e3,
            out * 1e3,
            100.0 * w.dye.delivered(w.valve, w.taps)[0],
            w.net.shaft_power(w.pump) * 1e-3,
        ]
    }

    fn tick_line(&mut self) -> Vec<f64> {
        let l = &mut self.line;
        if let GasKind::Store { volume, .. } = l.net.nodes[l.gate].kind {
            l.net.nodes[l.gate].kind = GasKind::Store {
                volume,
                demand: self.demand,
            };
        }
        l.net.step(TICK);
        vec![
            l.net.pipes[0].flow,
            self.demand,
            l.net.nodes[l.gate].pressure / BAR,
            l.net.stored() * 1e-3,
        ]
    }

    /// The bench's window.
    pub fn window(&mut self, ctx: &egui::Context) {
        egui::Window::new("Pipe bench (F6.6)")
            .default_pos([900.0, 600.0])
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
                    match self.rig {
                        Rig::Waterworks => {
                            ui.add(
                                egui::Slider::new(&mut self.speed, 0.0..=1.2).text("pump speed"),
                            );
                            ui.add(egui::Slider::new(&mut self.opening, 0.0..=1.0).text("valve"));
                            ui.checkbox(&mut self.dyed, "dye in the well");
                        }
                        Rig::GasLine => {
                            ui.add(
                                egui::Slider::new(&mut self.demand, 0.0..=60.0)
                                    .text("city demand (kg/s)"),
                            );
                        }
                    }
                    if ui.button("restart").clicked() {
                        *self = Self::build(self.rig);
                    }
                });
                self.plot(ui);
                ui.label(self.status());
            });
    }

    fn plot(&self, ui: &mut egui::Ui) {
        use creeer_ui::egui_plot::{Legend, Line as PlotLine, Plot, PlotPoints};
        Plot::new("pipe bench")
            .height(220.0)
            .legend(Legend::default())
            .x_axis_label("seconds")
            .show(ui, |plot| {
                for (name, points) in &self.traces {
                    let points: PlotPoints<'_> = points.iter().copied().collect();
                    plot.line(PlotLine::new(*name, points));
                }
            });
    }

    /// The network's rate and where its energy (or mass) went.
    pub fn status(&mut self) -> String {
        match self.rig {
            Rig::Waterworks => {
                let w = &mut self.works;
                let every = w.net.ticks_per_step();
                let (_, heat, out, pumped) = w.net.ledger().totals();
                format!(
                    "t = {:.0} s · steps at {} Hz · pumps and the well gave {:.1} kJ, friction heat {:.1} kJ, water's energy out {:.1} kJ, the tank holds {:.1} kJ · unaccounted {:.1e} (energy), {:.1e} (dye)",
                    self.time,
                    60 / every,
                    pumped * 1e-3,
                    heat * 1e-3,
                    out * 1e-3,
                    w.net.stored() * 1e-3,
                    w.net.ledger().relative_imbalance(w.net.stored()),
                    w.dye.ledger(1).relative_imbalance(w.dye.held()[1]),
                )
            }
            Rig::GasLine => {
                let n = &self.line.net;
                let (_, _, drawn, supplied) = n.ledger().totals();
                format!(
                    "t = {:.0} s · supplied {:.1} t, drawn {:.1} t, line pack {:.1} t · unaccounted {:.1e}",
                    self.time,
                    supplied * 1e-3,
                    drawn * 1e-3,
                    n.stored() * 1e-3,
                    n.ledger().relative_imbalance(n.stored())
                )
            }
        }
    }

    #[cfg(test)]
    pub(crate) fn works(&self) -> (&Network, &Tracer, usize) {
        (&self.works.net, &self.works.dye, self.works.tank)
    }
}

#[cfg(test)]
#[path = "water_bench_tests.rs"]
mod tests;
