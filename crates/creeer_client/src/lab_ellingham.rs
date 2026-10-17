//! The lab's Ellingham diagram: ΔG° per mole of O₂ of forming each oxide against
//! temperature, from the species' data, each substance in its stable phase at every
//! temperature (iron's α or γ, its oxide's solid or liquid): the lower line's element takes
//! oxygen from the higher's oxide.

use creeer_materials::species::{R, Species, SpeciesTable};

/// An oxidation per mole of O₂: metal (or reductant) phases' formula and moles, oxide's.
struct Line {
    name: &'static str,
    reactant: (&'static str, f64),
    oxide: (&'static str, f64),
}

const LINES: [Line; 11] = [
    Line {
        name: "4/3 Al + O₂ → 2/3 Al₂O₃",
        reactant: ("AL", 4.0 / 3.0),
        oxide: ("AL2O3", 2.0 / 3.0),
    },
    Line {
        name: "2 Mg + O₂ → 2 MgO",
        reactant: ("Mg", 2.0),
        oxide: ("MgO", 2.0),
    },
    Line {
        name: "Ti + O₂ → TiO₂",
        reactant: ("Ti", 1.0),
        oxide: ("TiO2", 1.0),
    },
    Line {
        name: "Si + O₂ → SiO₂",
        reactant: ("Si", 1.0),
        oxide: ("SiO2", 1.0),
    },
    Line {
        name: "4/3 Cr + O₂ → 2/3 Cr₂O₃",
        reactant: ("Cr", 4.0 / 3.0),
        oxide: ("Cr2O3", 2.0 / 3.0),
    },
    Line {
        name: "2 C + O₂ → 2 CO",
        reactant: ("C", 2.0),
        oxide: ("CO", 2.0),
    },
    Line {
        name: "2 Fe + O₂ → 2 FeO",
        reactant: ("Fe", 2.0 / 0.947),
        oxide: ("Fe.947O", 2.0),
    },
    Line {
        name: "2 H₂ + O₂ → 2 H₂O",
        reactant: ("H2", 2.0),
        oxide: ("H2O", 2.0),
    },
    Line {
        name: "2 CO + O₂ → 2 CO₂",
        reactant: ("CO", 2.0),
        oxide: ("CO2", 2.0),
    },
    Line {
        name: "2 Ni + O₂ → 2 NiO",
        reactant: ("Ni", 2.0),
        oxide: ("NiO", 2.0),
    },
    Line {
        name: "4 Cu + O₂ → 2 Cu₂O",
        reactant: ("Cu", 4.0),
        oxide: ("Cu2O", 2.0),
    },
];

/// The stable phase of the substance `formula` at `t`: of the species named it (a gas) or it
/// with a phase in brackets, and of its very composition (not Fe(OH)₂ for Fe), those whose
/// data cover `t`, the lowest in G.
fn stable<'a>(table: &'a SpeciesTable, formula: &str, t: f64) -> Option<&'a Species> {
    let named = |s: &&Species| s.name == formula || s.name.split('(').next() == Some(formula);
    let atoms = &table.all().iter().find(|s| named(s))?.formula;
    table
        .all()
        .iter()
        .filter(|s| named(s) && s.formula == *atoms && s.covers(t))
        .min_by(|a, b| a.g_rt(t).total_cmp(&b.g_rt(t)))
}

/// Each line's points (°C, kJ per mole of O₂) from 300 K to 2300 K.
pub fn lines(table: &SpeciesTable) -> Vec<(&'static str, Vec<[f64; 2]>)> {
    LINES
        .iter()
        .map(|line| {
            let points = (300..=2300)
                .step_by(25)
                .filter_map(|t| {
                    let t = f64::from(t);
                    let (reactant, oxide, oxygen) = (
                        stable(table, line.reactant.0, t)?,
                        stable(table, line.oxide.0, t)?,
                        table.get("O2")?,
                    );
                    let dg = line.oxide.1 * oxide.g_rt(t)
                        - line.reactant.1 * reactant.g_rt(t)
                        - oxygen.g_rt(t);
                    Some([t - 273.15, dg * R * t / 1e3])
                })
                .collect();
            (line.name, points)
        })
        .collect()
}
