//! Clients are told where the hands hold (G2.10, ADR-0036): the point grabbed, as it moves
//! along a pole hand over hand (rate-limited), and one hand while the other fastens.

use super::raising_tests::{self, ada_and_pole, grab_the_top};
use super::tacking_tests::{across, held_where_it_lies};
use super::*;
use creeer_net::message::Target;

/// What clients are told `ada` holds.
fn told(w: &ServerWorld, ada: EntityId) -> Option<creeer_net::entity::Carried> {
    match w.drawn(w.index(ada).expect("there")) {
        looks::Drawn::Person { carries, .. } => carries,
        _ => None,
    }
}

#[test]
fn the_grip_is_told_where_it_was_taken() {
    // A 2.5 kg pole taken where another crosses it, near its middle: in one hand.
    let (mut w, ada, part, _, at) = across();
    let target = Target {
        body: Some(part),
        at,
        seen: w.tick as f64,
    };
    w.apply(&Command::Grab(target), Some(ada)).expect("taken");
    let s = &w.world.bodies[w.index(part).expect("there")].structure;
    let grabbed = s.orientation[0].conjugate() * (at - s.position[0]);
    let first = told(&w, ada).expect("held");
    assert_eq!(first.load, part);
    assert!((first.grip - grabbed).length() < 1e-3, "{first:?}");
    assert!(first.one_hand, "light and levered little: one hand");
    // Aimed where it lies (first-person placing, G2.10): the hands go to its balance point, a
    // long pole in both; told once, then not again.
    let still = held_where_it_lies(&w, ada, part, false);
    w.take_changed();
    let mut tells = 0;
    for _ in 0..120 {
        w.steer(ada, &still);
        w.step();
        tells += usize::from(w.take_changed().contains(&ada));
    }
    let aimed = told(&w, ada).expect("held");
    assert_eq!(tells, 1, "told {tells} times");
    assert!(aimed.grip.length() < 1e-9 && !aimed.one_hand, "{aimed:?}");
}

#[test]
fn one_hand_is_told_while_the_other_works_and_two_again_after() {
    // A 27 kg pole raised by its end: in both hands.
    let (mut w, ada, id, east, mouth) = ada_and_pole();
    grab_the_top(&mut w, ada, id, east);
    raising_tests::wait(&mut w, ada, 30, DVec3::ZERO);
    assert_eq!(told(&w, ada).map(|c| c.one_hand), Some(false));
    w.take_changed();
    // The other hand backfilling round its foot (G2.4): one hand holds it, told at once.
    let work = building::UnderWay {
        part: id,
        at: mouth,
        fastener: 0,
        done: w.tick + 600,
        work: building::Work::Post,
    };
    w.building.under_way.insert(ada, work);
    w.step();
    assert!(w.take_changed().contains(&ada));
    assert_eq!(told(&w, ada).map(|c| c.one_hand), Some(true));
    w.building.under_way.remove(&ada);
    w.step();
    assert!(w.take_changed().contains(&ada));
    assert_eq!(told(&w, ada).map(|c| c.one_hand), Some(false));
}

#[test]
fn walked_up_hand_over_hand_the_grip_is_told_as_it_moves_and_rarely() {
    let (mut w, ada, id, east, _) = ada_and_pole();
    grab_the_top(&mut w, ada, id, east);
    w.take_changed();
    let (mut tells, mut lagged, mut moved) = (0, 0.0_f64, 0.0_f64);
    let start = w.carrying.get(&ada).expect("held").grip;
    let ticks = 360;
    for t in 0..ticks {
        // Held by the hands themselves (not aimed: an aimed thing is held at its balance point),
        // walked under it: they move along it hand over hand.
        let input = AvatarInput {
            walk: if t < 90 { DVec3::ZERO } else { east * 0.5 },
            hold: None,
            ..AvatarInput::default()
        };
        w.steer(ada, &input);
        w.step();
        let Some(now) = w.carrying.get(&ada).map(|c| c.grip) else {
            break;
        };
        tells += usize::from(w.take_changed().contains(&ada));
        lagged = lagged.max((told(&w, ada).expect("held").grip - now).length());
        moved = moved.max((now - start).length());
    }
    println!(
        "hand over hand {moved:.2} m: told {tells} times in {ticks} ticks, lagging {lagged:.3} m at most"
    );
    assert!(moved > 0.1, "the hands moved along it only {moved} m");
    // At most every 6 ticks, and never more than 1 cm behind but for 6 ticks at 0.5 m/s.
    assert!(tells <= ticks / 6, "{tells} tells");
    assert!(lagged < 0.01 + 0.5 * 6.0 * DT + 2e-3, "{lagged} m behind");
}
