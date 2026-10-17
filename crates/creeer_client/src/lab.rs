//! F6.5 lab: chemistry from the species' data. Equilibria of chosen mixtures at a temperature
//! and pressure you set (or adiabatic: a flame's temperature), a reaction relaxing to its
//! equilibrium, the Ellingham diagram, a furnace reducing iron ore.

use crate::lab_runs::{Furnace, Relaxation};
use creeer_chem::{Fixed, System, equilibrate};
use creeer_materials::species::SpeciesTable;
use creeer_ui::egui;

const TICK: f64 = 0.1;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Tab {
    Equilibrium,
    Kinetics,
    Ellingham,
    Furnace,
}

/// A mixture to equilibrate: its name, its feed, whether every species its elements can form
/// is considered (flames) or a list, and its default conditions (K, bar, adiabatic).
struct Preset {
    name: &'static str,
    feed: &'static [(&'static str, f64)],
    species: Option<&'static [&'static str]>,
    t: f64,
    bar: f64,
    adiabatic: bool,
}

const PRESETS: [Preset; 5] = [
    Preset {
        name: "methane in air",
        feed: &[("CH4", 1.0), ("O2", 2.0), ("N2", 7.52)],
        species: None,
        t: 298.15,
        bar: 1.01325,
        adiabatic: true,
    },
    Preset {
        name: "hydrogen in oxygen",
        feed: &[("H2", 2.0), ("O2", 1.0)],
        species: None,
        t: 298.15,
        bar: 1.01325,
        adiabatic: true,
    },
    Preset {
        name: "limestone in a kiln",
        feed: &[("CaCO3(cr)", 1.0), ("N2", 0.01)],
        species: Some(&["CO2", "N2", "CaCO3(cr)", "CaO(cr)"]),
        t: 1150.0,
        bar: 1.01325,
        adiabatic: false,
    },
    Preset {
        name: "iron ore and CO",
        feed: &[("Fe2O3(cr)", 1.0), ("CO", 3.0), ("N2", 1.0)],
        species: Some(&[
            "CO",
            "CO2",
            "O2",
            "N2",
            "C(gr)",
            "Fe(a)",
            "Fe(c)",
            "Fe.947O(cr)",
            "Fe3O4(cr)",
            "Fe2O3(cr)",
        ]),
        t: 1100.0,
        bar: 1.01325,
        adiabatic: false,
    },
    Preset {
        name: "ammonia synthesis",
        feed: &[("N2", 1.0), ("H2", 3.0)],
        species: Some(&["N2", "H2", "NH3"]),
        t: 700.0,
        bar: 300.0,
        adiabatic: false,
    },
];

/// What the lab holds once opened.
struct Loaded {
    table: SpeciesTable,
    preset: usize,
    system: System,
    t: f64,
    bar: f64,
    adiabatic: bool,
    result: String,
    relaxation: Relaxation,
    furnace: Furnace,
    ellingham: Vec<(&'static str, Vec<[f64; 2]>)>,
}

/// The lab window.
pub struct Lab {
    tab: Tab,
    loaded: Option<Box<Loaded>>,
    carry: f64,
    open: bool,
}

impl std::fmt::Debug for Lab {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Lab")
            .field("tab", &self.tab)
            .finish_non_exhaustive()
    }
}

impl Lab {
    pub fn new() -> Self {
        Self {
            tab: Tab::Equilibrium,
            loaded: None,
            carry: 0.0,
            open: false,
        }
    }

    fn load() -> Box<Loaded> {
        let table = SpeciesTable::reference();
        let ellingham = crate::lab_ellingham::lines(&table);
        let relaxation = Relaxation::new(&table);
        let furnace = Furnace::new(&table);
        let mut loaded = Box::new(Loaded {
            system: System::new(&table, &["N2"]).expect("nitrogen"),
            table,
            preset: 0,
            t: 0.0,
            bar: 1.0,
            adiabatic: false,
            result: String::new(),
            relaxation,
            furnace,
            ellingham,
        });
        loaded.choose(0);
        loaded
    }

    /// Runs the timed experiments by `real` seconds while the window is open.
    pub fn advance(&mut self, real: f64) {
        let Some(l) = self.loaded.as_mut().filter(|_| self.open) else {
            return;
        };
        self.carry += real.min(0.25);
        while self.carry >= TICK {
            self.carry -= TICK;
            match self.tab {
                Tab::Kinetics => l.relaxation.tick(TICK),
                Tab::Furnace => l.furnace.tick(TICK),
                _ => {}
            }
        }
    }

    /// The lab's window.
    pub fn window(&mut self, ctx: &egui::Context) {
        let response = egui::Window::new("Lab (F6.5)")
            .default_pos([60.0, 340.0])
            .default_open(false)
            .show(ctx, |ui| {
                let l = self.loaded.get_or_insert_with(Self::load);
                ui.horizontal(|ui| {
                    for (tab, name) in [
                        (Tab::Equilibrium, "equilibrium"),
                        (Tab::Kinetics, "kinetics"),
                        (Tab::Ellingham, "Ellingham diagram"),
                        (Tab::Furnace, "furnace"),
                    ] {
                        ui.selectable_value(&mut self.tab, tab, name);
                    }
                });
                match self.tab {
                    Tab::Equilibrium => l.equilibrium(ui),
                    Tab::Kinetics => crate::lab_plots::kinetics(ui, &mut l.relaxation),
                    Tab::Ellingham => crate::lab_plots::ellingham(ui, &l.ellingham),
                    Tab::Furnace => crate::lab_plots::furnace(ui, &mut l.furnace),
                }
            });
        self.open = response.is_some_and(|r| r.inner.is_some());
    }
}

impl Loaded {
    fn choose(&mut self, preset: usize) {
        let p = &PRESETS[preset];
        let names: Vec<&str> = p.feed.iter().map(|f| f.0).collect();
        self.system = match p.species {
            Some(list) => System::new(&self.table, list),
            None => System::of_elements(&self.table, &names),
        }
        .expect("the preset's species");
        (self.preset, self.t, self.bar, self.adiabatic) = (preset, p.t, p.bar, p.adiabatic);
        self.solve();
    }

    fn solve(&mut self) {
        let sys = &self.system;
        let mut feed = vec![0.0; sys.species.len()];
        for (name, n) in PRESETS[self.preset].feed {
            feed[sys.index(name).expect("a feed species")] = *n;
        }
        let p = self.bar * 1e5;
        let fixed = if self.adiabatic {
            Fixed::Hp {
                h: sys.enthalpy(&feed, self.t),
                p,
            }
        } else {
            Fixed::Tp { t: self.t, p }
        };
        self.result = match equilibrate(sys, &feed, fixed, None) {
            Ok(eq) => {
                let gas: f64 = eq.moles[..sys.gases].iter().sum();
                let mut lines: Vec<(f64, String)> = (0..sys.gases)
                    .map(|j| {
                        (
                            eq.moles[j] / gas,
                            format!(
                                "{:<10} {:>9.5} %",
                                sys.species[j].name,
                                100.0 * eq.moles[j] / gas
                            ),
                        )
                    })
                    .collect();
                lines.sort_by(|a, b| b.0.total_cmp(&a.0));
                let mut text = format!(
                    "{:.1} K ({:.1} °C), {} Newton iterations\n\ngas, by mole:\n",
                    eq.t,
                    eq.t - 273.15,
                    eq.iterations
                );
                for (_, line) in lines.iter().take(10) {
                    text.push_str(line);
                    text.push('\n');
                }
                let solids: Vec<String> = (sys.gases..sys.species.len())
                    .filter(|&j| eq.moles[j] > 1e-9)
                    .map(|j| format!("{} {:.4} mol", sys.species[j].name, eq.moles[j]))
                    .collect();
                if !solids.is_empty() {
                    text.push_str(&format!("\ncondensed: {}", solids.join(", ")));
                }
                text
            }
            Err(e) => format!("no equilibrium: {e}"),
        };
    }

    fn equilibrium(&mut self, ui: &mut egui::Ui) {
        let mut changed = false;
        ui.horizontal(|ui| {
            for (k, p) in PRESETS.iter().enumerate() {
                if ui.radio(self.preset == k, p.name).clicked() {
                    self.choose(k);
                }
            }
        });
        ui.horizontal(|ui| {
            changed |= ui
                .checkbox(&mut self.adiabatic, "adiabatic (from this temperature)")
                .changed();
            changed |= ui
                .add(egui::Slider::new(&mut self.t, 250.0..=3000.0).text("K"))
                .changed();
            changed |= ui
                .add(
                    egui::Slider::new(&mut self.bar, 0.01..=300.0)
                        .logarithmic(true)
                        .text("bar"),
                )
                .changed();
        });
        if changed {
            self.solve();
        }
        ui.label(egui::RichText::new(&self.result).monospace());
    }
}

#[cfg(test)]
mod tests {
    /// Every preset finds its equilibrium; the flames burn at their adiabatic temperatures.
    #[test]
    fn every_preset_equilibrates() {
        let mut l = super::Lab::load();
        for k in 0..super::PRESETS.len() {
            l.choose(k);
            assert!(
                !l.result.starts_with("no equilibrium"),
                "{}: {}",
                super::PRESETS[k].name,
                l.result
            );
        }
        l.choose(0);
        assert!(
            l.result.starts_with("2223") || l.result.starts_with("2224"),
            "methane in air: {}",
            l.result
        );
    }
}
