//! The data validator (F7.1, ADR-0018): everything the game would load from the packs,
//! loaded and checked, with each problem named by pack, file and entry.
//!
//! - every table: materials (each checked as the solvers need it), species, minerals, rocks;
//! - every planet in `planets/` (its geology, its air's gases among the species) and every
//!   file of `planets/` either a planet or some planet's geology;
//! - every solar system in `systems/` and the planet templates it names;
//! - the data integrity checks of plan.md §6 over the merged data: species' heat capacities
//!   positive over their ranges and their enthalpies continuous where ranges meet, and the
//!   signs of ΔG of reactions whose direction is known (so a mod's species cannot make water
//!   fall apart by itself at room temperature);
//! - the palette and its approved exceptions (G1.3a, `docs/art-style.md`) and every model in
//!   the manifests (G1.3b): they must load, and what breaks the style is a warning.

use crate::surroundings::Surroundings;
use creeer_art::model::{Entry, Model};
use creeer_art::{Exception, Palette, Srgb, checks, model_check};
use creeer_data::Data;
use creeer_materials::MaterialTable;
use creeer_materials::species::{Species, SpeciesTable};
use creeer_orbit::generate::Params;
use creeer_terrain::Planet;
use creeer_terrain::geology::rocks::RockTable;

/// What the validator found.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Report {
    /// What would stop the game loading the data.
    pub errors: Vec<String>,
    /// What looks wrong but loads.
    pub warnings: Vec<String>,
    /// What was checked: (kind, how many).
    pub counts: Vec<(&'static str, usize)>,
}

impl Report {
    /// Whether the data loads.
    pub fn ok(&self) -> bool {
        self.errors.is_empty()
    }
}

/// Reactions whose direction at 298.15 K and 1 bar is beyond doubt: (reactants, products,
/// what ΔG's sign must be). Each species with its stoichiometric number.
type Known = (
    &'static [(f64, &'static str)],
    &'static [(f64, &'static str)],
    f64,
);
const KNOWN: &[Known] = &[
    // Water forms from its elements (ΔG° = −237 kJ/mol).
    (&[(1.0, "H2"), (0.5, "O2")], &[(1.0, "H2O(L)")], -1.0),
    // Limestone does not calcine at room temperature (ΔG° = +130 kJ/mol).
    (
        &[(1.0, "CaCO3(cr)")],
        &[(1.0, "CaO(cr)"), (1.0, "CO2")],
        1.0,
    ),
    // Iron rusts in oxygen (ΔG° = −742 kJ/mol of hematite).
    (&[(2.0, "Fe(a)"), (1.5, "O2")], &[(1.0, "Fe2O3(cr)")], -1.0),
    // Methane burns (ΔG° = −818 kJ/mol).
    (
        &[(1.0, "CH4"), (2.0, "O2")],
        &[(1.0, "CO2"), (2.0, "H2O")],
        -1.0,
    ),
];

/// Validates every pack of `data` as merged.
pub fn validate(data: &Data) -> Report {
    let mut report = Report::default();
    match MaterialTable::from_data(data) {
        Ok(t) => report.counts.push(("materials", t.all().len())),
        Err(e) => report.errors.push(e),
    }
    let species = match SpeciesTable::from_data(data) {
        Ok(t) => {
            report.counts.push(("species", t.all().len()));
            check_species(&t, &mut report);
            Some(t)
        }
        Err(e) => {
            report.errors.push(e);
            None
        }
    };
    match RockTable::from_data(data) {
        Ok(t) => {
            report.counts.push(("minerals", t.minerals.len()));
            report.counts.push(("rocks", t.rocks.len()));
            art(data, &t, &mut report);
        }
        Err(e) => {
            // Every planet would fail the same way: say it once.
            report.errors.push(e.0);
            return report;
        }
    }
    planets(data, species.as_ref(), &mut report);
    systems(data, &mut report);
    report
}

/// The palette and its exceptions (G1.3a): they must load. The style's checks are warnings,
/// since a mod may choose its own look; the base pack is held to them by `creeer_art`'s tests.
fn art(data: &Data, rocks: &RockTable, report: &mut Report) {
    let palette = Palette::from_data(data);
    let exceptions = Exception::from_data(data);
    let (palette, exceptions) = match (palette, exceptions) {
        (Ok(p), Ok(e)) => (p, e),
        (p, e) => {
            report.errors.extend(p.err());
            report.errors.extend(e.err());
            return;
        }
    };
    report.counts.push(("palette tokens", palette.tokens.len()));
    let rocks: Vec<checks::Rock> = (rocks.rocks.iter())
        .map(|r| checks::Rock {
            name: &r.name,
            colour: Srgb(r.color),
            ore: r.ore,
        })
        .collect();
    for finding in checks::check(&palette, &rocks, &exceptions) {
        report.warnings.push(format!("art style: {finding}"));
    }
    models(data, &palette, report);
}

/// Every model in the manifests (G1.3b): it must load, its materials palette tokens; what
/// breaks the style (budget, bounds, facets) is a warning.
fn models(data: &Data, palette: &Palette, report: &mut Report) {
    let entries = match Entry::all(data) {
        Ok(e) => e,
        Err(e) => return report.errors.push(e),
    };
    report.counts.push(("models", entries.len()));
    for entry in &entries {
        let at = format!("{}: {}", origin(data, &entry.file), entry.file);
        let model = data
            .binary(&entry.file)
            .map_err(|e| e.to_string())
            .and_then(|bytes| Model::read(bytes, palette));
        match model {
            Ok(model) => {
                for problem in model_check::check(&model, entry) {
                    report.warnings.push(format!("art style: {at}: {problem}"));
                }
            }
            Err(e) => report.errors.push(format!("{at}: {e}")),
        }
    }
}

/// Whether a file's first item (past comments) is `Planet(`.
fn is_planet(text: &str) -> bool {
    text.lines()
        .map(str::trim)
        .find(|l| !l.is_empty() && !l.starts_with("//"))
        .is_some_and(|l| l.starts_with("Planet("))
}

fn planets(data: &Data, species: Option<&SpeciesTable>, report: &mut Report) {
    let files = data.list("planets/");
    let mut geologies = Vec::new();
    let mut count = 0;
    for &path in &files {
        let Ok(text) = data.file(path) else { continue };
        if !is_planet(text) {
            continue;
        }
        count += 1;
        match Planet::load(data, path) {
            Ok(planet) => {
                if let Some(geology) = geology_of(text) {
                    geologies.push(format!("planets/{geology}"));
                }
                if let Some(table) = species
                    && let Err(e) = Surroundings::of(&planet, table)
                {
                    report
                        .errors
                        .push(format!("{}: {path}: air: {e}", origin(data, path)));
                }
            }
            Err(e) => report.errors.push(e.0),
        }
    }
    report.counts.push(("planets", count));
    for path in files {
        let used = geologies.iter().any(|g| g == path);
        let planet = data.file(path).is_ok_and(is_planet);
        if !used && !planet {
            report.warnings.push(format!(
                "{}: {path}: neither a planet nor any planet's geology",
                origin(data, path)
            ));
        }
    }
}

/// The geology file a planet's text names.
fn geology_of(text: &str) -> Option<&str> {
    let after = text.split("geology:").nth(1)?;
    let start = after.find('"')? + 1;
    let end = start + after[start..].find('"')?;
    Some(&after[start..end])
}

fn origin<'a>(data: &'a Data, path: &str) -> &'a str {
    data.origin(path).unwrap_or("?")
}

fn systems(data: &Data, report: &mut Report) {
    let files = data.list("systems/");
    report.counts.push(("systems", files.len()));
    for path in files {
        let params = match Params::load(data, path) {
            Ok(p) => p,
            Err(e) => {
                report.errors.push(format!("{}: {e}", origin(data, path)));
                continue;
            }
        };
        let templates = [
            &params.rocky.surface,
            &params.giant.surface,
            &params.moon.surface,
            &params.asteroid_surface,
        ];
        // The templates are planets of `planets/`, validated there: only that they are.
        for template in templates.into_iter().flatten() {
            if !data.file(template).is_ok_and(is_planet) {
                report.errors.push(format!(
                    "{}: {path}: template {template} is not a planet of any pack",
                    origin(data, path)
                ));
            } else if !template.starts_with("planets/")
                && let Err(e) = Planet::load(data, template)
            {
                report.errors.push(e.0);
            }
        }
    }
}

/// Heat capacities positive across each species' range, enthalpy continuous where its
/// ranges meet (a jump would be a latent heat no phase change has), and the known
/// reactions' directions.
fn check_species(table: &SpeciesTable, report: &mut Report) {
    for sp in table.all() {
        if let Some(t) = negative_cp(sp) {
            report.errors.push(format!(
                "species {}: heat capacity not positive at {t} K",
                sp.name
            ));
        }
        if let Some((t, jump)) = enthalpy_jump(sp) {
            report.warnings.push(format!(
                "species {}: enthalpy jumps by {jump:.6}·RT at {t} K",
                sp.name
            ));
        }
    }
    for (reactants, products, sign) in KNOWN {
        let t = 298.15;
        let g = |side: &[(f64, &str)]| -> Option<f64> {
            side.iter()
                .map(|&(n, name)| table.get(name).map(|s| n * s.g(t)))
                .sum()
        };
        let (Some(before), Some(after)) = (g(reactants), g(products)) else {
            report.warnings.push(format!(
                "a reference reaction's species are missing: {reactants:?} → {products:?}"
            ));
            continue;
        };
        if (after - before) * sign <= 0.0 {
            report.errors.push(format!(
                "ΔG of {reactants:?} → {products:?} at 298 K is {:.0} J/mol: the wrong sign",
                after - before
            ));
        }
    }
}

/// A temperature in the species' range (sampled every 10 K and at its ends) where c_p ≤ 0.
fn negative_cp(sp: &Species) -> Option<f64> {
    let (low, high) = sp.range();
    let steps = ((high - low) / 10.0).ceil().max(1.0);
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let n = steps as usize;
    (0..=n)
        .map(|k| low + (high - low) * k as f64 / n as f64)
        .find(|&t| sp.cp_r(t).is_nan() || sp.cp_r(t) <= 0.0)
}

/// The first join of its ranges where H/RT jumps by more than 2·10⁻³ (NASA's fits join
/// within that: NaCN's phases by 1.2–1.3·10⁻³, a few J/mol).
/// NASA's data have two, kept as given (ADR-0013): SnS(cr)'s α→β transition and ALN(L)'s
/// Gurvich step; they are warnings.
fn enthalpy_jump(sp: &Species) -> Option<(f64, f64)> {
    sp.joins().iter().find_map(|&t| {
        let jump = (sp.h_rt(t - 1e-9 * t) - sp.h_rt(t + 1e-9 * t)).abs();
        (jump > 2e-3).then_some((t, jump))
    })
}
