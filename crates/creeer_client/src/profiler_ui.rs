//! The profiler window (F9; F7.6, ADR-0021): the last ticks' lengths, and one tick's spans
//! as a flame graph with where its time went, of this process or of the server the client
//! is connected to (its reports, a second apart: `creeer_core::profile`).

use creeer_core::profile::{self, Frame, Want};
use creeer_net::message::ProfileReport;
use creeer_server::Connection;
use creeer_ui::egui;
use std::collections::VecDeque;

/// A tick at 60 Hz (ms): the line the bars are measured against.
const TICK_MS: f32 = 1000.0 / 60.0;
/// Frames the worst is looked for among by default: five seconds.
const RECENT: usize = 300;
/// A row of the flame graph (points).
const ROW: f32 = 16.0;

/// The window's state.
#[derive(Debug, Default)]
pub(crate) struct ProfilerWindow {
    pub open: bool,
    /// Held while open: scopes record.
    want: Option<Want>,
    /// Showing the server's profile (else this process's).
    server: bool,
    /// Whether the server was asked for its reports.
    asked: bool,
    /// The tick of this process's frame chosen (else the worst of the last five seconds).
    chosen: Option<u64>,
    /// The server's tick lengths (µs) from its reports, and the last report taken in.
    server_ticks: VecDeque<u32>,
    last_report: Option<ProfileReport>,
}

impl ProfilerWindow {
    /// Opens or closes it.
    pub fn toggle(&mut self) {
        self.open = !self.open;
    }

    /// Draws it (when open), `connection` the server's if connected.
    pub fn window(&mut self, ctx: &egui::Context, mut connection: Option<&mut Connection>) {
        self.want = if self.open {
            self.want.take().or_else(|| Some(profile::want()))
        } else {
            None
        };
        let watching = self.open && self.server && connection.is_some();
        if let Some(c) = connection.as_deref_mut() {
            if watching != self.asked {
                c.watch_profile(watching);
                self.asked = watching;
            }
            self.take_report(c.profile.as_ref());
        } else {
            self.asked = false;
        }
        if !self.open {
            return;
        }
        let mut open = self.open;
        egui::Window::new("Profiler")
            .open(&mut open)
            .default_pos([380.0, 60.0])
            .default_width(720.0)
            .show(ctx, |ui| self.contents(ui, connection.is_some()));
        self.open = open;
    }

    fn take_report(&mut self, report: Option<&ProfileReport>) {
        let Some(report) = report else {
            return;
        };
        if self.last_report.as_ref() == Some(report) {
            return;
        }
        self.server_ticks.extend(&report.durations);
        while self.server_ticks.len() > profile::KEPT {
            self.server_ticks.pop_front();
        }
        self.last_report = Some(report.clone());
    }

    fn contents(&mut self, ui: &mut egui::Ui, connected: bool) {
        ui.horizontal(|ui| {
            ui.radio_value(&mut self.server, false, "this process");
            ui.add_enabled_ui(connected, |ui| {
                ui.radio_value(&mut self.server, true, "the server");
            });
        });
        let (lengths, frame, note) = if self.server && connected {
            let lengths: Vec<f32> = self
                .server_ticks
                .iter()
                .map(|us| *us as f32 / 1000.0)
                .collect();
            let frame = self.last_report.as_ref().map(|r| r.worst.clone());
            (
                lengths,
                frame,
                "the longest tick of the server's last report",
            )
        } else {
            let frames = profile::frames();
            let lengths = frames.iter().map(|f| f.duration as f32 / 1e6).collect();
            let chosen = self
                .chosen
                .and_then(|t| frames.iter().find(|f| f.tick == t));
            let frame = chosen
                .or_else(|| frames.iter().rev().take(RECENT).max_by_key(|f| f.duration))
                .cloned();
            let note = if chosen.is_some() {
                "the tick chosen (click a bar; right-click: back to the worst)"
            } else {
                "the longest tick of the last five seconds (click a bar to choose one)"
            };
            (lengths, frame, note)
        };
        if let Some(picked) = bars(ui, &lengths) {
            if self.server {
                // The server's reports carry their longest tick only.
            } else if let Some(i) = picked {
                self.chosen = profile::frames().get(i).map(|f| f.tick);
            } else {
                self.chosen = None;
            }
        }
        let Some(frame) = frame else {
            ui.label("No ticks profiled yet.");
            return;
        };
        ui.label(format!(
            "Tick {}: {:.2} ms, {note}.",
            frame.tick,
            frame.duration as f64 / 1e6
        ));
        flame(ui, &frame);
        totals(ui, &frame);
    }
}

/// The ticks' lengths as bars against a 60 Hz tick; a click gives the bar's index, a
/// right-click `Some(None)`.
fn bars(ui: &mut egui::Ui, lengths: &[f32]) -> Option<Option<usize>> {
    let width = ui.available_width();
    let (rect, response) = ui.allocate_exact_size(egui::vec2(width, 60.0), egui::Sense::click());
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, 2.0, ui.visuals().extreme_bg_color);
    let top = lengths.iter().copied().fold(2.0 * TICK_MS, f32::max);
    let y = |ms: f32| rect.bottom() - rect.height() * (ms / top).min(1.0);
    let step = width / profile::KEPT as f32;
    let first = profile::KEPT.saturating_sub(lengths.len());
    for (i, ms) in lengths.iter().enumerate() {
        let x = rect.left() + (first + i) as f32 * step;
        let colour = if *ms > TICK_MS {
            egui::Color32::from_rgb(230, 90, 70)
        } else {
            egui::Color32::from_rgb(90, 170, 110)
        };
        let bar = egui::Rect::from_min_max(
            egui::pos2(x, y(*ms)),
            egui::pos2(x + step.max(1.0), rect.bottom()),
        );
        painter.rect_filled(bar, 0.0, colour);
    }
    painter.hline(
        rect.x_range(),
        y(TICK_MS),
        (1.0, ui.visuals().weak_text_color()),
    );
    if response.secondary_clicked() {
        return Some(None);
    }
    let at = response
        .interact_pointer_pos()
        .filter(|_| response.clicked())?;
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)] // within the strip
    let slot = ((at.x - rect.left()) / step) as usize;
    Some(slot.checked_sub(first).filter(|i| *i < lengths.len()))
}

/// The frame's spans: a band per thread, a row per depth, each span as wide as it took.
fn flame(ui: &mut egui::Ui, frame: &Frame) {
    let depth = |t: u16| {
        frame
            .spans
            .iter()
            .filter(|s| s.thread == t)
            .map(|s| s.depth)
            .max()
            .map_or(0, |d| d + 1)
    };
    let threads: Vec<(u16, u8)> = (0..frame.threads.len())
        .filter_map(|t| u16::try_from(t).ok())
        .map(|t| (t, depth(t)))
        .filter(|(_, d)| *d > 0)
        .collect();
    let rows: f32 = threads.iter().map(|(_, d)| f32::from(*d) + 1.0).sum();
    let width = ui.available_width();
    let (rect, response) =
        ui.allocate_exact_size(egui::vec2(width, rows * ROW), egui::Sense::hover());
    let painter = ui.painter_at(rect);
    let length = frame.duration.max(1) as f32;
    let mut top = rect.top();
    let mut hovered = None;
    for (t, d) in threads {
        painter.text(
            egui::pos2(rect.left() + 2.0, top + 1.0),
            egui::Align2::LEFT_TOP,
            &frame.threads[usize::from(t)],
            egui::FontId::proportional(11.0),
            ui.visuals().strong_text_color(),
        );
        top += ROW;
        for s in frame.spans.iter().filter(|s| s.thread == t) {
            let x0 = rect.left() + width * s.start as f32 / length;
            let x1 = (rect.left() + width * s.end as f32 / length).max(x0 + 1.0);
            let y0 = top + f32::from(s.depth) * ROW;
            let span = egui::Rect::from_min_max(egui::pos2(x0, y0), egui::pos2(x1, y0 + ROW - 1.0));
            let name = &frame.names[usize::from(s.name)];
            painter.rect_filled(span, 1.0, colour(name));
            if span.width() > 40.0 {
                painter.text(
                    span.left_center() + egui::vec2(2.0, 0.0),
                    egui::Align2::LEFT_CENTER,
                    name,
                    egui::FontId::proportional(10.0),
                    egui::Color32::BLACK,
                );
            }
            if response.hover_pos().is_some_and(|p| span.contains(p)) {
                hovered = Some((name.clone(), s.end - s.start));
            }
        }
        top += f32::from(d) * ROW;
    }
    if let Some((name, ns)) = hovered {
        response.on_hover_text(format!("{name}: {:.3} ms", ns as f64 / 1e6));
    }
}

/// Where the frame's time went, by name (most first).
fn totals(ui: &mut egui::Ui, frame: &Frame) {
    egui::Grid::new("profile totals")
        .striped(true)
        .show(ui, |ui| {
            for (name, ns) in frame.totals().into_iter().take(14) {
                ui.label(name);
                ui.label(format!("{:.3} ms", ns as f64 / 1e6));
                ui.label(format!(
                    "{:.0} %",
                    100.0 * ns as f64 / frame.duration.max(1) as f64
                ));
                ui.end_row();
            }
        });
}

/// A name's colour: the same every frame.
fn colour(name: &str) -> egui::Color32 {
    let h = name.bytes().fold(0x811c_9dc5u32, |h, b| {
        (h ^ u32::from(b)).wrapping_mul(0x0100_0193)
    });
    let [r, g, b, _] = h.to_le_bytes();
    egui::Color32::from_rgb(140 + r / 3, 140 + g / 3, 140 + b / 3)
}
