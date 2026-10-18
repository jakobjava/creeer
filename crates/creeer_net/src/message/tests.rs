//! Every message reads back as written, and bytes that are no message are refused.

use super::*;
use crate::entity::{EntityId, Kind};
use crate::entity::{Frame, Shape};

fn back<T: Encode + Decode + PartialEq + std::fmt::Debug>(m: &T) {
    let bytes = to_bytes(m);
    assert_eq!(&from_bytes::<T>(&bytes).unwrap(), m);
    // Cut short or with a byte too many: refused (but a snapshot, tag 6, is the rest of its
    // packet whatever its length: the snapshot's own decoder checks it).
    if bytes.first() == Some(&6) {
        return;
    }
    for cut in 0..bytes.len() {
        assert!(
            from_bytes::<T>(&bytes[..cut]).is_err(),
            "{m:?} cut at {cut}"
        );
    }
    let mut longer = bytes.clone();
    longer.push(0);
    assert!(from_bytes::<T>(&longer).is_err(), "{m:?} with a byte more");
}

#[test]
fn client_messages_read_back() {
    let input = |seq| AvatarInput {
        seq,
        walk: DVec3::new(1.5, 0.0, -0.25),
        jump: seq % 2 == 0,
    };
    back(&ClientMessage::Hello {
        protocol: crate::PROTOCOL_VERSION,
        name: "Ada Lovelace — ✓".into(),
        content: 0x0123_4567_89ab_cdef,
    });
    back(&ClientMessage::Inputs {
        ack: 77,
        inputs: (5..13).map(input).collect(),
    });
    for command in [
        Command::Crate(DVec3::new(1.0, 2.0, 3.0)),
        Command::Ball(DVec3::new(-1.0, 2.5, 3e5)),
        Command::Edit(vec![1, 2, 3, 250]),
        Command::Clear,
        Command::Frame(DVec3::new(0.5, 0.25, 0.125)),
        Command::Powerhouse(DVec3::new(4.0, -2.5, 1e5)),
    ] {
        back(&ClientMessage::Command(command));
    }
}

#[test]
fn server_messages_read_back() {
    back(&ServerMessage::Welcome {
        client: u64::MAX - 3,
        avatar: EntityId(12),
        tick: 9000,
        rate: 60,
        planet: "planets/home.ron".into(),
    });
    back(&ServerMessage::Refused("the server is full".into()));
    let frame = Frame {
        nodes: vec![DVec3::ZERO, DVec3::X, DVec3::Y],
        members: vec![[0, 1], [1, 2]],
        radius: 0.05,
    };
    for kind in [
        Kind::Body(Shape::Box(DVec3::splat(0.5))),
        Kind::Body(Shape::Sphere(0.4)),
        Kind::Body(Shape::Hull(vec![DVec3::X, DVec3::Y, DVec3::Z, DVec3::ZERO])),
        Kind::Avatar {
            radius: 0.4,
            player: "ada".into(),
        },
        Kind::Structure(frame),
    ] {
        back(&ServerMessage::Spawn(EntityId(3), kind));
    }
    back(&ServerMessage::Despawn(EntityId(1 << 30)));
    back(&ServerMessage::Edits {
        first: 17,
        log: vec![9; 300],
    });
    let nodes = vec![
        (4, Quantised([1, -2, 3])),
        (40, Quantised([-1_000_000, 0, 7])),
    ];
    back(&ServerMessage::Plastic(
        EntityId(8),
        Plastic {
            nodes: nodes.clone(),
            members: None,
            broken: vec![3],
        },
    ));
    back(&ServerMessage::Plastic(
        EntityId(8),
        Plastic {
            nodes,
            members: Some(vec![[0, 41], [41, 1]]),
            broken: vec![],
        },
    ));
    back(&ServerMessage::Snapshot(vec![1, 2, 3]));
    back(&ServerMessage::Machines(vec![
        MachineView {
            id: 3,
            at: DVec3::new(1.0, 2.0, 3.0),
            mode: 1,
            water: 140.5,
            hydrogen: 21.25,
            time: 3600.0,
        };
        2
    ]));
}

#[test]
fn bytes_that_are_no_message_are_refused() {
    for bytes in [&[][..], &[9], &[2, 9], &[0, 1]] {
        assert!(from_bytes::<ClientMessage>(bytes).is_err(), "{bytes:?}");
    }
    for bytes in [&[][..], &[7], &[2, 0, 9]] {
        assert!(from_bytes::<ServerMessage>(bytes).is_err(), "{bytes:?}");
    }
}

#[test]
fn inputs_cost_a_byte_or_two_each() {
    // Eight inputs walking on at 3.2 m/s, one of them jumping: a few bytes each at most.
    let walk = DVec3::new(2.0, 0.0, -2.5);
    let inputs: Vec<AvatarInput> = (100..108)
        .map(|seq| {
            AvatarInput {
                seq,
                walk,
                jump: seq == 104,
            }
            .quantised()
        })
        .collect();
    let bytes = to_bytes(&ClientMessage::Inputs {
        ack: 5000,
        inputs: inputs.clone(),
    });
    assert!(bytes.len() <= 20, "{} bytes", bytes.len());
    assert_eq!(
        from_bytes::<ClientMessage>(&bytes).unwrap(),
        ClientMessage::Inputs { ack: 5000, inputs }
    );
    // Out of order, or numbered 0: refused.
    let mut w = Writer::new();
    w.u8(1);
    w.varint(1);
    w.varint(1);
    w.u8(SAME_WALK);
    w.varint(0);
    assert!(from_bytes::<ClientMessage>(&w.finish()).is_err());
}
