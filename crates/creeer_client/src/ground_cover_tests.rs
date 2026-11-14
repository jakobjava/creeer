//! The ground cover's cells: made the same every time, on the ground, never on dug ground.

use super::*;
use creeer_terrain::edits::{Brush, Edit};
use creeer_terrain::planet::Planet;

fn builder() -> Builder {
    let data = creeer_data::Data::base();
    let terrain = Arc::new(Terrain::new(
        Planet::load(&data, "planets/home.ron").expect("the home planet"),
    ));
    let palette = creeer_art::palette::Palette::from_data(&data).expect("palette");
    let decoratives = Ground::from_data(&data).expect("ground").decoratives;
    let colours = (decoratives.iter())
        .filter_map(|d| d.token.clone())
        .filter_map(|t| Some((t.clone(), palette.token(&t)?.shade.fixed()?.0)))
        .collect();
    Builder {
        step: CELL / terrain.planet().radius,
        terrain,
        look: Arc::new(GroundLook::from_data(&data).expect("look")),
        decoratives,
        colours,
    }
}

/// Where players appear: forest floor, with grit and tufts.
fn spawn_key(b: &Builder) -> (Key, DVec3) {
    let spawn = creeer_server::world::spawn_point(&b.terrain);
    (key_of(spawn, b.step), spawn)
}

#[test]
fn a_cell_is_the_same_every_time_and_on_the_ground() {
    let b = builder();
    let (key, _) = spawn_key(&b);
    let (one, two) = (b.cell(key), b.cell(key));
    assert!(!one.placed.is_empty(), "nothing stands at the spawn");
    assert_eq!(one.placed.len(), two.placed.len());
    for (p, q) in one.placed.iter().zip(&two.placed) {
        assert_eq!((p.offset, p.instance), (q.offset, q.instance));
        // Its root on the ground (the terrain's density there within a centimetre).
        let at = one.origin + p.offset.as_dvec3();
        assert!(b.terrain.density(at).abs() < 0.01, "{at}");
        assert!(p.instance.height <= 0.25 && p.instance.width <= 0.15);
    }
}

#[test]
fn nothing_stands_on_dug_ground() {
    let b = builder();
    let (key, spawn) = spawn_key(&b);
    let before = b.cell(key);
    // Off the cell's middle, where its rock is read: the rest of the cell keeps its cover.
    let (east, _) = creeer_terrain::geology::deposits::tangent_frame(before.origin.normalize());
    let centre = before.origin + east * 1.0;
    b.terrain.edit(Edit::Dig(Brush::Sphere {
        center: centre,
        radius: 1.5,
    }));
    let after = b.cell(key);
    let inside = |c: &Cell| {
        (c.placed.iter())
            .filter(|p| (c.origin + p.offset.as_dvec3() - centre).length() < 1.5)
            .count()
    };
    assert!(
        inside(&before) > 0,
        "nothing stood there to begin with ({spawn})"
    );
    assert_eq!(inside(&after), 0, "decoratives in a hole");
}
