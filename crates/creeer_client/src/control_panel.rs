//! F6.7 control panel: a Lua microcontroller driving a machine, its script edited live. Two
//! rigs: a tank whose level a script holds with its pump (the F6.6 pipes) while the taps open
//! and close, and a motor whose speed a script holds with its supply's voltage (the F6.3
//! circuit and the F6.4 drivetrain) under a load.

use crate::control_rigs::{FLOOR, Machine, motor, pi, tank};
use creeer_fluids::pipes::LinkKind;
use creeer_logic::{BlockId, Kind, Limits, Network, Script};
use creeer_math::glam::DVec3;
use creeer_sim::controls::{Controls, Plant, Probe, Setting};
use creeer_sim::drive;
use creeer_ui::egui;
use std::collections::VecDeque;

const TICK: f64 = 1.0 / 60.0;
/// Seconds of trace kept.
const WINDOW: f64 = 60.0;
/// The rigs on offer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Rig {
    Tank,
    Motor,
}

/// The panel.
pub struct ControlPanel {
    rig: Rig,
    machine: Machine,
    controls: Controls,
    setpoint_sensor: BlockId,
    script: BlockId,
    actuator: BlockId,
    /// The script being edited, the setpoint and the disturbance (taps' opening or load, N·m).
    source: String,
    setpoint: f64,
    disturbance: f64,
    time: f64,
    carry: f64,
    traces: [(&'static str, VecDeque<[f64; 2]>); 3],
}

impl std::fmt::Debug for ControlPanel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ControlPanel")
            .field("rig", &self.rig)
            .field("time", &self.time)
            .finish_non_exhaustive()
    }
}

impl ControlPanel {
    /// The tank rig.
    pub fn new() -> Self {
        Self::build(Rig::Tank, None)
    }

    fn build(rig: Rig, source: Option<String>) -> Self {
        let (machine, probe, setting, source, setpoint, disturbance, names) = match rig {
            Rig::Tank => {
                let (machine, pump) = tank();
                let Machine::Tank { tank, .. } = machine else {
                    unreachable!("a tank")
                };
                let source = source.unwrap_or_else(|| pi(4.0, 0.2, (0.0, 1.2), 0.8));
                let names = ["level (m)", "setpoint (m)", "pump speed"];
                (
                    machine,
                    Probe::Head(tank),
                    Setting::PumpSpeed(pump),
                    source,
                    2.5,
                    0.3,
                    names,
                )
            }
            Rig::Motor => {
                let (machine, supply) = motor();
                let Machine::Motor { rotor, .. } = machine else {
                    unreachable!("a motor")
                };
                let source = source.unwrap_or_else(|| pi(0.05, 1.0, (0.0, 24.0), 0.0));
                let names = ["speed (rad/s)", "setpoint (rad/s)", "supply (V)"];
                let setting = Setting::Source(supply, 0.0, 24.0);
                (
                    machine,
                    Probe::Speed(0, rotor),
                    setting,
                    source,
                    150.0,
                    0.0,
                    names,
                )
            }
        };
        let mut net = Network::new();
        let measured = net.add(Kind::Sensor);
        let setpoint_sensor = net.add(Kind::Sensor);
        let script = net.add_script(Script::new(&source, 2, 1, Limits::default(), 1));
        let actuator = net.add(Kind::Actuator);
        net.wire((measured, 0), script, 0);
        net.wire((setpoint_sensor, 0), script, 1);
        net.wire((script, 0), actuator, 0);
        let mut controls = Controls::new(net);
        controls.sense(measured, probe);
        controls.actuate(actuator, setting);
        Self {
            rig,
            machine,
            controls,
            setpoint_sensor,
            script,
            actuator,
            source,
            setpoint,
            disturbance,
            time: 0.0,
            carry: 0.0,
            traces: names.map(|n| (n, VecDeque::new())),
        }
    }

    /// Steps the rig by `real` seconds of real time (at most a quarter of a second).
    pub fn advance(&mut self, real: f64) {
        self.carry += real.min(0.25);
        while self.carry >= TICK {
            self.carry -= TICK;
            self.tick();
        }
    }

    fn tick(&mut self) {
        self.time += TICK;
        let (measured, target) = match &mut self.machine {
            Machine::Tank { pipes, tank, valve } => {
                if let LinkKind::Valve { opening, .. } = &mut pipes.links[*valve].kind {
                    *opening = self.disturbance;
                }
                let target = FLOOR + self.setpoint;
                self.controls.logic.set_sensor(self.setpoint_sensor, target);
                let mut plant = Plant {
                    pipes: Some(&mut **pipes),
                    ..Plant::default()
                };
                self.controls.tick(&mut plant, TICK);
                pipes.tick(TICK);
                (pipes.nodes[*tank].head - FLOOR, self.setpoint)
            }
            Machine::Motor {
                circuit,
                trains,
                link,
                rotor,
                wheel,
            } => {
                // The wheel turns backwards (the gearbox reverses): the load acts forwards.
                trains[0].set_load(*wheel, self.disturbance);
                self.controls
                    .logic
                    .set_sensor(self.setpoint_sensor, self.setpoint);
                let mut plant = Plant {
                    circuit: Some(&mut **circuit),
                    drivetrains: Some(trains.as_mut_slice()),
                    ..Plant::default()
                };
                self.controls.tick(&mut plant, TICK);
                drive::step(circuit, trains, &[*link], TICK, &[DVec3::ZERO]);
                (trains[0].speed(*rotor), self.setpoint)
            }
        };
        let setting = self.controls.logic.value((self.actuator, 0)).number();
        let t = self.time;
        for ((_, points), v) in self.traces.iter_mut().zip([measured, target, setting]) {
            points.push_back([t, v]);
            while points.front().is_some_and(|p| p[0] < t - WINDOW) {
                points.pop_front();
            }
        }
    }

    /// The panel's window.
    pub fn window(&mut self, ctx: &egui::Context) {
        egui::Window::new("Control panel (F6.7)")
            .default_pos([60.0, 600.0])
            .default_open(false)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    for (rig, name) in [(Rig::Tank, "tank level"), (Rig::Motor, "motor speed")] {
                        if ui.radio(self.rig == rig, name).clicked() && self.rig != rig {
                            *self = Self::build(rig, None);
                        }
                    }
                    if ui.button("restart").clicked() {
                        *self = Self::build(self.rig, Some(self.source.clone()));
                    }
                });
                ui.horizontal(|ui| match self.rig {
                    Rig::Tank => {
                        ui.add(egui::Slider::new(&mut self.setpoint, 0.5..=4.0).text("level (m)"));
                        ui.add(egui::Slider::new(&mut self.disturbance, 0.0..=1.0).text("taps"));
                    }
                    Rig::Motor => {
                        ui.add(egui::Slider::new(&mut self.setpoint, 0.0..=250.0).text("speed"));
                        ui.add(
                            egui::Slider::new(&mut self.disturbance, 0.0..=20.0).text("load (N·m)"),
                        );
                    }
                });
                self.plot(ui);
                ui.add(
                    egui::TextEdit::multiline(&mut self.source)
                        .code_editor()
                        .desired_rows(10)
                        .desired_width(f32::INFINITY),
                );
                ui.horizontal(|ui| {
                    if ui.button("upload (keeps memory)").clicked() {
                        self.upload();
                    }
                    ui.label(self.status());
                });
            });
    }

    /// Starts the edited script in place of the running one, its `memory` kept.
    fn upload(&mut self) {
        if let Some(old) = self.controls.logic.script(self.script) {
            let new = old.restarted(&self.source);
            self.controls.logic.replace_script(self.script, new);
        }
    }

    fn plot(&self, ui: &mut egui::Ui) {
        use creeer_ui::egui_plot::{Legend, Line, Plot, PlotPoints};
        Plot::new("control panel")
            .height(200.0)
            .legend(Legend::default())
            .x_axis_label("seconds")
            .show(ui, |plot| {
                for (name, points) in &self.traces {
                    let points: PlotPoints<'_> = points.iter().copied().collect();
                    plot.line(Line::new(*name, points));
                }
            });
    }

    /// Whether the script runs, its memory, and the last line it printed.
    pub fn status(&self) -> String {
        let Some(script) = self.controls.logic.script(self.script) else {
            return String::new();
        };
        let state = script
            .halted()
            .map_or_else(|| "running".to_owned(), |why| format!("stopped: {why}"));
        let log = script.log();
        format!(
            "t = {:.0} s · {state} · {:.1} kB of {} kB · {}",
            self.time,
            script.memory_used() as f64 / 1024.0,
            script.limits().memory / 1024,
            log.last().map_or("", String::as_str)
        )
    }

    #[cfg(test)]
    pub(crate) fn last(&self) -> (f64, f64) {
        let at = |k: usize| self.traces[k].1.back().map_or(f64::NAN, |p| p[1]);
        (at(0), at(2))
    }

    #[cfg(test)]
    pub(crate) fn edit_and_upload(&mut self, source: &str) {
        self.source = source.to_owned();
        self.upload();
    }

    #[cfg(test)]
    pub(crate) fn switch(&mut self, motor: bool) {
        *self = Self::build(if motor { Rig::Motor } else { Rig::Tank }, None);
    }
}

#[cfg(test)]
#[path = "control_panel_tests.rs"]
mod tests;
