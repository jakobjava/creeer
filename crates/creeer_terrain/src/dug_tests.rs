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
fn a_sphere_is_found_that_takes_no_more_than_was_dug() {
    let terrain = home();
    let at = surface(&terrain);
    let field = Field::around(&terrain, at, 1.6, 0.1);
    let least = field.removed(&Brush::Sphere {
        center: at,
        radius: 0.5,
    });
    assert!(field.sphere_for(at, 0.9 * least, (0.5, 0.8)).is_none());
    let want = 1.3 * least;
    let t = Instant::now();
    let (r, took) = field.sphere_for(at, want, (0.5, 0.8)).expect("enough");
    let spent = t.elapsed();
    assert!(r > 0.5 && r < 0.8);
    assert!(took <= want && took > 0.999 * want, "{took} of {want}");
    // Timed, not gated: shared CI machines are slow and noisy (ADR-0032's Cost measures it).
    println!("the ball found in {spent:?}");
}
