//! The lab's plotted tabs: the relaxing reaction, the Ellingham diagram, the furnace.

use crate::lab_runs::{Furnace, Relaxation};
use creeer_ui::egui;
use creeer_ui::egui_plot::{HLine, Legend, Line, Plot, PlotPoints};

pub fn kinetics(ui: &mut egui::Ui, r: &mut Relaxation) {
    ui.label(
        "N₂O₄ ⇌ 2 NO₂ at 1 atm (its rate slowed to be watched; where it settles is the data's)",
    );
    ui.add(egui::Slider::new(&mut r.t, 250.0..=420.0).text("K"));
    Plot::new("relaxation")
        .height(220.0)
        .legend(Legend::default())
        .x_axis_label("seconds")
        .y_axis_label("NO₂ share of the gas")
        .show(ui, |plot| {
            let points: PlotPoints<'_> = r.trace.iter().copied().collect();
            plot.line(Line::new("NO₂", points));
            if r.equilibrium.is_finite() {
                plot.hline(HLine::new("equilibrium", r.equilibrium));
            }
        });
}

pub fn ellingham(ui: &mut egui::Ui, lines: &[(&'static str, Vec<[f64; 2]>)]) {
    ui.label("ΔG° of forming each oxide per mole of O₂ (kJ), each substance in its stable phase: a lower line's element takes oxygen from a higher line's oxide");
    Plot::new("ellingham")
        .height(320.0)
        .legend(Legend::default())
        .x_axis_label("°C")
        .y_axis_label("kJ per mol O₂")
        .show(ui, |plot| {
            for (name, points) in lines {
                let points: PlotPoints<'_> = points.iter().copied().collect();
                plot.line(Line::new(*name, points));
            }
        });
}

pub fn furnace(ui: &mut egui::Ui, f: &mut Furnace) {
    ui.label("2 mol of hematite, carbon monoxide blown through (each parcel leaving in equilibrium with the bed)");
    ui.add(egui::Slider::new(&mut f.t, 800.0..=1400.0).text("K"));
    let names = ["Fe₂O₃", "Fe₃O₄", "FeO", "Fe"];
    Plot::new("furnace solids")
        .height(180.0)
        .legend(Legend::default())
        .x_axis_label("seconds")
        .y_axis_label("mol")
        .show(ui, |plot| {
            for (name, trace) in names.iter().zip(&f.solids) {
                let points: PlotPoints<'_> = trace.iter().copied().collect();
                plot.line(Line::new(*name, points));
            }
        });
    Plot::new("furnace gas")
        .height(120.0)
        .legend(Legend::default())
        .x_axis_label("seconds")
        .y_axis_label("CO in the off-gas (%)")
        .show(ui, |plot| {
            let points: PlotPoints<'_> = f.off_gas.iter().copied().collect();
            plot.line(Line::new("CO", points));
        });
}
