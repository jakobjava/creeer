//! The events against what the physics says of them.

use super::*;
use crate::happenings::{Underfoot, creak, footstep};
use crate::voice::Excite;

fn lib() -> Library {
    Library::base()
}

#[test]
fn a_box_is_rung_along_its_longest_side_wherever_that_lies() {
    let lib = lib();
    let spruce = lib.matter("spruce_c24").unwrap();
    // A plank lying along z.
    let plank = Body::boxed(
        spruce,
        DVec3::new(0.075, 0.025, 1.0),
        (DVec3::ZERO, DQuat::IDENTITY),
        None,
    );
    assert!(
        (plank.turn * DVec3::X).z.abs() > 0.999,
        "its length along z"
    );
    // Struck at one end, its bending (not its stretching) is rung by a blow across it.
    let across = plank.modes(DVec3::new(0.0, 0.0, 0.95), DVec3::new(0.0, -5.0, 0.0), 1e-4);
    let along = plank.modes(DVec3::new(0.0, 0.0, 0.95), DVec3::new(0.0, 0.0, -5.0), 1e-4);
    let lowest = |m: &Modes| m.as_slice().iter().map(|m| m.freq).fold(f32::MAX, f32::min);
    // Across: bending at tens of Hz; along: stretching at c/2L ≈ 1.28 kHz.
    assert!(lowest(&across) < 200.0, "{}", lowest(&across));
    assert!((lowest(&along) - 1279.0).abs() < 5.0, "{}", lowest(&along));
}

#[test]
fn a_stone_clicks_on_rock_and_thuds_on_soil() {
    let lib = lib();
    let granite = lib.matter("granite").unwrap();
    let stone = Body::boxed(
        granite,
        DVec3::new(0.04, 0.03, 0.02),
        (DVec3::ZERO, DQuat::IDENTITY),
        None,
    );
    let j = DVec3::new(0.0, 2.0, 0.0);
    let rock = impact(
        &stone,
        &Other::Ground(lib.ground("granite")),
        DVec3::ZERO,
        j,
        1,
    );
    let soil = impact(
        &stone,
        &Other::Ground(lib.ground("soil")),
        DVec3::ZERO,
        j,
        1,
    );
    // Rock is stiffer: a contact over ten times shorter (Hertz: 0.2 ms against 3.7), a click far
    // louder.
    assert!(
        rock.click.contact * 10.0 < soil.click.contact,
        "{} {}",
        rock.click.contact,
        soil.click.contact
    );
    assert!(rock.click.reach() > 10.0 * soil.click.reach());
    assert!(rock.click.baffled && soil.click.baffled);
}

#[test]
fn a_nail_rings_higher_with_every_blow_and_the_last_seats_it() {
    let lib = lib();
    let driver = lib.driver("nail_75").unwrap();
    let wood = lib.matter("spruce_green").unwrap();
    let log = Body::member(
        wood,
        (DVec3::new(-1.0, 0.0, 0.0), DVec3::new(1.0, 0.0, 0.0)),
        0.1,
    );
    let nail = (lib.matter("steel_s235").unwrap(), 0.0031, 0.075);
    let blow = |k| {
        driven(
            &lib,
            driver,
            nail,
            &log,
            (DVec3::new(0.0, 0.1, 0.0), DVec3::NEG_Y),
            k,
            7,
        )
    };
    // The nail's own fundamental: 1.875²/(2πL²)·√(E/ρ)·d/4 for the length standing out.
    let fundamental = |standing: f64| {
        1.875_104f64.powi(2) / (2.0 * std::f64::consts::PI * standing * standing)
            * (210e9f64 / 7850.0).sqrt()
            * 0.0031
            / 4.0
    };
    for k in 0..driver.blows - 1 {
        let standing = 0.075 * (1.0 - f64::from(k) / f64::from(driver.blows));
        let want = fundamental(standing);
        let has = blow(k)
            .modes
            .as_slice()
            .iter()
            .any(|m| (f64::from(m.freq) - want).abs() < 0.005 * want);
        assert!(has, "blow {k}: no mode near {want:.0} Hz");
    }
    // The last blow: no nail standing out, a softer contact (steel on the wood's face).
    let last = blow(driver.blows - 1);
    assert!(last.click.contact > blow(0).click.contact);
}

#[test]
fn a_member_creaks_only_past_its_onset_and_more_as_it_nears_breaking() {
    let lib = lib();
    let wood = lib.matter("spruce_c24").unwrap();
    let plank = Body::boxed(
        wood,
        DVec3::new(1.5, 0.025, 0.1),
        (DVec3::ZERO, DQuat::IDENTITY),
        None,
    );
    assert!(creak(&plank, 0.4, 1, 1).is_none());
    let rate = |u: f64| match creak(&plank, u, 1, 1).map(|s| s.excite) {
        Some(Excite::Creak { rate, level }) => (rate, level),
        _ => (0.0, 0.0),
    };
    let (half, nearly) = (rate(0.7), rate(0.95));
    assert!(
        nearly.0 > 5.0 * half.0 && nearly.1 > half.1,
        "{half:?} {nearly:?}"
    );
}

#[test]
fn footsteps_crunch_on_gravel_and_knock_on_a_log() {
    let lib = lib();
    let at = (DVec3::ZERO, DVec3::NEG_Y);
    let [_, gravel] = footstep(&lib, &Underfoot::Ground(lib.ground("gravel")), at, 1.0, 1);
    assert!(matches!(
        gravel.map(|g| g.excite),
        Some(Excite::Grains { .. })
    ));
    let log = Body::member(
        lib.matter("spruce_green").unwrap(),
        (DVec3::new(-2.0, -0.15, 0.0), DVec3::new(2.0, -0.15, 0.0)),
        0.15,
    );
    let [thud, none] = footstep(&lib, &Underfoot::Body(log), at, 1.0, 1);
    assert!(none.is_none());
    let thud = thud.unwrap();
    assert!(thud.modes.as_slice().len() > 4, "the log's modes rung");
    // A firmer step is louder.
    let [soft, _] = footstep(&lib, &Underfoot::Ground(lib.ground("soil")), at, 0.5, 1);
    let [firm, _] = footstep(&lib, &Underfoot::Ground(lib.ground("soil")), at, 1.5, 1);
    assert!(firm.unwrap().peak > soft.unwrap().peak);
}
