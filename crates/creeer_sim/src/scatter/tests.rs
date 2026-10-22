//! What lies loose (ADR-0029): the same on every call, each found again by its id, as many per
//! square metre as the rules say (to Poisson's spread), sizes in fragmentation's law, lying on
//! the ground, nodules in their share, and an item of the rock's density.

use super::*;
use crate::items::kit::Densities;
use creeer_terrain::Planet;

fn data() -> Data {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../data");
    Data::open(&root, &[]).unwrap()
}

fn terrain(data: &Data) -> Terrain {
    Terrain::new(Planet::load(data, "planets/home.ron").unwrap())
}

/// Land at a temperate latitude, as the spawn is.
fn land(terrain: &Terrain) -> DVec3 {
    let up = creeer_terrain::elevation::fibonacci_sphere(5000)
        .filter(|d| (0.5..0.7).contains(&d.y.abs()))
        .find(|&d| (300.0..1500.0).contains(&terrain.surface_height(d)))
        .unwrap();
    on_ground(terrain, up)
}

/// The cells about `p` and what lies in them, with their areas (m²) and ground rocks.
fn around(
    scatter: &Scatter,
    terrain: &Terrain,
    p: DVec3,
    r: f64,
) -> Vec<(f64, Vec<Loose>, Option<u16>)> {
    let radius = terrain.planet().radius;
    (scatter.cells_near(p, r).into_iter())
        .map(|cell| {
            let corner = |i: i32, j: i32| {
                CubeCoord {
                    face: cell.face,
                    s: f64::from(i) * scatter.step,
                    t: f64::from(j) * scatter.step,
                }
                .direction()
            };
            let (a, b, c) = (
                corner(cell.i, cell.j),
                corner(cell.i + 1, cell.j),
                corner(cell.i, cell.j + 1),
            );
            let area = (b - a).cross(c - a).length() * radius * radius;
            let mid = CubeCoord {
                face: cell.face,
                s: (f64::from(cell.i) + 0.5) * scatter.step,
                t: (f64::from(cell.j) + 0.5) * scatter.step,
            };
            let ground = on_ground(terrain, mid.direction());
            let rock = terrain.rock_at(ground - ground.normalize() * 0.3);
            (area, scatter.in_cell(terrain, cell), rock)
        })
        .collect()
}

#[test]
fn the_same_everywhere_and_each_found_by_its_id() {
    let data = data();
    let terrain = terrain(&data);
    let scatter = Scatter::from_data(&data, &terrain).unwrap();
    let p = land(&terrain);
    let cells = scatter.cells_near(p, 20.0);
    assert!(cells.len() > 10, "{}", cells.len());
    let mut count = 0;
    for &cell in &cells {
        let once = scatter.in_cell(&terrain, cell);
        assert_eq!(once, scatter.in_cell(&terrain, cell));
        for loose in &once {
            assert_eq!(scatter.find(&terrain, loose.id), Some(*loose));
            assert_eq!(scatter.cell_of(loose.centre), cell, "in its own cell");
        }
        count += once.len();
    }
    assert!(count > 0, "something lies about");
}

#[test]
fn stones_lie_on_the_ground_as_many_and_as_large_as_the_rules_say() {
    let data = data();
    let terrain = terrain(&data);
    let scatter = Scatter::from_data(&data, &terrain).unwrap();
    let p = land(&terrain);
    let (mut area, mut stones, mut sizes) = (0.0, 0usize, Vec::new());
    let soil = RockTable::from_data(&data).unwrap().rock("soil").unwrap();
    let rule = scatter.stones[usize::from(soil)].clone().unwrap();
    for (a, loose, ground) in around(&scatter, &terrain, p, 60.0) {
        // (The cells of soil ground: the spawn's.)
        if ground != Some(soil) {
            continue;
        }
        area += a;
        for l in &loose {
            let Lying::Stone { size, .. } = l.what else {
                continue;
            };
            stones += 1;
            sizes.push(size.x);
            // Its underside on the ground, within a centimetre.
            let bottom = l.centre - l.centre.normalize() * (size.z * 0.5);
            assert!(
                terrain.density(bottom).abs() < 0.01,
                "{}",
                terrain.density(bottom)
            );
            assert!(size.x >= size.y && size.y >= size.z);
        }
    }
    #[allow(clippy::cast_precision_loss)]
    let (n, mean) = (stones as f64, rule.per_m2 * area);
    assert!(area > 5000.0, "{area} m² of soil");
    assert!(
        (n - mean).abs() < 4.0 * mean.sqrt(),
        "{n} stones, {mean} expected"
    );
    // Fragmentation's law: the share over twice the smallest, P(d > 2s) = (¼ − (s/L)²)/(1 − (s/L)²).
    let (s, l) = (scatter.smallest, rule.largest);
    let want = (0.25 - (s / l) * (s / l)) / (1.0 - (s / l) * (s / l));
    #[allow(clippy::cast_precision_loss)]
    let got = sizes.iter().filter(|&&d| d > 2.0 * s).count() as f64 / n;
    assert!(
        (got - want).abs() < 4.0 * (want * (1.0 - want) / n).sqrt(),
        "{got} against {want}"
    );
    assert!(sizes.iter().all(|&d| d >= s && d <= l));
}

#[test]
fn nodules_come_in_their_share() {
    let data = data();
    let terrain = terrain(&data);
    let mut scatter = Scatter::from_data(&data, &terrain).unwrap();
    let rocks = RockTable::from_data(&data).unwrap();
    let chert = rocks.rock("chert").unwrap();
    // As if every stone's host had chert in it, at a third.
    for rule in scatter.stones.iter_mut().flatten() {
        rule.nodules = vec![(chert, 1.0 / 3.0)];
    }
    let (mut all, mut nodules) = (0usize, 0usize);
    for (_, loose, _) in around(&scatter, &terrain, land(&terrain), 60.0) {
        for l in loose {
            if let Lying::Stone { rock, .. } = l.what {
                all += 1;
                nodules += usize::from(rock == chert);
            }
        }
    }
    #[allow(clippy::cast_precision_loss)]
    let (share, n) = (nodules as f64 / all as f64, all as f64);
    assert!(
        (share - 1.0 / 3.0).abs() < 4.0 * (2.0 / 9.0 / n).sqrt(),
        "{share} of {all}"
    );
}

#[test]
fn a_stone_picked_up_is_its_rock_at_its_density() {
    let data = data();
    let terrain = terrain(&data);
    let scatter = Scatter::from_data(&data, &terrain).unwrap();
    let (rocks, densities) = (
        RockTable::from_data(&data).unwrap(),
        Densities::from_data(&data).unwrap(),
    );
    let all: Vec<Loose> = (around(&scatter, &terrain, land(&terrain), 30.0).into_iter())
        .flat_map(|(_, l, _)| l)
        .collect();
    let stone = all
        .iter()
        .find(|l| matches!(l.what, Lying::Stone { .. }))
        .unwrap();
    let Lying::Stone { rock, size } = stone.what else {
        unreachable!()
    };
    let item = scatter.item(stone, &rocks, &densities).unwrap();
    let r = &rocks.rocks[usize::from(rock)];
    let volume = std::f64::consts::PI / 6.0 * size.x * size.y * size.z;
    assert!((item.mass() - r.density * volume).abs() < 1e-12 * item.mass());
    let branch = all.iter().find(|l| matches!(l.what, Lying::Branch { .. }));
    if let Some(b) = branch {
        assert!(scatter.item(b, &rocks, &densities).unwrap().mass() > 0.0);
    }
}
