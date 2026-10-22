//! Making by hand (ADR-0030): a blow knaps chert to a sharp flake and crumbles granite, mass
//! kept exactly; a weak blow bruises; bark peels as the branch's loss; a head bound to a haft
//! swings as the parts say; a steel axe made of its parts swings as the catalogue's measured one.

use super::*;
use crate::strike;
use crate::tools::Tool;

const CHERT: Breaks = Breaks {
    grain: 1e-5,
    fracture_energy: 32.0,
};
const GRANITE: Breaks = Breaks {
    grain: 3e-3,
    fracture_energy: 45.0,
};

fn stone(material: &str, density: f64) -> Piece {
    Piece::solid(
        material,
        density,
        Form::Lump {
            size: DVec3::new(0.12, 0.08, 0.05),
        },
    )
}

fn mass(b: &Broken) -> f64 {
    match b {
        Broken::Flake(p) => p.mass(),
        Broken::Grit(g) => g.mass,
    }
}

#[test]
fn a_hard_blow_knaps_chert_to_a_sharp_flake_and_keeps_the_mass() {
    let mut core = stone("chert", 2600.0);
    let before = core.mass();
    let flake = knap(&mut core, &CHERT, 20.0).expect("a flake");
    assert!((core.mass() + mass(&flake) - before).abs() < 1e-12 * before);
    let Broken::Flake(f) = flake else {
        panic!("chert flakes")
    };
    let long = f.form.extents()[0];
    assert!((0.03..0.06).contains(&long), "a flake {long} m long");
    let edge = f.edge.unwrap();
    assert!((edge.radius - 5e-6).abs() < 1e-12, "half a grain");
    assert!((edge.angle - FLAKE_ANGLE).abs() < 1e-12);
    assert!(
        core.edge.is_some_and(|e| e.angle == CORE_ANGLE),
        "the core keeps an edge"
    );
}

#[test]
fn the_same_blow_crumbles_granite() {
    let mut core = stone("granite", 2650.0);
    let before = core.mass();
    let grit = knap(&mut core, &GRANITE, 20.0).expect("something comes off");
    assert!(matches!(grit, Broken::Grit(_)), "coarse grains crumble");
    assert!((core.mass() + mass(&grit) - before).abs() < 1e-12 * before);
    assert!(core.edge.is_none());
}

#[test]
fn a_weak_blow_only_bruises() {
    let mut core = stone("chert", 2600.0);
    let before = core.clone();
    assert!(knap(&mut core, &CHERT, 0.05).is_none());
    assert_eq!(core, before);
}

#[test]
fn bark_peels_as_the_branchs_loss() {
    let mut branch = Piece::solid(
        "spruce_c24",
        420.0,
        Form::Rod {
            radius: 0.02,
            length: 0.6,
        },
    );
    let before = branch.mass();
    let strip = strip(&mut branch, ("spruce_bast", 600.0)).expect("it peels");
    assert!((branch.mass() + strip.mass() - before).abs() < 1e-12 * before);
    let thin = Piece::solid(
        "spruce_c24",
        420.0,
        Form::Rod {
            radius: 0.004,
            length: 0.6,
        },
    );
    assert!(super::strip(&mut thin.clone(), ("spruce_bast", 600.0)).is_none());
}

/// The catalogue's axe (`data/tools.ron`): Gränsfors Bruk's American felling axe.
fn catalogue_axe() -> Tool {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../data");
    let data = creeer_data::Data::open(&root, &[]).unwrap();
    (Tool::all(&data).unwrap().into_iter())
        .find(|t| t.name == "axe")
        .unwrap()
}

#[test]
fn a_steel_axe_made_of_its_parts_swings_as_the_measured_one() {
    let axe = catalogue_axe();
    // A hickory haft (its 0.5 kg over the listing's 80 cm) and a forged steel head of 1.6 kg.
    let haft_volume = PI * 0.016 * 0.016 * axe.length;
    let haft = Piece::solid(
        "hickory",
        (axe.mass - axe.head) / haft_volume,
        Form::Rod {
            radius: 0.016,
            length: axe.length,
        },
    );
    let head = Piece::solid(
        "steel_s235",
        7850.0,
        Form::Block {
            size: DVec3::new(0.04, axe.head / (7850.0 * 0.04 * 0.03), 0.03),
        },
    );
    let binding = Piece::solid(
        "spruce_bast",
        600.0,
        Form::Block {
            size: DVec3::new(1.0, 0.03, 0.0015),
        },
    );
    let made = bind(haft, head, binding, 20e6).unwrap();
    let s = made.swinging();
    assert!((s.mass - axe.mass).abs() < 0.03 * axe.mass, "{} kg", s.mass);
    assert!((s.carry - axe.carry).abs() < 0.02, "balance {}", s.carry);
    assert!(
        (s.gyration - axe.gyration).abs() < 0.02,
        "gyration {}",
        s.gyration
    );
    assert!((s.strike - axe.strike).abs() < 0.02, "reach {}", s.strike);
    let (ours, theirs) = (
        strike::swing_of(s.mass, s.gyration, s.carry, s.strike),
        strike::swing(&axe),
    );
    assert!((ours.energy - theirs.energy).abs() < 0.05 * theirs.energy);
}

#[test]
fn a_stone_axe_holds_what_its_binding_holds_and_comes_apart_whole() {
    let haft = Piece::solid(
        "spruce_c24",
        420.0,
        Form::Rod {
            radius: 0.015,
            length: 0.6,
        },
    );
    let head = stone("chert", 2600.0);
    let mut branch = Piece::solid(
        "spruce_c24",
        420.0,
        Form::Rod {
            radius: 0.02,
            length: 0.9,
        },
    );
    let binding = strip(&mut branch, ("spruce_bast", 600.0)).unwrap();
    let made = bind(haft.clone(), head.clone(), binding.clone(), 20e6).unwrap();
    // Wraps of the strip's strength (20 MPa over its section), each gripping twice by friction.
    let Form::Block { size } = binding.form else {
        unreachable!()
    };
    let wraps = made.hold / (2.0 * FRICTION * 20e6 * size.y * size.z);
    assert!(
        wraps >= 2.0 && (wraps - wraps.round()).abs() < 1e-9,
        "{wraps}"
    );
    let total = made.mass();
    assert!((total - haft.mass() - head.mass() - binding.mass()).abs() < 1e-12);
    let pieces = apart(made);
    let back: f64 = pieces.iter().map(Item::mass).sum();
    assert!((back - total).abs() < 1e-12);
    // A strip too short to wrap twice binds nothing.
    let short = Piece::solid(
        "spruce_bast",
        600.0,
        Form::Block {
            size: DVec3::new(0.2, 0.03, 0.0015),
        },
    );
    assert!(bind(haft, head, short, 20e6).is_err());
}

#[test]
fn a_stone_in_the_fist_swings_harder_than_a_pebble_and_softer_than_an_axe() {
    let swing = |item: &Item| {
        let s = item.swinging().unwrap();
        strike::swing_of(s.mass, s.gyration, s.carry, s.strike).energy
    };
    let hammer = Item::Piece(stone("granite", 2650.0), Vec::new());
    let pebble = Item::Piece(
        Piece::solid(
            "granite",
            2650.0,
            Form::Lump {
                size: DVec3::new(0.03, 0.02, 0.015),
            },
        ),
        Vec::new(),
    );
    let axe = strike::swing(&catalogue_axe()).energy;
    let (h, p) = (swing(&hammer), swing(&pebble));
    assert!(
        p < h && h < axe,
        "pebble {p} J, hammerstone {h} J, axe {axe} J"
    );
    assert!((5.0..60.0).contains(&h), "a hammerstone's blow {h} J");
}
