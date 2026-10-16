use super::*;
use crate::analysis::hill_radius;
use crate::body::BodyKind;
use crate::kepler::Elements;
use std::path::PathBuf;
use std::sync::OnceLock;

fn data() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../data")
}

fn params() -> Params {
    Params::load(&data().join("systems/default.ron"), &data()).expect("default.ron loads")
}

/// The default system, generated once for all tests.
fn system() -> &'static Generated {
    static SYSTEM: OnceLock<Generated> = OnceLock::new();
    SYSTEM.get_or_init(|| {
        let start = std::time::Instant::now();
        let g = generate(&params()).expect("a stable system");
        println!(
            "generated {} majors and {} minors in {:.2} s ({} rejected candidates)",
            g.majors.len(),
            g.minors.len(),
            start.elapsed().as_secs_f64(),
            g.attempts
        );
        g
    })
}

#[test]
fn the_definition_loads_with_the_home_planet() {
    let p = params();
    assert_eq!(p.home.name, "Home");
    assert!((p.home.radius - 5e5).abs() < 1e-6);
    assert!((p.home_a - 1.36e10).abs() < 1.0);
    assert!((p.moon_i.1 - 2f64.to_radians()).abs() < 1e-15);
}

#[test]
fn home_orbits_the_star_where_the_file_says() {
    let g = system();
    let (star, home) = (&g.majors[0], &g.majors[g.home]);
    assert_eq!(star.body.kind, BodyKind::Star);
    let el = Elements::from_state(
        star.body.mu + home.body.mu,
        home.x - star.x,
        home.v - star.v,
    );
    assert!((el.semi_major_axis() / 1.36e10 - 1.0).abs() < 1e-3);
    assert!(el.inclination < 1e-3);
    // Home has its moon.
    assert!(g.parents.contains(&Some(g.home)));
    // Its spin is the file's: 6-hour day, 23.5° tilt.
    assert!((home.body.spin.rate - std::f64::consts::TAU / 21_600.0).abs() < 1e-15);
    assert!((home.body.spin.axis().angle_between(DVec3::Y) - 23.5f64.to_radians()).abs() < 1e-9);
}

#[test]
fn moons_sit_between_the_roche_limit_and_the_hill_sphere() {
    let g = system();
    let star = &g.majors[0];
    for (m, parent) in g.parents.iter().enumerate() {
        let Some(p) = *parent else { continue };
        if p == 0 {
            continue;
        }
        let (planet, moon) = (&g.majors[p], &g.majors[m]);
        assert_eq!(moon.body.kind, BodyKind::Moon);
        let mu = planet.body.mu + moon.body.mu;
        let el = Elements::from_state(mu, moon.x - planet.x, moon.v - planet.v);
        let orbit = Elements::from_state(star.body.mu, planet.x - star.x, planet.v - star.v);
        let hill = hill_radius(
            orbit.semi_major_axis(),
            orbit.e,
            planet.body.mu,
            star.body.mu,
        );
        let a = el.semi_major_axis();
        assert!(
            a < 0.31 * hill,
            "{} at {a:.3e} beyond {hill:.3e}",
            moon.body.name
        );
        assert!(
            a > 2.0 * planet.body.radius,
            "{}: a {a:.3e}",
            moon.body.name
        );
        // Tidally locked: one turn per orbit.
        let period = std::f64::consts::TAU / moon.body.spin.rate;
        let orbital = el.period();
        assert!(
            (period / orbital - 1.0).abs() < 0.01,
            "{}: {period} vs {orbital}",
            moon.body.name
        );
    }
}

#[test]
fn everything_is_at_rest_at_the_barycentre() {
    let g = system();
    let total: f64 = g.majors.iter().map(|m| m.body.mu).sum();
    let centre = g
        .majors
        .iter()
        .fold(DVec3::ZERO, |s, m| s + m.x * m.body.mu)
        / total;
    let drift = g
        .majors
        .iter()
        .fold(DVec3::ZERO, |s, m| s + m.v * m.body.mu)
        / total;
    assert!(centre.length() < 1e-3, "{centre}");
    assert!(drift.length() < 1e-12, "{drift}");
}

#[test]
fn giants_have_trojans_and_the_belt_is_full() {
    let g = system();
    let giants = g
        .majors
        .iter()
        .filter(|m| m.body.kind == BodyKind::GasGiant)
        .count();
    let belt = 150;
    assert!(
        g.minors.len() >= belt + 2 * giants,
        "{} minors",
        g.minors.len()
    );
    assert!(g.minors.iter().all(|m| m.body.kind == BodyKind::Asteroid));
    for (i, m) in g.majors.iter().enumerate() {
        println!(
            "{i:2} {:<12} {:?} parent {:?}: r {:.0} km, g {:.2} m/s², at {:.3e} m",
            m.body.name,
            m.body.kind,
            g.parents[i],
            m.body.radius / 1e3,
            m.body.surface_gravity(),
            m.x.length()
        );
    }
}

#[test]
fn the_same_seed_gives_the_same_system() {
    let again = generate(&params()).expect("stable");
    assert_eq!(again.state_hash(), system().state_hash());
    let mut other = params();
    other.seed += 1;
    let different = generate(&other).expect("stable");
    assert_ne!(different.state_hash(), system().state_hash());
}

/// Cross-platform determinism of generation (random draws, Kepler, stability checks).
/// Generated on Windows x86-64; never regenerate it to make a failure go away.
#[test]
fn the_default_system_is_identical_on_every_platform() {
    let hash = system().state_hash();
    if std::env::var_os("CREEER_PRINT_GOLDEN").is_some() {
        println!("system golden hash: {hash:#018x}");
    }
    assert_eq!(hash, GOLDEN_SYSTEM_HASH, "got {hash:#018x}");
}

const GOLDEN_SYSTEM_HASH: u64 = 0x2d36_93a8_94bc_f51c;
