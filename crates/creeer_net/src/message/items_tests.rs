//! The messages of items (G1.6) read back as written: what a player carries, what lies on the
//! ground, what came of what they did, and what an item body is.

use super::*;
use crate::entity::EntityId;

fn back<T: Encode + Decode + PartialEq + std::fmt::Debug>(m: &T) {
    let bytes = to_bytes(m);
    assert_eq!(&from_bytes::<T>(&bytes).unwrap(), m);
}

#[test]
fn item_messages_read_back_as_written() {
    let item = |text: &str| crate::message::ItemView {
        text: text.into(),
        word: "stone".into(),
        colour: Some([95, 90, 85]),
        mass: 0.04,
        room: 2.7e-5,
        swing: 12.5,
    };
    back(&ServerMessage::Inventory(crate::message::Inventory {
        pockets: vec![
            crate::message::StoreView {
                name: "jacket.chest.L".into(),
                volume: 3.3e-4,
                items: vec![item("granite, 3 cm across"), item("granite, 2 cm across")],
            },
            crate::message::StoreView::default(),
        ],
        open: Some((EntityId(9), crate::message::StoreView::default())),
        hands: Some(item("granite, 8 cm across")),
    }));
    back(&ServerMessage::Inventory(
        crate::message::Inventory::default(),
    ));
    back(&ServerMessage::Laid(vec![crate::message::Laid {
        id: (1 << 63) | 5,
        centre: DVec3::new(1.0, 2.0, 3.0),
        turn: creeer_math::glam::DQuat::from_rotation_z(0.5),
        half: DVec3::new(0.02, 0.015, 0.004),
        colour: [95, 90, 85],
        word: "stone".into(),
        parts: vec![crate::message::Part {
            at: DVec3::X * 0.3,
            turn: creeer_math::glam::DQuat::IDENTITY,
            half: DVec3::new(0.3, 0.015, 0.015),
            colour: [200, 160, 100],
        }],
        mass: 0.2,
    }]));
    back(&ServerMessage::Note("a flake came off".into()));
    back(&ServerMessage::Item(
        EntityId(4),
        crate::message::ItemLook {
            word: "flake".into(),
            parts: Vec::new(),
            mass: 0.05,
            grip: None,
        },
    ));
    back(&ServerMessage::Item(
        EntityId(5),
        crate::message::ItemLook {
            word: "tool".into(),
            parts: Vec::new(),
            mass: 0.4,
            grip: Some(DVec3::new(0.1, -0.02, 0.0)),
        },
    ));
}
