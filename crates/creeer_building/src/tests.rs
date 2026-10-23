//! The fasteners' laws against EN 1995-1-1, and the catalogue.

use crate::fasteners::{Fastener, problems, side};
use crate::law::{Placement, Side};
use creeer_data::Data;
use creeer_materials::MaterialTable;

fn catalogue() -> (Vec<Fastener>, MaterialTable) {
    let data = Data::base();
    (
        Fastener::all(&data).expect("the fasteners load"),
        MaterialTable::from_data(&data).expect("the materials load"),
    )
}

fn named<'a>(all: &'a [Fastener], name: &str) -> &'a Fastener {
    all.iter()
        .find(|f| f.name == name)
        .expect("in the catalogue")
}

/// C24 (ρ_mean 420 kg/m³), `thickness` m along the fasteners.
fn c24(materials: &MaterialTable, thickness: f64) -> Side {
    side(materials.get("spruce_c24").expect("C24"), thickness).expect("a wood")
}

const ONE: Placement = Placement {
    count: 1,
    spread: 0.0,
    contact: 0.01,
};

/// EN 1995-1-1 Table 7.1: a 3.1 mm nail in C24 slips by ρ_m^1.5·d^0.8/30 = 708.7 N/mm.
#[test]
fn a_nails_slip_modulus_is_the_codes() {
    let (all, m) = catalogue();
    let law = named(&all, "nail_75")
        .law(&m, (&c24(&m, 0.038), &c24(&m, 0.05)), &ONE, 0.044)
        .expect("holds");
    assert!(
        (law.lateral / 708.7e3 - 1.0).abs() < 1e-3,
        "{} N/m",
        law.lateral
    );
}

/// A 4 × 100 mm nail through 50 mm of C24 into 50 mm more: Johansen's mode (f), two plastic
/// hinges, governs: √(2·M_y·f_h·d) with M_y = 0.3·600·4^2.6 = 6,617 N·mm and
/// f_h = 0.082·420·4^−0.3 = 22.72 N/mm²: 1,097 N. It pulls out at 20·10⁻⁶·420²·4·50 = 706 N
/// (its 9 mm head holds 1,000 N), and the rope effect adds min(706/4, 15 % of 1,097) = 165 N.
#[test]
fn a_long_nail_yields_by_two_hinges_and_its_rope_effect_adds() {
    let (all, m) = catalogue();
    let law = named(&all, "nail_100")
        .law(&m, (&c24(&m, 0.05), &c24(&m, 0.05)), &ONE, 0.05)
        .expect("holds");
    let close = |a: f64, b: f64| (a / b - 1.0).abs() < 2e-3;
    assert!(
        close(law.lateral_yield, 1096.7),
        "yield {}",
        law.lateral_yield
    );
    assert!(
        close(law.withdrawal, 705.6),
        "withdrawal {}",
        law.withdrawal
    );
    assert!(
        close(law.lateral_peak, 1096.7 + 164.5),
        "peak {}",
        law.lateral_peak
    );
    assert!(
        (law.interaction - 1.0).abs() < f64::EPSILON,
        "smooth nails interact linearly"
    );
}

#[test]
fn a_nail_too_short_for_the_second_part_is_refused() {
    let (all, m) = catalogue();
    let short = named(&all, "nail_75").law(&m, (&c24(&m, 0.06), &c24(&m, 0.05)), &ONE, 0.055);
    assert!(short.is_err(), "15 mm into the second part: under 6·d");
}

/// Two nails hold twice one, and turn by their slip at their radius.
#[test]
fn fasteners_add_and_their_spread_resists_turning() {
    let (all, m) = catalogue();
    let nail = named(&all, "nail_100");
    let (a, b) = (c24(&m, 0.05), c24(&m, 0.05));
    let one = nail.law(&m, (&a, &b), &ONE, 0.05).expect("holds");
    let two = Placement {
        count: 2,
        spread: 2.0 * 0.04 * 0.04,
        contact: 0.01,
    };
    let pair = nail.law(&m, (&a, &b), &two, 0.05).expect("holds");
    assert!((pair.lateral_yield / one.lateral_yield - 2.0).abs() < 1e-12);
    // Σ K·r² with each nail's own diameter as its least arm: (2·0.04² + 2·d²)/d² = 202.
    let ratio = pair.twist / one.twist;
    assert!(
        (ratio - 202.0).abs() < 1e-9,
        "two nails 8 cm apart against one: {ratio}"
    );
}

/// A bast lashing's four turns (eight strands of 12 × 2 mm at 20 MPa) break at 3,840 N pulled
/// apart, and slip at μ = 0.4 on their clamp: from a quarter of that pressing to all of it.
#[test]
fn a_lashing_clamps_by_its_turns() {
    let (all, m) = catalogue();
    let law = named(&all, "bast_lashing")
        .law(&m, (&c24(&m, 0.15), &c24(&m, 0.15)), &ONE, 0.15)
        .expect("holds");
    let close = |a: f64, b: f64| (a / b - 1.0).abs() < 1e-9;
    assert!(close(law.withdrawal, 3840.0), "{}", law.withdrawal);
    assert!(
        close(law.lateral_peak, 0.4 * 3840.0),
        "{}",
        law.lateral_peak
    );
    assert!(
        close(law.lateral_yield, 0.25 * law.lateral_peak),
        "{}",
        law.lateral_yield
    );
}

/// Every entry, generically: it validates, and fastening a board as thick as 40 % of it (a
/// lashing: 8 cm) onto a green spruce pole makes a law the solver takes (physical: its
/// equivalent member builds).
#[test]
fn every_fastener_is_valid_and_makes_a_physical_joint() {
    use crate::fasteners::Kind;
    let (all, m) = catalogue();
    assert!(problems(&all, &m).is_empty(), "{:?}", problems(&all, &m));
    let green = m.get("spruce_green").expect("green spruce");
    for f in &all {
        let first = match f.kind {
            Kind::Nail { length, .. } | Kind::Peg { length, .. } => 0.4 * length,
            Kind::Lashing { .. } => 0.08,
        };
        let between = 0.5 * first + 0.075;
        let (a, b) = (
            side(green, first).expect("wood"),
            side(green, 0.15).expect("wood"),
        );
        let law =
            (f.law(&m, (&a, &b), &ONE, between)).unwrap_or_else(|e| panic!("{}: {e}", f.name));
        let _ = law.equivalent(between);
    }
}

/// Green wood fastens by its seasoned density, not its wet one (EN 1995-1-1's ρ at 12 %).
#[test]
fn green_wood_holds_by_its_seasoned_density() {
    let (_, m) = catalogue();
    let green = side(m.get("spruce_green").expect("green spruce"), 0.1).expect("wood");
    assert!((green.density - 450.0).abs() < 1e-9, "{}", green.density);
}
