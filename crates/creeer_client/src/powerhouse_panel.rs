//! F6.8 powerhouse panel: one machine through every domain (`creeer_sim::powerhouse`). Water
//! through a turbine turns a generator that splits water into hydrogen; a Lua script holds the
//! shaft's speed with the valve; everything warms, the platform turns against the shaft, and
//! the energy books show where every joule of the water's went.

use creeer_sim::powerhouse::{DT, Powerhouse, Readings};
use creeer_ui::egui;
use std::collections::VecDeque;

/// Seconds of trace kept.
const WINDOW: f64 = 300.0;
/// The plots: their titles and their lines' names.
const PLOTS: [(&str, &[&str]); 3] = [
    ("shaft", &["speed (rad/s)", "setpoint (rad/s)", "valve (%)"]),
    ("power", &["turbine (kW)", "stack (kW)"]),
    (
        "heat",
        &["generator (°C)", "frame (°C)", "coolant (°C)", "stack (°C)"],
    ),
];

/// The panel.
pub struct PowerhousePanel {
    house: Box<Powerhouse>,
    /// How many times real time it runs.
    warp: u32,
    carry: f64,
    traces: Vec<Vec<VecDeque<[f64; 2]>>>,
}

impl std::fmt::Debug for PowerhousePanel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PowerhousePanel")
            .field("time", &self.house.time)
            .finish_non_exhaustive()
    }
}

impl PowerhousePanel {
    pub fn new() -> Self {
        Self {
            house: Box::new(Powerhouse::new()),
            warp: 1,
            carry: 0.0,
            traces: PLOTS
                .iter()
                .map(|(_, names)| names.iter().map(|_| VecDeque::new()).collect())
                .collect(),
        }
    }

    /// Steps it by `real` seconds of real time (at most a quarter of a second) times its warp.
    pub fn advance(&mut self, real: f64) {
        self.carry += real.min(0.25) * f64::from(self.warp);
        while self.carry >= DT {
            self.carry -= DT;
            self.tick();
        }
    }

    pub(crate) fn tick(&mut self) {
        self.house.tick();
        let r = self.house.readings();
        let t = self.house.time;
        let celsius = |k: f64| k - 273.15;
        let values: [Vec<f64>; 3] = [
            vec![r.speed, self.house.speed, 100.0 * r.valve],
            vec![r.turbine / 1e3, r.stack / 1e3],
            r.temperatures.iter().map(|&k| celsius(k)).collect(),
        ];
        for (lines, values) in self.traces.iter_mut().zip(values) {
            for (points, v) in lines.iter_mut().zip(values) {
                points.push_back([t, v]);
                while points.front().is_some_and(|p| p[0] < t - WINDOW) {
                    points.pop_front();
                }
            }
        }
    }

    /// The panel's window.
    pub fn window(&mut self, ctx: &egui::Context) {
        egui::Window::new("Powerhouse (F6.8)")
            .default_pos([700.0, 600.0])
            .default_open(false)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.add(
                        egui::Slider::new(&mut self.house.speed, 100.0..=180.0)
                            .text("shaft setpoint (rad/s)"),
                    );
                    for warp in [1, 10, 60] {
                        ui.radio_value(&mut self.warp, warp, format!("{warp}×"));
                    }
                    if ui.button("restart").clicked() {
                        *self = Self::new();
                    }
                });
                let r = self.house.readings();
                ui.label(Self::state(&r, self.house.time));
                for ((title, names), lines) in PLOTS.iter().zip(&self.traces) {
                    Self::plot(ui, title, names, lines);
                }
                ui.label(self.books());
            });
    }

    fn state(r: &Readings, time: f64) -> String {
        format!(
            "t = {time:.0} s · tank {:.2} m · {:.1} L/s · {:.0} A · H₂ {:.3} mol · platform {:.4} rad/s",
            r.level,
            1e3 * r.flow,
            r.current,
            r.hydrogen,
            r.spin
        )
    }

    /// Where the water's energy went, in kJ and shares.
    pub(crate) fn books(&self) -> String {
        let (gave, went) = self.house.books();
        let names = [
            "water friction",
            "shaft and circuit",
            "reactor (H₂)",
            "heat held",
            "heat to air",
            "unrouted",
        ];
        let share = |j: f64| if gave > 0.0 { 100.0 * j / gave } else { 0.0 };
        let mut text = format!("the water gave {:.1} kJ:", gave / 1e3);
        for (name, j) in names.iter().zip(went) {
            text += &format!(" {name} {:.1} %,", share(j));
        }
        let rest = gave - went.iter().sum::<f64>();
        text + &format!(" unaccounted {rest:.2e} J")
    }

    fn plot(ui: &mut egui::Ui, title: &str, names: &[&str], lines: &[VecDeque<[f64; 2]>]) {
        use creeer_ui::egui_plot::{Legend, Line, Plot, PlotPoints};
        Plot::new(format!("powerhouse {title}"))
            .height(120.0)
            .legend(Legend::default())
            .show(ui, |plot| {
                for (name, points) in names.iter().zip(lines) {
                    let points: PlotPoints<'_> = points.iter().copied().collect();
                    plot.line(Line::new(*name, points));
                }
            });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A minute in, the shaft is at its setpoint and the books close.
    #[test]
    fn the_powerhouse_panel_runs_and_its_books_close() {
        let mut panel = PowerhousePanel::new();
        for _ in 0..60 * 60 {
            panel.tick();
        }
        let r = panel.house.readings();
        assert!((r.speed / 150.0 - 1.0).abs() < 1e-3, "{} rad/s", r.speed);
        let books = panel.books();
        assert!(books.contains("reactor (H₂)"), "{books}");
        let (gave, went) = panel.house.books();
        assert!((gave - went.iter().sum::<f64>()).abs() < 1e-9 * gave);
    }
}
