use super::*;
use crate::Planet;
use std::f64::consts::PI;
use std::time::Instant;

fn home() -> Terrain {
    Terrain::new(Planet::load(&creeer_data::Data::base(), "planets/home.ron").expect("loads"))
}

/// A point on the surface somewhere on the planet.
fn surface(terrain: &Terrain) -> DVec3 {
    let up = DVec3::new(0.31, 0.82, -0.48).normalize();
    up * (terrain.planet().datum.radius(up) + terrain.surface_height(up))
}

#[test]
fn a_small_pit_takes_about_its_half_ball_and_grows_with_its_brush() {
    let terrain = home();
    let at = surface(&terrain);
    let field = Field::around(&terrain, at, 1.6, 0.05);
    let half = |r: f64| 2.0 / 3.0 * PI * r * r * r;
    let mut last = 0.0;
    for r in [0.5, 0.6, 0.7, 0.8] {
        let v = field.removed(&Brush::Sphere {
            center: at,
            radius: r,
        });
        // The half ball under the surface, and its rounded rim (at most as wide as itself).
        assert!(v > 0.8 * half(r) && v < 2.5 * half(r), "r {r}: {v} m³");
        assert!(v > last);
        last = v;
    }
    // A brush in the air takes nothing.
    let up = at.normalize();
    let air = Brush::Sphere {
        center: at + up * 3.0,
        radius: 0.5,
    };
    assert_eq!(field.removed(&air), 0.0);
}

#[test]
fn the_field_measures_what_a_dig_really_removes() {
    // The field after the cut, sampled anew, holds what the measure said it would lose.
    let terrain = home();
    let at = surface(&terrain);
    let brush = Brush::Sphere {
        center: at,
        radius: 0.6,
    };
    let before = Field::around(&terrain, at, 1.6, 0.05);
    let predicted = before.removed(&brush);
    let rock = |f: &Field| f.samples.iter().map(|&(_, d)| rock(d, f.step)).sum::<f64>();
    terrain.edit(Edit::Dig(brush));
    let after = Field::around(&terrain, at, 1.6, 0.05);
    let lost = (rock(&before) - rock(&after)) * 0.05 * 0.05 * 0.05;
    assert!((lost - predicted).abs() < 1e-9, "{lost} vs {predicted}");
}

#[test]
fn a_growing_shaft_is_found_that_takes_what_was_dug() {
    let terrain = home();
    let at = surface(&terrain);
    let up = at.normalize();
    let field = Field::around(&terrain, at, 1.2, 0.03);
    // A shaft 15 cm in radius, from a dent to a metre deep.
    let shaft = |t: f64| Brush::Capsule {
        a: at,
        b: at - up * t,
        radius: 0.15,
    };
    for want in [0.02, 0.05, 0.08] {
        let t0 = Instant::now();
        let (depth, took) = field.grown_for(want, (shaft, false), (0.0, 1.0));
        let spent = t0.elapsed();
        assert!((took - want).abs() <= 1e-3 * want, "{took} of {want}");
        // About its cylinder below the surface: πr²·depth plus its half-ball ends and rim.
        let cylinder = PI * 0.15 * 0.15 * depth;
        assert!(
            depth > 0.0 && cylinder < want,
            "{depth} m deep for {want} m³"
        );
        // Timed, not gated: shared CI machines are slow and noisy.
        println!("{want} m³: {depth:.3} m deep, found in {spent:?}");
    }
    // More than its largest takes: the largest.
    let (depth, _) = field.grown_for(10.0, (shaft, false), (0.0, 1.0));
    assert_eq!(depth, 1.0);
}

/// Measured without a spot's own brush, the field is the ground before it: its brush grown in
/// place takes from that ground, not twice.
#[test]
fn a_field_without_a_spots_brush_is_the_ground_before_it() {
    let terrain = home();
    let at = surface(&terrain);
    let field = |w| Field::in_box_without(&terrain, at, (DVec3::splat(0.8), 0.04), w).volume();
    let before = field(None);
    terrain.edit(Edit::Dig(Brush::Sphere {
        center: at,
        radius: 0.3,
    }));
    let without = field(Some(0));
    let with = field(None);
    assert_eq!(without, before);
    assert!(with < before);
}

/// Measured only where a grown hole reaches beyond what it was, a blow takes what the whole
/// field says it does (on the same grid, to rounding).
#[test]
fn a_hole_grown_is_measured_where_it_grows() {
    let terrain = home();
    let at = surface(&terrain);
    let up = at.normalize();
    let shaft = |depth: f64| Brush::Capsule {
        a: at,
        b: at - up * depth,
        radius: 0.15,
    };
    let (from, to) = (shaft(0.4), shaft(0.45));
    let (centre, half, step) = (at - up * 0.3, DVec3::new(0.6, 0.6, 0.9), 0.03);
    let whole = Field::in_box_without(&terrain, centre, (half, step), None);
    let beyond = Field::beyond_without(&terrain, (from, to), (centre, half, step), None);
    let expected = whole.removed(&to) - whole.removed(&from);
    let measured = beyond.removed_beyond(Some(&from), &to);
    assert!(
        (measured - expected).abs() < 1e-12,
        "{measured} vs {expected}"
    );
    // Its floor alone: a few hundredths of the samples.
    assert!(beyond.samples.len() * 10 < whole.samples.len());
}
