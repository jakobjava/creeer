//! Performance overlay: frame rate, frame/tick time percentiles and a live graph.

use creeer_core::TimingStats;
use egui::Color32;
use egui_plot::{HLine, Line, Plot, PlotPoints};

const FRAME_COLOR: Color32 = Color32::from_rgb(90, 170, 255);
const TICK_COLOR: Color32 = Color32::from_rgb(255, 170, 60);
const BUDGET_COLOR: Color32 = Color32::from_rgb(220, 70, 70);

/// Everything the performance window shows.
#[derive(Debug)]
pub struct PerfInfo<'a> {
    /// Render frame durations (time between presented frames).
    pub frame_times: &'a TimingStats,
    /// Simulation tick durations (time spent inside one tick).
    pub tick_times: &'a TimingStats,
    /// Simulation ticks per second.
    pub tick_rate_hz: u32,
    /// Current simulation tick.
    pub tick: u64,
    /// GPU name and graphics backend.
    pub adapter: &'a str,
}

/// Draws the performance window.
pub fn perf_window(ctx: &egui::Context, info: &PerfInfo<'_>) {
    egui::Window::new("Performance")
        .default_pos([12.0, 12.0])
        .resizable(false)
        .show(ctx, |ui| {
            let fps = info.frame_times.mean_ms().map_or(0.0, |ms| 1000.0 / ms);
            ui.label(format!("{fps:.0} FPS   ·   {}", info.adapter));
            ui.separator();

            egui::Grid::new("perf_grid").num_columns(4).show(ui, |ui| {
                ui.label("");
                ui.label("p50");
                ui.label("p99");
                ui.label("max");
                ui.end_row();
                stat_row(ui, "frame (ms)", info.frame_times, FRAME_COLOR);
                stat_row(ui, "tick (ms)", info.tick_times, TICK_COLOR);
            });

            let budget_ms = 1000.0 / f64::from(info.tick_rate_hz);
            ui.label(format!(
                "tick {} at {} Hz · budget {budget_ms:.1} ms",
                info.tick, info.tick_rate_hz
            ));

            Plot::new("frame_times")
                .height(110.0)
                .width(320.0)
                .allow_drag(false)
                .allow_zoom(false)
                .allow_scroll(false)
                .include_y(0.0)
                .include_y(budget_ms * 1.2)
                .y_axis_label("ms")
                .show(ui, |plot| {
                    plot.line(series("frame", info.frame_times).color(FRAME_COLOR));
                    plot.line(series("tick", info.tick_times).color(TICK_COLOR));
                    plot.hline(HLine::new("tick budget", budget_ms).color(BUDGET_COLOR));
                });
        });
}

fn stat_row(ui: &mut egui::Ui, label: &str, stats: &TimingStats, color: Color32) {
    ui.colored_label(color, label);
    for value in [
        stats.percentile_ms(50.0),
        stats.percentile_ms(99.0),
        stats.max_ms(),
    ] {
        ui.label(value.map_or_else(|| "-".to_owned(), |ms| format!("{ms:.2}")));
    }
    ui.end_row();
}

fn series<'a>(name: &str, stats: &TimingStats) -> Line<'a> {
    let points: PlotPoints<'a> = stats
        .samples_ms()
        .enumerate()
        .map(|(i, ms)| {
            #[allow(clippy::cast_precision_loss)] // a few hundred samples
            let x = i as f64;
            [x, ms]
        })
        .collect();
    Line::new(name, points)
}
