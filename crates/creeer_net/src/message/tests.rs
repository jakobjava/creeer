//! Every message reads back as written, and bytes that are no message are refused.

use super::*;
use crate::entity::{EntityId, Kind};
use crate::entity::{Frame, Look, Matter, Shape};

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
        // Thrust coming and going (on alternate thirds).
        thrust: DVec3::new(0.0, if seq % 3 == 0 { 0.5 } else { 0.0 }, -0.25),
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
        Command::Look { skin: 9, hair: 0 },
        Command::Wield { tool: Some(3) },
        Command::Wield { tool: None },
        Command::Strike {
            target: Target {
                body: Some(EntityId(42)),
                at: DVec3::new(1.0, -2.0, 3e5),
                seen: 1234.375,
            },
            started: 1240.0,
        },
        Command::Strike {
            target: Target {
                body: None,
                at: DVec3::new(0.5, 0.0, -7.0),
                seen: 9.0,
            },
            started: 12.5,
        },
        Command::Grab(Target {
            body: Some(EntityId(3)),
            at: DVec3::new(2.0, 1.0, -0.5),
            seen: 77.25,
        }),
        Command::Drop,
        Command::Throw(DVec3::new(0.6, 0.8, 0.0)),
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
    let matter = Matter {
        mass: 80.0,
        friction: 0.7,
        restitution: 0.25,
        surface: [210.0e9, 0.3, 0.002],
        predicted: true,
    };
    let still = Matter {
        predicted: false,
        ..matter
    };
    for kind in [
        Kind::Body(Shape::Box(DVec3::splat(0.5)), matter),
        Kind::Body(Shape::Sphere(0.4), still),
        Kind::Body(
            Shape::Hull(vec![DVec3::X, DVec3::Y, DVec3::Z, DVec3::ZERO]),
            matter,
        ),
        Kind::Character {
            player: "ada".into(),
            look: Look {
                colour: 15,
                skin: 2,
                hair: 8,
                outfit: vec!["base_layer".into(), "boots".into()],
            },
            fallen: true,
            wields: None,
            carrying: true,
        },
        Kind::Character {
            player: "bob".into(),
            look: Look {
                colour: 0,
                skin: 0,
                hair: 0,
                outfit: vec![],
            },
            fallen: false,
            wields: Some(2),
            carrying: false,
        },
        Kind::Segment {
            person: EntityId(9),
            index: 14,
            shape: Shape::Box(DVec3::new(0.06, 0.07, 0.13)),
        },
        Kind::Structure(frame),
    ] {
        back(&ServerMessage::Spawn(EntityId(3), kind));
    }
    back(&ServerMessage::Despawn(EntityId(1 << 30)));
    back(&ServerMessage::Own {
        gas: 2.25,
        health: 0.75,
        fallen: true,
    });
    let age = crate::message::Age {
        exposure: 1.5,
        rate: 1.0 / 21_600.0,
        at: 1200,
        wear: 0.25,
    };
    back(&ServerMessage::Age(EntityId(42), age));
    back(&ServerMessage::Swing(EntityId(7), 123_456));
    // Six hours of 60 Hz ticks later, a year more.
    assert!((age.exposure_at(1200.0 + 6.0 * 3600.0 * 60.0, 60) - 2.5).abs() < 1e-9);
    assert!(
        (age.exposure_at(0.0, 60) - 1.5).abs() < 1e-12,
        "never before it was told"
    );
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
                thrust: DVec3::ZERO,
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

#[test]
fn profiles_read_back() {
    use creeer_core::profile::{Frame as Ticked, Span};
    back(&ClientMessage::Profile(true));
    back(&ClientMessage::Profile(false));
    let frame = |tick, duration| Ticked {
        tick,
        duration,
        names: vec!["physics: group".into(), "machines".into()],
        threads: vec!["server".into(), "thread 3".into()],
        spans: vec![
            Span {
                name: 1,
                thread: 0,
                depth: 0,
                start: 10,
                end: 2_000,
            },
            Span {
                name: 0,
                thread: 1,
                depth: 2,
                start: 3_000,
                end: 9_000_000,
            },
        ],
    };
    let frames = [
        frame(7, 16_000_000),
        frame(8, 21_500_000),
        frame(9, 16_700_000),
    ];
    let report = ProfileReport::of(&frames).expect("frames");
    assert_eq!(report.durations, vec![16_000, 21_500, 16_700]);
    assert_eq!(report.worst.tick, 8);
    back(&ServerMessage::Profile(report));
    assert!(ProfileReport::of(&[]).is_none());
    // A span naming what the frame does not have is refused.
    let mut bad = frame(1, 1);
    bad.spans[0].name = 5;
    let bytes = to_bytes(&ServerMessage::Profile(ProfileReport {
        durations: vec![1],
        worst: bad,
    }));
    assert!(from_bytes::<ServerMessage>(&bytes).is_err());
}

#[test]
fn a_busy_ticks_report_stays_small() {
    use creeer_core::profile::{Frame as Ticked, Span};
    let names: Vec<String> = (0..40).map(|k| format!("phase {k}")).collect();
    let threads: Vec<String> = (0..16).map(|k| format!("thread {k}")).collect();
    let spans = (0..5000u64)
        .map(|k| Span {
            name: (k % 40) as u16,
            thread: (k % 16) as u16,
            depth: (k % 4) as u8,
            start: k * 3_000,
            end: k * 3_000 + 1_000 + (k * 7_919) % 90_000,
        })
        .collect();
    let frame = Ticked {
        tick: 1,
        duration: 16_000_000,
        names,
        threads,
        spans,
    };
    let frames = vec![frame; 60];
    let report = ProfileReport::of(&frames).expect("frames");
    assert_eq!(report.worst.spans.len(), super::profile::MOST_SPANS);
    let bytes = to_bytes(&ServerMessage::Profile(report)).len();
    assert!(bytes < 4_000, "{bytes} bytes a second");
}
