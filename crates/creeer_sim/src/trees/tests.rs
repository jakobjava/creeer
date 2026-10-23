//! Trees grown and placed (ADR-0031): every species at every size stands under its own weight and
//! below its buckling height, its crown's mass is its biomass, its branches share the stem's
//! section (Leonardo), the same seed grows the same tree, trees stand where their ranges allow and
//! are found by their ids, and a species added as data only grows.

use super::*;
use crate::scatter::cells::on_ground;
use creeer_terrain::Planet;

pub(super) fn repository(path: &str) -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join(path)
}

pub(super) fn data() -> Data {
    Data::open(&repository("data"), &[]).unwrap()
}

pub(super) fn terrain(data: &Data) -> Terrain {
    Terrain::new(Planet::load(data, "planets/home.ron").unwrap())
}

pub(super) fn forest() -> (Forest, Terrain) {
    let data = data();
    let terrain = terrain(&data);
    (Forest::from_data(&data, &terrain).unwrap(), terrain)
}

/// Every species at sizes across its range, with seeds.
fn every_tree(forest: &Forest) -> Vec<(usize, Tree)> {
    let mut out = Vec::new();
    for (s, species) in forest.species.iter().enumerate() {
        for k in 0..=8 {
            let d = species.dbh.0 + (species.dbh.1 - species.dbh.0) * f64::from(k) / 8.0;
            for seed in 0..6 {
                out.push((s, Tree::grow(species, seed * 7919 + 13, d)));
            }
        }
    }
    out
}

#[test]
fn every_species_at_every_size_stands_under_its_own_weight() {
    let (forest, _) = forest();
    let mut most: f64 = 0.0;
    for (s, tree) in every_tree(&forest) {
        let used = tree.self_weight(&forest.species[s], DVec3::ZERO);
        assert!(
            used < 1.0,
            "{} {:.2} m: used {used:.2}",
            forest.species[s].name,
            tree.dbh
        );
        most = most.max(used);
    }
    // And a fresh breeze (10 m/s) breaks none; trees break near 42 m/s (Virot et al. 2016).
    for (s, tree) in every_tree(&forest) {
        let used = tree.self_weight(&forest.species[s], DVec3::new(10.0, 0.0, 0.0));
        assert!(
            used < 1.0,
            "{} {:.2} m in wind: {used:.2}",
            forest.species[s].name,
            tree.dbh
        );
    }
    println!("most used under own weight: {most:.2}");
}

#[test]
fn none_stands_taller_than_its_buckling_height() {
    // Greenhill's height for a column under its own weight, as McMahon (1973) compared trees
    // with it: 0.79·(E/ρg)^⅓·D^⅔; trees stand at about a quarter to a half of it.
    let (forest, _) = forest();
    let (mut least, mut most) = (f64::MAX, 0.0f64);
    for (s, tree) in every_tree(&forest) {
        let sp = &forest.species[s];
        let critical = 0.79
            * creeer_math::det::cbrt(sp.youngs_modulus / (sp.density * G))
            * creeer_math::det::pow(tree.dbh, 2.0 / 3.0);
        let ratio = tree.height / critical;
        assert!(ratio < 1.0, "{} {:.2} m: {ratio:.2}", sp.name, tree.dbh);
        least = least.min(ratio);
        most = most.max(ratio);
    }
    println!("heights over buckling heights: {least:.2}–{most:.2}");
}

#[test]
fn a_crowns_mass_is_its_biomass_and_its_branches_share_the_stems_section() {
    let (forest, _) = forest();
    let spruce = &forest.species[0];
    let tree = Tree::grow(spruce, 1, 0.3);
    // Repola 2009 at 30 cm and this height, fresh.
    let h = tree.height;
    let wood = spruce.branch_mass.of(30.0, h) * spruce.fresh;
    let foliage = spruce.foliage_mass.of(30.0, h) * spruce.fresh;
    let all = tree.nodes.iter().flat_map(|n| n.branches.iter());
    let (w, f) = all.fold((0.0, 0.0), |(w, f), b| (w + b.wood, f + b.foliage));
    assert!((w - wood).abs() < 1e-9 * wood && (f - foliage).abs() < 1e-9 * foliage);
    // Needles of a 30 cm, 25 m spruce: about 33 kg dry (Repola 2009, eq. for needles).
    let dry = spruce.foliage_mass.of(30.0, 25.0);
    assert!((dry - 33.0).abs() < 1.5, "{dry}");
    // Leonardo: the branches at a node have the section the stem loses across it.
    let node = &tree.nodes[3];
    let spacing = spruce.branching.spacing;
    let (rb, ra) = (
        tree.radius(node.z - 0.5 * spacing),
        tree.radius(node.z + 0.5 * spacing),
    );
    let lost = rb * rb - ra * ra;
    let branches: f64 = node.branches.iter().map(|b| b.radius * b.radius).sum();
    assert!((branches - lost).abs() < 1e-12, "{branches} {lost}");
    println!(
        "spruce 30 cm: {h:.1} m, {} branches, crown {:.0} kg fresh, stem {:.0} kg",
        tree.branch_count(),
        w + f,
        tree.stem(0.0, h).0
    );
}

#[test]
fn the_same_seed_grows_the_same_tree() {
    let (forest, _) = forest();
    for species in &forest.species {
        assert_eq!(Tree::grow(species, 42, 0.2), Tree::grow(species, 42, 0.2));
        assert_ne!(Tree::grow(species, 42, 0.2), Tree::grow(species, 43, 0.2));
    }
}

/// Land at a temperate latitude, as the spawn is.
pub(super) fn land(terrain: &Terrain) -> DVec3 {
    let up = creeer_terrain::elevation::fibonacci_sphere(5000)
        .filter(|d| (0.5..0.7).contains(&d.y.abs()))
        .find(|&d| (300.0..1500.0).contains(&terrain.surface_height(d)))
        .unwrap();
    on_ground(terrain, up)
}

#[test]
fn trees_stand_where_they_grow_each_found_by_its_id() {
    let (forest, terrain) = forest();
    let p = land(&terrain);
    let (mut count, mut area, mut hash) = (0usize, 0.0, 0u64);
    for cell in forest.cells_near(p, 60.0) {
        area += forest.cells.area(cell, terrain.planet().radius);
        let trees = forest.in_cell(&terrain, cell);
        assert_eq!(trees, forest.in_cell(&terrain, cell), "the same twice");
        for t in &trees {
            assert_eq!(forest.find(&terrain, t.id), Some(*t));
            let up = t.base.normalize();
            assert!((t.turn * DVec3::Z - up).length() < 1e-9, "upright");
            // On the ground.
            assert!(
                terrain.density(t.base).abs() < 0.05,
                "{}",
                terrain.density(t.base)
            );
            let sp = &forest.species[usize::from(t.species)];
            assert!((sp.dbh.0..=sp.dbh.1).contains(&t.dbh));
            hash = hash.rotate_left(7) ^ t.id ^ t.dbh.to_bits();
            count += 1;
        }
        for (a, b) in trees.iter().zip(trees.iter().skip(1)) {
            assert_ne!(a.id, b.id);
        }
    }
    println!(
        "{count} trees on {area:.0} m² ({:.3}/m²), hash {hash:#x}",
        count as f64 / area
    );
    assert!(count > 0, "a temperate forest");
}

#[test]
fn a_species_added_as_data_only_grows_and_stands() {
    let data = Data::open(&repository("data"), &[repository("tests/packs")]).unwrap();
    let terrain = terrain(&data);
    let forest = Forest::from_data(&data, &terrain).unwrap();
    let oak = (forest.species.iter())
        .find(|s| s.name == "oak")
        .expect("the mod's oak");
    assert_eq!(oak.wood, "oak_green");
    for d in [0.1, 0.4, 0.8] {
        let tree = Tree::grow(oak, 5, d);
        assert!(tree.branch_count() > 3);
        assert!(tree.self_weight(oak, DVec3::ZERO) < 1.0);
    }
    assert_eq!(forest.species.len(), 4);
}
