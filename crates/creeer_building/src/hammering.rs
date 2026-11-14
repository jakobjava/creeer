//! Driving a nail by hand (G2.11, ADR-0036 amended 2026-11-14): how many hammer blows a nail
//! takes and how long they take, from the work of driving it and a carpenter's blows
//! (`data/hammering.ron`), not a time written down for it.
//!
//! The work: a smooth nail is held along its length by the wood's friction on it, EN 1995-1-1's
//! withdrawal parameter f_ax = 20·10⁻⁶·ρ_k² (N/mm², ρ_k in kg/m³) per millimetre of diameter and
//! of length in the wood. Driven, that grip grows as it goes in, so its work is the area under
//! it: through part a (thickness t_a, f_a) into b, W = d·(f_a·(t_a·L − t_a²/2) + f_b·(L − t_a)²/2)
//! (the tip's crushing of the wood left out). A blow drives with a share of its energy; the
//! rest goes back into the hammer and the wood.

use crate::law::Side;
use creeer_data::Data;
use creeer_data::merge::ron_list;
use serde::Deserialize;

pub const TABLE: &str = "hammering.ron";

/// A way of driving nails: whose blows, how hard and how fast.
#[derive(Clone, Debug, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Hammering {
    pub name: String,
    /// A blow's energy (J).
    pub blow: f64,
    /// The share of it that drives the nail.
    pub share: f64,
    /// Blows a second.
    pub rate: f64,
}

impl Hammering {
    /// Every entry of `data`'s packs (the first is a player's).
    ///
    /// # Errors
    /// If the table does not load.
    pub fn all(data: &Data) -> Result<Vec<Self>, String> {
        (data.table("hammering", TABLE))
            .merge(ron_list, |h: &Self| &h.name)
            .map_err(|e| e.to_string())
    }

    /// The blows driving a nail of `diameter` and `length` (m) through `a` into `b` takes: at
    /// least one.
    // (A nail of reason takes a few blows: validated under a thousand.)
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    pub fn blows(&self, (diameter, length): (f64, f64), (a, b): (&Side, &Side)) -> u32 {
        let work = driving_work((diameter, length), (a, b));
        (work / (self.blow * self.share)).ceil().clamp(1.0, 1000.0) as u32
    }

    /// How long `blows` take (s).
    pub fn seconds(&self, blows: u32) -> f64 {
        f64::from(blows) / self.rate
    }
}

/// The work of driving a smooth nail of `diameter` and `length` (m) through part `a` into `b`
/// (J): its withdrawal grip, growing as it goes in, integrated over its way.
pub fn driving_work((diameter, length): (f64, f64), (a, b): (&Side, &Side)) -> f64 {
    // f_ax in Pa: 20·10⁻⁶·ρ² N/mm² is 20·ρ² Pa.
    let grip = |s: &Side| 20.0 * s.density * s.density;
    let through = a.thickness.min(length);
    let into = length - through;
    diameter
        * (grip(a) * (through * length - 0.5 * through * through) + grip(b) * 0.5 * into * into)
}

/// What is wrong with the entries: energies, shares and rates out of reason.
pub fn problems(all: &[Hammering]) -> Vec<String> {
    (all.iter())
        .filter(|h| {
            !((1.0..1000.0).contains(&h.blow)
                && h.share > 0.0
                && h.share <= 1.0
                && (0.1..10.0).contains(&h.rate))
        })
        .map(|h| format!("hammering {}: {h:?} out of reason", h.name))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn side(density: f64, thickness: f64) -> Side {
        Side {
            density,
            across: 2.5e6,
            across_stiffness: 3.7e8,
            thickness,
            damping: 0.0,
        }
    }

    fn carpenter() -> Hammering {
        let all = Hammering::all(&Data::base()).expect("the table");
        assert!(problems(&all).is_empty(), "{:?}", problems(&all));
        all.into_iter().next().expect("an entry")
    }

    #[test]
    fn a_framing_nail_takes_the_blows_a_framer_takes() {
        // A 16d common nail (88.9 × 4.11 mm) through a 38 mm stud into another, framing timber
        // of ρ_k 380: 47 J of work; Pro Tool Reviews' framers took 2.3–2.7 swings a nail.
        let (a, b) = (side(380.0, 0.038), side(380.0, 0.038));
        let work = driving_work((0.00411, 0.0889), (&a, &b));
        assert!((work - 46.9).abs() < 0.5, "{work} J");
        let h = carpenter();
        let blows = h.blows((0.00411, 0.0889), (&a, &b));
        assert!((2..=3).contains(&blows), "{blows} blows");
    }

    #[test]
    fn the_catalogues_nails_into_green_spruce_take_a_second_or_two() {
        // Green spruce at its seasoned 450 kg/m³: a 60 mm rail nailed to a pole.
        let h = carpenter();
        let (rail, pole) = (side(450.0, 0.06), side(450.0, 0.12));
        let seconds = |d: f64, l: f64| h.seconds(h.blows((d, l), (&rail, &pole)));
        let (n75, n90, n100) = (
            seconds(0.0031, 0.075),
            seconds(0.0034, 0.090),
            seconds(0.004, 0.1),
        );
        eprintln!("75 mm {n75} s, 90 mm {n90} s, 100 mm {n100} s");
        assert!(n75 <= n90 && n90 <= n100, "longer nails take longer");
        assert!((0.5..=1.5).contains(&n75) && (1.0..=3.0).contains(&n100));
    }
}
