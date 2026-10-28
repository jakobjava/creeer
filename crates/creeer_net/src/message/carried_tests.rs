//! What a character's hands hold reads back as the server told it (G2.10, ADR-0036): the
//! thing, the point held to the nearest of 1/1024 m, and one hand or two.

use super::*;
use crate::entity::{Carried, EntityId, GRIP_SCALE, Kind, Look};

fn holding(carries: Option<Carried>) -> Kind {
    Kind::Character {
        player: "ada".into(),
        look: Look::default(),
        fallen: false,
        wields: None,
        carries,
    }
}

#[test]
fn a_grip_reads_back_to_the_millimetre_in_one_hand_or_two() {
    // A pole's end 1.43 m from its centre, a box's top: neither on the quantisation's grid.
    for (grip, one_hand) in [
        (DVec3::new(0.0123, -1.4321, 0.0405), false),
        (DVec3::new(-0.2, 0.2001, 0.19999), true),
    ] {
        let told = Carried::new(EntityId(77), grip, one_hand);
        // Quantised as it is made: within half a step of what was meant, and itself again.
        assert!((told.grip - grip).abs().max_element() <= 0.5 / GRIP_SCALE + 1e-12);
        assert_eq!(Carried::new(told.load, told.grip, one_hand), told);
        let message = ServerMessage::Spawn(EntityId(3), holding(Some(told)));
        let bytes = to_bytes(&message);
        let ServerMessage::Spawn(_, Kind::Character { carries, .. }) =
            from_bytes::<ServerMessage>(&bytes).unwrap()
        else {
            panic!("a character");
        };
        assert_eq!(carries, Some(told));
        // Told in at most eight bytes more than holding nothing: its id, and three varints of
        // millimetres (measured: 5 and 7).
        let none = to_bytes(&ServerMessage::Spawn(EntityId(3), holding(None))).len();
        assert!(bytes.len() <= none + 8, "{} against {none}", bytes.len());
    }
}

#[test]
fn held_in_three_hands_is_refused() {
    let told = Carried::new(EntityId(77), DVec3::ONE, false);
    let mut bytes = to_bytes(&ServerMessage::Spawn(EntityId(3), holding(Some(told))));
    // The hands' byte is the one before the load's id (77, one byte) and its three varints.
    let at = bytes.len() - 1 - 3 * 2 - 1;
    assert_eq!(bytes[at], 1, "two hands");
    bytes[at] = 3;
    assert!(from_bytes::<ServerMessage>(&bytes).is_err());
}
