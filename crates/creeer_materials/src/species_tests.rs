//! The species data against the tables they were fitted to: JANAF (M. W. Chase, NIST-JANAF
//! Thermochemical Tables, 4th ed., 1998) and Gurvich et al. (Thermodynamic Properties of
//! Individual Substances, 4th ed.) where NASA cites them (`L` in a species' note).

use crate::species::{Phase, SpeciesTable};

/// (species, T (K), ΔH from the elements at 298.15 K plus H(T) − H(298.15) (kJ/mol),
/// S (J/(mol·K)), c_p (J/(mol·K))), from the JANAF tables but for two whose heats of formation
/// NASA took from later work: methane Gurvich's (−74.600 kJ/mol; JANAF −74.873, the Active
/// Thermochemical Tables −74.52) and OH Ruščić's (2002: 37.30 kJ/mol; JANAF 38.987), its c_p
/// Gurvich's (29.886 J/(mol·K); JANAF 29.986).
const JANAF: [(&str, f64, f64, f64, f64); 14] = [
    ("H2O", 298.15, -241.826, 188.834, 33.590),
    ("H2O", 1000.0, -241.826 + 26.000, 232.738, 41.268),
    ("H2O(L)", 298.15, -285.830, 69.950, 75.351),
    ("CO2", 298.15, -393.522, 213.795, 37.135),
    ("CO2", 1000.0, -393.522 + 33.397, 269.299, 54.308),
    ("CO", 298.15, -110.527, 197.653, 29.141),
    ("CH4", 298.15, -74.600, 186.369, 35.69),
    ("O2", 298.15, 0.0, 205.147, 29.376),
    ("O2", 2000.0, 59.199, 268.748, 37.788),
    ("N2", 298.15, 0.0, 191.609, 29.124),
    ("NH3", 298.15, -45.898, 192.774, 35.652),
    ("Fe(a)", 298.15, 0.0, 27.280, 25.094),
    ("C(gr)", 298.15, 0.0, 5.740, 8.517),
    ("OH", 298.15, 37.30, 183.708, 29.886),
];

#[test]
fn the_shipped_species_parse_and_match_janaf() {
    let table = SpeciesTable::reference();
    assert!(table.all().len() > 900, "{} species", table.all().len());
    for (name, t, h, s, cp) in JANAF {
        let sp = table.get(name).unwrap_or_else(|| panic!("{name} missing"));
        // NASA's fits reproduce the tables to a few J/mol and hundredths of J/(mol·K).
        assert!(
            (sp.h(t) / 1e3 - h).abs() < 0.05,
            "{name} at {t} K: H {} kJ/mol, JANAF {h}",
            sp.h(t) / 1e3
        );
        assert!(
            (sp.s(t) - s).abs() < 0.05,
            "{name} at {t} K: S {}, JANAF {s}",
            sp.s(t)
        );
        assert!(
            (sp.cp(t) / cp - 1.0).abs() < 2e-3,
            "{name} at {t} K: c_p {}, JANAF {cp}",
            sp.cp(t)
        );
    }
    let water = table.get("H2O").unwrap();
    // From the game's atomic weights (IUPAC's abridged: mass balances add up from the same ones).
    assert!(
        (water.molar_mass - 18.015e-3).abs() < 1e-9,
        "{}",
        water.molar_mass
    );
    assert_eq!(table.get("H2O(L)").unwrap().phase, Phase::Liquid);
    assert_eq!(table.get("Fe(a)").unwrap().phase, Phase::Solid);
    assert_eq!(table.get("SiO2(a-qz)").unwrap().phase, Phase::Solid);
    assert_eq!(table.get("SiO2(L)").unwrap().phase, Phase::Liquid);
}

/// Each species' polynomials join where their ranges meet: H continuous (to the fits'
/// constraint; a jump in H would be a latent heat no phase change has). But for two, kept as
/// NASA gives them: SnS(cr) jumps by 0.092·RT at 875 K, its α→β transition within the one
/// species (a latent heat); ALN(L) by 3.03·RT at 2700 K, as Gurvich (1996) tabulates it.
#[test]
fn polynomials_join_at_their_range_boundaries() {
    let table = SpeciesTable::reference();
    let mut worst = (0.0f64, String::new());
    for sp in table.all() {
        let (low, high) = sp.range();
        let mut t = low;
        // Probe just either side of every interior boundary.
        for _ in 0..8 {
            let next = (low..high).contains(&t).then_some(t);
            let Some(t0) = next else { break };
            let boundary = (1..)
                .map(|k| t0 + f64::from(k))
                .find(|&b| (sp.cp_r(b - 1e-6) - sp.cp_r(b + 1e-6)).abs() > 1e-3 || b >= high);
            let Some(b) = boundary.filter(|&b| b < high) else {
                break;
            };
            let jump = (sp.h_rt(b - 1e-6) - sp.h_rt(b + 1e-6)).abs();
            let known = [("SnS(cr)", 0.093), ("ALN(L)", 3.04)];
            if let Some((_, most)) = known.iter().find(|(n, _)| *n == sp.name) {
                assert!(jump < *most, "{} jumps by {jump}", sp.name);
                continue;
            }
            if jump > worst.0 {
                worst = (jump, format!("{} at {b} K", sp.name));
            }
            t = b + 1.0;
        }
    }
    assert!(worst.0 < 1e-3, "H/RT jumps by {} ({})", worst.0, worst.1);
}
