use crate::Planet;
use crate::elevation::fibonacci_sphere;
use crate::tiles::TileKey;
use creeer_materials::elements::atomic_number;
use std::collections::BTreeMap;
use std::sync::OnceLock;

fn home() -> &'static Planet {
    static PLANET: OnceLock<Planet> = OnceLock::new();
    PLANET.get_or_init(|| Planet::load(&crate::data_path("planets/home.ron")).expect("loads"))
}

#[test]
fn computed_rock_densities_match_measured_ones() {
    // Typical bulk densities (kg/m³) from rock-physics tables (e.g. Schön 2015).
    let rocks = &home().geology.rocks;
    for (name, lo, hi) in [
        ("granite", 2600.0, 2750.0),
        ("basalt", 2750.0, 3050.0),
        ("gabbro", 2850.0, 3100.0),
        ("peridotite", 3150.0, 3350.0),
        ("sandstone", 2100.0, 2450.0),
        ("shale", 2300.0, 2600.0),
        ("limestone", 2300.0, 2600.0),
        ("marble", 2650.0, 2750.0),
        ("schist", 2700.0, 3000.0),
        ("gneiss", 2650.0, 2850.0),
        ("soil", 1300.0, 1600.0),
        ("rock salt", 2100.0, 2250.0),
        ("coal", 1200.0, 1500.0),
        ("banded iron formation", 3300.0, 3900.0),
    ] {
        let rock = &rocks.rocks[usize::from(rocks.rock(name).expect(name))];
        assert!(
            (lo..=hi).contains(&rock.density),
            "{name}: {} kg/m³",
            rock.density
        );
    }
}

#[test]
fn ore_grades_are_realistic() {
    // Mass fractions of the payable element at a deposit's average grade.
    let rocks = &home().geology.rocks;
    let grade = |rock: &str, element: &str| {
        rocks.element_fraction(
            rocks.rock(rock).expect(rock),
            atomic_number(element).expect(element),
        )
    };
    let cu = grade("porphyry copper ore", "Cu");
    assert!((0.003..0.012).contains(&cu), "porphyry Cu {cu}");
    let mo = grade("porphyry copper ore", "Mo");
    assert!((0.0002..0.001).contains(&mo), "porphyry Mo {mo}");
    let fe = grade("banded iron formation", "Fe");
    assert!((0.35..0.55).contains(&fe), "BIF Fe {fe}");
    let ni = grade("nickel sulfide ore", "Ni");
    assert!((0.008..0.025).contains(&ni), "Ni sulfide Ni {ni}");
    let u = grade("uranium sandstone", "U");
    assert!((0.0005..0.004).contains(&u), "sandstone U {u}");
    let al = grade("laterite bauxite", "Al");
    assert!((0.22..0.35).contains(&al), "bauxite Al {al}");
    // Placer gold in grams per tonne.
    let au = grade("gold placer gravel", "Au") * 1e6;
    assert!((0.3..5.0).contains(&au), "placer Au {au} g/t");
    let c = grade("coal", "C");
    assert!((0.55..0.85).contains(&c), "coal C {c}");
}

#[test]
fn every_province_occurs_and_ocean_crust_lies_deep() {
    let planet = home();
    let g = &planet.geology;
    let mut counts = vec![0usize; g.provinces.len()];
    for up in fibonacci_sphere(6000) {
        let h = g.elevation_at(up);
        let province = g.province_at(up, h);
        counts[province] += 1;
        if g.provinces[province].name == "oceanic crust" {
            assert!(h < -1000.0, "oceanic crust at {h} m");
        }
    }
    for (p, &n) in g.provinces.iter().zip(&counts) {
        assert!(n > 30, "province {} covers only {n} of 6000 points", p.name);
    }
}

#[test]
fn columns_run_from_the_top_layer_down_to_the_basement() {
    let g = &home().geology;
    for (i, province) in g.provinces.iter().enumerate() {
        let p = creeer_math::glam::DVec3::new(0.3, 0.7, -0.6).normalize() * 500_000.0;
        let order: Vec<u16> = province.column.iter().map(|l| l.rock).collect();
        assert_eq!(g.column_rock(i, p, 0.0, 1.0), order[0], "{}", province.name);
        assert_eq!(
            g.column_rock(i, p, 20_000.0, 1.0),
            *order.last().expect("layers")
        );
        if province.folds.is_none() {
            // Walking down meets the layers in their order.
            let mut position = 0;
            for step in 0..4000 {
                let rock = g.column_rock(i, p, f64::from(step), 1.0);
                while order[position] != rock {
                    position += 1;
                    assert!(position < order.len(), "{}: out of order", province.name);
                }
            }
        }
    }
}

#[test]
fn every_deposit_kind_occurs_where_its_rules_allow() {
    let planet = home();
    let g = &planet.geology;
    let mut found: BTreeMap<&str, usize> = BTreeMap::new();
    for (kind, k) in g.deposits.iter().enumerate() {
        let kind = u16::try_from(kind).expect("few");
        let n = 1u32 << k.level;
        for face in 0..6 {
            for i in 0..n {
                for j in 0..n {
                    let key = TileKey {
                        face,
                        level: k.level,
                        i,
                        j,
                    };
                    let Some(d) = g.generate_deposit(kind, key) else {
                        continue;
                    };
                    *found.entry(&k.name).or_default() += 1;
                    let up = d.center.normalize();
                    let h = g.elevation_at(up);
                    assert!(k.provinces.contains(&g.province_at(up, h)), "{}", k.name);
                    assert!(h >= k.elevation.0 && h <= k.elevation.1, "{}", k.name);
                    let depth = planet.datum.radius(up) + h - d.center.length();
                    assert!(depth >= k.depth.0 - 1e-6 && depth <= k.depth.1 + 1e-6);
                    // The core is ore at the top grade; well outside is not.
                    let core = g.sample(d.center, h, &[d], 1.0);
                    assert_eq!(core.rock, k.rock, "{}", k.name);
                    let grade = core.deposit.expect("inside").grade;
                    assert!((grade - 2.5).abs() < 1e-9);
                    let outside = d.center + d.axes[2] * (d.radii.z * 1.6);
                    assert!(d.hit(outside, g).is_none(), "{}", k.name);
                }
            }
        }
    }
    println!("deposits on the planet: {found:?}");
    for k in &g.deposits {
        assert!(
            found.get(k.name.as_str()).copied().unwrap_or(0) > 0,
            "no {}",
            k.name
        );
    }
}

#[test]
fn deposits_near_finds_the_deposit_around_a_point() {
    let g = &home().geology;
    // Find some deposit, then look it up from a point inside it, near its rim.
    let k = g
        .deposits
        .iter()
        .position(|k| k.name == "coal seam")
        .expect("coal");
    let kind = u16::try_from(k).expect("few");
    let n = 1u32 << g.deposits[k].level;
    let d = (0..n * n)
        .find_map(|c| {
            g.generate_deposit(
                kind,
                TileKey {
                    face: 2,
                    level: g.deposits[k].level,
                    i: c % n,
                    j: c / n,
                },
            )
        })
        .expect("a coal seam on face +Y");
    let p = d.center + d.axes[0] * (0.7 * d.radii.x);
    let near = g.deposits_near(p, 0.0);
    assert!(near.contains(&d), "the seam itself is found");
}

#[test]
fn geology_is_bit_identical_across_platforms() {
    let planet = home();
    let g = &planet.geology;
    let mut hash = 0xcbf2_9ce4_8422_2325u64;
    for up in fibonacci_sphere(200) {
        let h = g.elevation_at(up);
        for depth in [0.5, 5.0, 60.0, 400.0, 2500.0] {
            let s = g.sample_at(up * (planet.datum.radius(up) + h - depth), h);
            for b in s.rock.to_le_bytes() {
                hash = (hash ^ u64::from(b)).wrapping_mul(0x0100_0000_01b3);
            }
        }
    }
    assert_eq!(hash, 0xfecb_5508_b282_88eb, "geology hash {hash:#018x}");
}
