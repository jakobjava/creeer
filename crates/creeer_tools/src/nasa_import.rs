//! `import-nasa9 <thermo.inp> <out.ron>`: converts NASA Glenn's thermodynamic database
//! (`thermo.inp` of the CEA program, github.com/nasa/cea, Apache-2.0; B. J. McBride,
//! M. J. Zehe and S. Gordon, NASA/TP-2002-211556, updated by NASA GRC to 2021) into the
//! game's species file (`data/chemistry/species.ron`, read by `creeer_materials::species`).
//!
//! The file is read as data, in its fixed-width records (TP-2002-211556, appendix A): a
//! species' name, then its element counts, phase, molar mass and heat of formation, then for
//! each temperature range its bounds and nine coefficients (Fortran `D` exponents). A species
//! split over two records (iron's α phase below and above its magnetic transition, hematite
//! about its Curie point) is joined. Ions (an electron in the formula), deuterated species,
//! the CEA's "inert" copies and reactants without coefficients are left out.

use std::fmt::Write as _;
use std::path::Path;

/// One species as read.
struct Entry {
    name: String,
    comment: String,
    date: String,
    atoms: Vec<(String, f64)>,
    condensed: bool,
    ranges: Vec<f64>,
    polynomials: Vec<[f64; 9]>,
}

/// Columns `from..to` of `line` (blank beyond its end), trimmed.
fn field(line: &str, from: usize, to: usize) -> &str {
    let to = to.min(line.len());
    if from >= to {
        ""
    } else {
        line[from..to].trim()
    }
}

/// A Fortran real (`D` exponent).
fn real(text: &str) -> Result<f64, String> {
    text.replace(['D', 'd'], "E")
        .parse::<f64>()
        .map_err(|e| format!("number {text:?}: {e}"))
}

/// An element symbol as NASA writes it (`FE`) as the element table has it (`Fe`).
fn symbol(text: &str) -> String {
    let mut chars = text.chars();
    chars.next().map_or_else(String::new, |first| {
        first.to_ascii_uppercase().to_string() + &chars.as_str().to_ascii_lowercase()
    })
}

fn read(path: &Path) -> Result<Vec<Entry>, String> {
    let text = std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let lines: Vec<&str> = text.lines().filter(|l| !l.starts_with('!')).collect();
    let start = lines
        .iter()
        .position(|l| l.trim() == "thermo")
        .ok_or("no `thermo` record")?
        + 2;
    let mut entries: Vec<Entry> = Vec::new();
    let mut at = start;
    while at < lines.len() {
        let line = lines[at];
        if line.starts_with("END") {
            at += 1;
            continue;
        }
        let (name, comment) = (
            field(line, 0, 18).to_owned(),
            field(line, 18, 80).to_owned(),
        );
        let head = lines
            .get(at + 1)
            .ok_or(format!("{name}: no second record"))?;
        let intervals: usize = field(head, 0, 2)
            .parse()
            .map_err(|e| format!("{name}: intervals: {e}"))?;
        let date = field(head, 3, 9).to_owned();
        let mut atoms = Vec::new();
        for k in 0..5 {
            let element = field(head, 10 + 8 * k, 12 + 8 * k);
            let count = real(field(head, 12 + 8 * k, 18 + 8 * k))?;
            if !element.is_empty() && count != 0.0 {
                atoms.push((symbol(element), count));
            }
        }
        let condensed = field(head, 50, 52) != "0";
        let mut ranges = Vec::new();
        let mut polynomials = Vec::new();
        for i in 0..intervals {
            let base = at + 2 + 3 * i;
            let (bounds, first, second) = (lines[base], lines[base + 1], lines[base + 2]);
            let (mut low, high) = (real(field(bounds, 0, 11))?, real(field(bounds, 11, 22))?);
            // NASA's 2021 revision set most lower bounds to 300 K, and so turned eleven species'
            // first interval (from 200 K up to 298.15 K or a melting point below 300 K) inside
            // out: its bound is 200 K again, as the database had it.
            if i == 0 && low >= high {
                low = 200.0;
            }
            #[allow(clippy::float_cmp)] // the same bound written twice, digit for digit
            if ranges.last().is_none_or(|&t: &f64| t != low) {
                if !ranges.is_empty() {
                    return Err(format!("{name}: ranges with a gap at {low} K"));
                }
                ranges.push(low);
            }
            ranges.push(high);
            let mut p = [0.0; 9];
            for (j, c) in p.iter_mut().take(5).enumerate() {
                *c = real(field(first, 16 * j, 16 * j + 16))?;
            }
            p[5] = real(field(second, 0, 16))?;
            p[6] = real(field(second, 16, 32))?;
            p[7] = real(field(second, 48, 64))?;
            p[8] = real(field(second, 64, 80))?;
            polynomials.push(p);
        }
        // A reactant without coefficients has one more record: the temperature its enthalpy
        // is assigned at.
        at += if intervals == 0 { 3 } else { 2 + 3 * intervals };
        // The same species continued (its next temperature ranges in a record of its own).
        if let Some(last) = entries
            .last_mut()
            .filter(|e| e.name == name && !polynomials.is_empty())
        {
            if last.ranges.last() != ranges.first() {
                return Err(format!("{name}: records that do not join"));
            }
            last.ranges.extend_from_slice(&ranges[1..]);
            last.polynomials.extend(polynomials);
            continue;
        }
        let atoms = precise(&name, atoms);
        entries.push(Entry {
            name,
            comment,
            date,
            atoms,
            condensed,
            ranges,
            polynomials,
        });
    }
    Ok(entries)
}

/// The counts in `name` where it spells the formula more precisely than the record's
/// two-decimal fields and agrees with them to that: wüstite, Fe.947O, whose record says
/// Fe 0.95 (and its data are Pankratz's for Fe₀.₉₄₇O). Otherwise the record's `atoms`.
fn precise(name: &str, atoms: Vec<(String, f64)>) -> Vec<(String, f64)> {
    let formula = name.split('(').next().unwrap_or(name);
    let mut spelled: Vec<(String, f64)> = Vec::new();
    let chars: Vec<char> = formula.chars().collect();
    let mut k = 0;
    while k < chars.len() {
        if !chars[k].is_ascii_uppercase() {
            return atoms;
        }
        let mut symbol = chars[k].to_string();
        k += 1;
        if k < chars.len() && chars[k].is_ascii_lowercase() {
            symbol.push(chars[k]);
            k += 1;
        }
        let start = k;
        while k < chars.len() && (chars[k].is_ascii_digit() || chars[k] == '.') {
            k += 1;
        }
        let digits: String = chars[start..k].iter().collect();
        let count = if digits.is_empty() {
            1.0
        } else {
            digits.parse().unwrap_or(f64::NAN)
        };
        spelled.push((symbol, count));
    }
    let agrees = spelled.len() == atoms.len()
        && spelled
            .iter()
            .zip(&atoms)
            .all(|(s, a)| s.0 == a.0 && (s.1 - a.1).abs() <= 0.005);
    if agrees { spelled } else { atoms }
}

/// Whether the game keeps a species.
fn kept(e: &Entry) -> bool {
    !e.polynomials.is_empty()
        && !e.atoms.iter().any(|(s, _)| s == "E" || s == "D")
        && !e.name.to_ascii_lowercase().contains("inert")
}

fn write_entry(out: &mut String, e: &Entry) -> Result<(), String> {
    let phase = match (e.condensed, e.name.contains("(L)")) {
        (false, _) => "Gas",
        (true, true) => "Liquid",
        (true, false) => "Solid",
    };
    let atoms: Vec<String> = e
        .atoms
        .iter()
        .map(|(s, n)| format!("(\"{s}\", {n:?})"))
        .collect();
    let ranges: Vec<String> = e.ranges.iter().map(|r| format!("{r:?}")).collect();
    let polys: Vec<String> = e
        .polynomials
        .iter()
        .map(|p| {
            format!(
                "[{}]",
                p.iter()
                    .map(|c| format!("{c:e}"))
                    .collect::<Vec<_>>()
                    .join(", ")
            )
        })
        .collect();
    let note = format!("{} {}", e.date, e.comment);
    writeln!(
        out,
        "    (name: {:?}, atoms: [{}], phase: {phase}, ranges: [{}], polynomials: [{}], note: {:?}),",
        e.name,
        atoms.join(", "),
        ranges.join(", "),
        polys.join(", "),
        note.trim()
    )
    .map_err(|e| e.to_string())
}

/// Converts `thermo` into `out`. Returns how many species were written.
pub fn run(thermo: &Path, out: &Path) -> Result<usize, String> {
    let mut text = String::from(
        "// Species' thermodynamic data: NASA-9 polynomials for c_p/R, H/RT and S/R of each species\n\
         // in its standard state (1 bar), by temperature range. From NASA Glenn's database:\n\
         // `thermo.inp` of the CEA program (github.com/nasa/cea, Apache License 2.0, see\n\
         // LICENSE-nasa-cea.txt beside this file); B. J. McBride, M. J. Zehe and S. Gordon,\n\
         // \"NASA Glenn Coefficients for Calculating Thermodynamic Properties of Individual\n\
         // Species\", NASA/TP-2002-211556, updated by NASA GRC to 2021. Each `note` is NASA's date\n\
         // code (g: Glenn, j: JANAF, tpis: Gurvich, ...) and source. Changed from the original:\n\
         // converted to this format; ions, deuterated species, inert copies and reactants without\n\
         // coefficients left out; species split over two records joined; eleven first intervals\n\
         // the 2021 revision turned inside out (lower bound 300 K above the upper) start at 200 K\n\
         // again, as before it; wüstite's iron 0.947, as its name and data have it (its record's\n\
         // two-decimal field says 0.95). H(298.15 K) is the heat of formation. Written by\n\
         // `creeer_tools import-nasa9`: do not edit by hand.\n[\n",
    );
    let mut count = 0;
    let mut seen = std::collections::HashSet::new();
    for e in read(thermo)? {
        if kept(&e) && seen.insert(e.name.clone()) {
            write_entry(&mut text, &e)?;
            count += 1;
        }
    }
    text.push_str("]\n");
    std::fs::write(out, text).map_err(|e| format!("{}: {e}", out.display()))?;
    Ok(count)
}
