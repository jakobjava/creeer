//! Items (ADR-0028): what fits where, bulk mixed with its mass, species and heat conserved, every
//! transfer conserving mass and species (random sequences), the codec, and the packs' pockets
//! and kit.

use super::kit::{Densities, describe, kit, pockets};
use super::*;
use creeer_data::Data;
use creeer_math::codec::{Reader, Writer};
use proptest::prelude::*;

fn stone(radius: f64) -> Item {
    Item::Piece(
        Piece::solid("granite", 2650.0, Form::Ball { radius }),
        Vec::new(),
    )
}

fn sand(mass: f64, t: f64) -> Bulk {
    Bulk {
        mass,
        composition: vec![("quartz".into(), 0.9), ("orthoclase".into(), 0.1)],
        temperature: t,
        specific_heat: 830.0,
        grain: 0.0005,
        bulk_density: 1600.0,
    }
}

fn ore(mass: f64, t: f64) -> Bulk {
    Bulk {
        mass,
        composition: vec![("chalcopyrite".into(), 0.02), ("quartz".into(), 0.98)],
        temperature: t,
        specific_heat: 600.0,
        grain: 0.01,
        bulk_density: 1800.0,
    }
}

#[test]
fn a_piece_fits_by_its_size_not_only_its_volume() {
    // A chest pocket: 9.5 × 11.5 × 3 cm.
    let mut pocket = Store::new("chest", [0.095, 0.115, 0.03]);
    // A 4 cm ball is wider than the pocket is deep, though it is a quarter of its volume.
    assert!(pocket.refuses(&stone(0.02)).is_some());
    // 3 cm pebbles: their boxes fill it twelve times.
    let mut fitted = 0;
    while pocket.put(stone(0.015)).is_ok() {
        fitted += 1;
    }
    assert_eq!(fitted, 12);
    assert!(
        (pocket.mass() - 12.0 * 2650.0 * 4.0 / 3.0 * PI * (0.015 * 0.015 * 0.015)).abs() < 1e-12
    );
}

#[test]
fn a_container_holds_what_fits_inside_its_walls() {
    let mut crate_ = Piece::solid(
        "spruce_c24",
        420.0,
        Form::Block {
            size: DVec3::new(0.5, 0.4, 0.4),
        },
    );
    crate_.wall = 0.018;
    let inside = crate_.inside().unwrap();
    assert!((inside[0] - 0.464).abs() < 1e-12 && (inside[2] - 0.364).abs() < 1e-12);
    // Its boards' mass: the outer block less its inside.
    let boards = 420.0 * (0.08 - 0.464 * 0.364 * 0.364);
    assert!((crate_.mass() - boards).abs() < 1e-12);
    let full = Item::Piece(crate_, vec![stone(0.04), Item::Bulk(sand(10.0, 290.0))]);
    assert!((full.mass() - boards - 10.0 - stone(0.04).mass()).abs() < 1e-12);
    assert_eq!(full.store().unwrap().items.len(), 2);
}

#[test]
fn mixing_bulk_keeps_mass_species_and_heat() {
    let (a, b) = (sand(3.0, 280.0), ore(1.0, 320.0));
    let mut mix = a.clone();
    mix.merge(&b);
    assert!((mix.mass - 4.0).abs() < 1e-15);
    let quartz = mix
        .composition
        .iter()
        .find(|(n, _)| n == "quartz")
        .unwrap()
        .1;
    assert!((quartz * 4.0 - (0.9 * 3.0 + 0.98 * 1.0)).abs() < 1e-14);
    let total: f64 = mix.composition.iter().map(|(_, f)| f).sum();
    assert!((total - 1.0).abs() < 1e-15);
    // The heat: m·c·T before and after.
    let heat = |x: &Bulk| x.mass * x.specific_heat * x.temperature;
    assert!((heat(&mix) - heat(&a) - heat(&b)).abs() < 1e-9 * heat(&mix));
    // The volumes add.
    assert!((mix.volume() - a.volume() - b.volume()).abs() < 1e-15);
}

#[test]
fn bulk_in_a_store_is_one_mix() {
    let mut pocket = Store::new("thigh", [0.115, 0.18, 0.06]);
    pocket.put(Item::Bulk(sand(0.5, 290.0))).unwrap();
    pocket.put(Item::Bulk(ore(0.5, 290.0))).unwrap();
    assert_eq!(pocket.items.len(), 1);
    // More than the 1.24 l left is refused, and given back.
    let (back, why) = *pocket.put(Item::Bulk(sand(3.0, 290.0))).unwrap_err();
    assert!(why.contains("no room"), "{why}");
    assert!((back.mass() - 3.0).abs() < 1e-15);
}

/// Every species' mass over a set of stores.
fn species(stores: &[Store]) -> Vec<(String, f64)> {
    let mut out = Vec::new();
    for item in stores.iter().flat_map(|s| &s.items) {
        item.species(&mut out);
    }
    out.sort_by(|a, b| a.0.cmp(&b.0));
    out
}

proptest! {
    /// Random moves between pockets and the ground (a store too large to refuse) conserve the
    /// mass and every species, to rounding; a refused move changes nothing.
    #[test]
    fn transfers_conserve_mass_and_species(moves in proptest::collection::vec((0usize..4, 0usize..4, 0usize..8), 1..80)) {
        let mut stores = vec![
            Store::new("ground", [100.0, 100.0, 100.0]),
            Store::new("chest", [0.095, 0.115, 0.03]),
            Store::new("thigh", [0.115, 0.18, 0.06]),
            Store::new("pouch", [0.10, 0.11, 0.04]),
        ];
        for k in 0..6 {
            stores[0].items.push(stone(0.005 + 0.004 * f64::from(k)));
        }
        stores[0].items.push(Item::Bulk(sand(0.4, 285.0)));
        stores[0].items.push(Item::Bulk(ore(0.3, 300.0)));
        let before = species(&stores);
        let mass: f64 = stores.iter().map(Store::mass).sum();
        for (from, to, which) in moves {
            if from == to { continue; }
            let n = stores[from].items.len().max(1);
            let Some(item) = stores[from].take(which % n) else { continue };
            if let Err(refused) = stores[to].put(item) {
                let (item, _) = *refused;
                stores[from].items.push(item);
            }
        }
        let after = species(&stores);
        prop_assert_eq!(before.len(), after.len());
        for ((n0, m0), (n1, m1)) in before.iter().zip(&after) {
            prop_assert_eq!(n0, n1);
            prop_assert!((m0 - m1).abs() < 1e-12 * m0.max(1.0));
        }
        let now: f64 = stores.iter().map(Store::mass).sum();
        prop_assert!((now - mass).abs() < 1e-12);
        for s in &stores[1..] {
            prop_assert!(s.used() <= s.volume() + 1e-12);
        }
    }
}

#[test]
fn items_and_stores_round_trip_bit_for_bit() {
    let mut pocket = Store::new("pouch", [0.10, 0.11, 0.04]);
    pocket.put(stone(0.012)).unwrap();
    pocket.put(Item::Bulk(ore(0.2, 301.5))).unwrap();
    let mut w = Writer::new();
    w.put(&pocket);
    let bytes = w.finish();
    let mut r = Reader::new(&bytes);
    let back: Store = r.get().unwrap();
    r.end().unwrap();
    assert_eq!(back, pocket);
}

fn data() -> Data {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../data");
    Data::open(&root, &[]).unwrap()
}

#[test]
fn the_outfit_has_its_modelled_pockets() {
    let outfit: Vec<String> = ["base_layer", "coverall", "jacket", "belt", "boots"]
        .map(String::from)
        .into();
    let pockets = pockets(&data(), &outfit).unwrap();
    let names: Vec<&str> = pockets.iter().map(|p| p.name.as_str()).collect();
    assert_eq!(
        names,
        [
            "coverall.thigh.L",
            "coverall.thigh.R",
            "jacket.chest.L",
            "jacket.chest.R",
            "belt.pouch"
        ]
    );
    // Three and a half litres in all.
    let litres: f64 = pockets.iter().map(Store::volume).sum::<f64>() * 1000.0;
    assert!((litres - 3.58).abs() < 0.01, "{litres}");
}

#[test]
fn the_creative_kit_is_made_from_the_packs_materials() {
    let (data, densities) = (data(), Densities::from_data(&data()).unwrap());
    let items: Vec<(String, Item)> = (kit(&data).unwrap().iter())
        .map(|k| (k.name.clone(), densities.make(k).unwrap()))
        .collect();
    let mass = |n: &str| items.iter().find(|(k, _)| k == n).unwrap().1.mass();
    assert!((mass("cobble") - 0.711).abs() < 0.001);
    assert!((mass("crate") - 7.78).abs() < 0.01);
    assert!(describe(&items[0].1).starts_with("granite, 8 cm across, 0.71 kg"));
    // A rock found by its name, where no engineering material has it.
    assert!(densities.of("basalt").is_some_and(|d| d > 2500.0));
    assert!(densities.of("unobtainium").is_none());
}
