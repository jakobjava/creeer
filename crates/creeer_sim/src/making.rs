//! Making by hand (G1.6d, ADR-0030): what a blow breaks off a stone, bark stripped from a branch,
//! and a head bound to a haft. No recipe: each is physics on the pieces as they are, so the same
//! blow knaps chert to a sharp edge and crumbles granite.
//!
//! **A blow breaks a brittle piece** (a rock with `Breaks`). A share of the blow's energy goes
//! into new crack surface at the rock's fracture energy G_c; the flake is as long as that surface
//! allows (two faces of about L × 0.8 L), a fifth as thick. Where the rock's grains are coarser
//! than a tenth of the flake's thickness, the flake crumbles to grit; else it comes off whole,
//! its margin a feather edge, and the core keeps a steeper edge where it came from. No edge is
//! sharper than half a grain: obsidian's are nanometres, chert's microns, granite's a millimetre.
//!
//! **Bark** peels off a fresh branch by hand as a strip of inner bark (bast), its mass the
//! branch's loss. **Binding** sets a head across a haft's end and winds a strip about the joint;
//! what it holds is the strip's strength times its wraps and friction, and a head wedged into
//! the haft's split end the wood's bearing across its grain besides (G2.3, `bind_split`).

use crate::items::{Assembly, Bulk, Edge, Form, Item, Part, Piece};
use creeer_math::det;
use creeer_math::glam::{DQuat, DVec3};
use creeer_terrain::geology::rocks::Breaks;
use std::f64::consts::PI;

/// The share of a blow's energy that goes into new crack surface. Calibrated: a 20 J
/// hard-hammer blow on chert takes off a flake about 4 cm long, as experimental knapping's
/// hard-hammer flakes are; the rest goes into motion, heat and sound.
pub const INTO_CRACKS: f64 = 0.005;
/// The smallest flake that comes off (m): a blow too weak for it only bruises.
pub const SMALLEST_FLAKE: f64 = 0.005;
/// A flake's margin's angle (rad, measured flakes' edges 20–45°) and a core's after flaking (60–80°).
pub const FLAKE_ANGLE: f64 = 30.0 * PI / 180.0;
pub const CORE_ANGLE: f64 = 70.0 * PI / 180.0;
/// No edge is sharper than this (m): an atomic step.
const SHARPEST: f64 = 1e-9;
/// Inner bark's thickness on a branch (m), and the least branch radius it peels from (m).
pub const BARK: f64 = 0.0015;
const PEELS_FROM: f64 = 0.006;
/// Bark on bark and wood, wound and pulled tight: friction.
const FRICTION: f64 = 0.5;

/// What a blow broke off.
#[derive(Clone, Debug, PartialEq)]
pub enum Broken {
    /// A flake, with its edge.
    Flake(Piece),
    /// Grit: the rock crumbled.
    Grit(Bulk),
}

/// A blow of `energy` (J) on brittle `piece` (a rock that `breaks`): what comes off, if anything;
/// the piece shrinks by it and keeps an edge where it came from.
pub fn knap(piece: &mut Piece, breaks: &Breaks, energy: f64) -> Option<Broken> {
    let area = INTO_CRACKS * energy / breaks.fracture_energy;
    let long = piece.form.extents()[0];
    let length = (area / 1.6).sqrt().min(0.6 * long);
    if length < SMALLEST_FLAKE {
        return None;
    }
    let size = DVec3::new(length, 0.8 * length, 0.2 * length);
    let mut volume = Form::Lump { size }.volume();
    // At most a third of what is left at a blow.
    volume = volume.min(piece.form.volume() / 3.0);
    let scale = det::cbrt((piece.form.volume() - volume) / piece.form.volume());
    let mass = piece.density * volume;
    piece.form = scaled(piece.form, scale);
    let size = size * det::cbrt(volume / Form::Lump { size }.volume());
    let radius = (0.5 * breaks.grain).max(SHARPEST);
    let crumbles = breaks.grain > 0.1 * size.z;
    if !crumbles {
        let rim = PI * 0.5 * (piece.form.extents()[0] + piece.form.extents()[1]);
        let had = piece.edge.map_or(0.0, |e| e.length);
        piece.edge = Some(Edge {
            angle: CORE_ANGLE,
            length: (had + size.y).min(rim),
            radius,
        });
    }
    Some(if crumbles {
        Broken::Grit(Bulk {
            mass,
            composition: vec![(piece.material.clone(), 1.0)],
            temperature: 288.15,
            specific_heat: 800.0,
            grain: breaks.grain,
            // Loose grit packs to about 60 % solid.
            bulk_density: 0.6 * piece.density,
        })
    } else {
        let mut flake = Piece::solid(&piece.material, piece.density, Form::Lump { size });
        flake.edge = Some(Edge {
            angle: FLAKE_ANGLE,
            length: size.x,
            radius,
        });
        Broken::Flake(flake)
    })
}

fn scaled(form: Form, s: f64) -> Form {
    match form {
        Form::Block { size } => Form::Block { size: size * s },
        Form::Lump { size } => Form::Lump { size: size * s },
        Form::Ball { radius } => Form::Ball { radius: radius * s },
        Form::Rod { radius, length } => Form::Rod {
            radius: radius * s,
            length: length * s,
        },
    }
}

/// Bark peeled from `branch` (a rod of wood, fresh) as a strip of `bast` (its id and density):
/// the branch thinner by the strip's mass. None if it is no rod or too thin to peel.
pub fn strip(branch: &mut Piece, bast: (&str, f64)) -> Option<Piece> {
    let Form::Rod { radius, length } = branch.form else {
        return None;
    };
    if radius < PEELS_FROM {
        return None;
    }
    let width = 0.5 * 2.0 * PI * radius;
    let strip = Piece::solid(
        bast.0,
        bast.1,
        Form::Block {
            size: DVec3::new(length, width, BARK),
        },
    );
    let lost = strip.mass() / (branch.density * PI * length);
    branch.form = Form::Rod {
        radius: (radius * radius - lost).sqrt(),
        length,
    };
    Some(strip)
}

/// `head` bound across the end of `haft` (a rod) with `binding` (a strip of `strength` Pa along
/// it): an axe or an adze. Its grip is the haft's other end; the head stands out on the side its
/// edge is, a quarter of its length behind.
///
/// # Errors
/// If the haft is no rod, or the binding no strip long enough for two wraps.
pub fn bind(haft: Piece, head: Piece, binding: Piece, strength: f64) -> Result<Assembly, String> {
    bind_split(haft, head, binding, strength, None)
}

/// As [`bind`], the haft's wood `across` its grain this strong (Pa): a head no thicker than the
/// haft goes into its split end, the two halves sprung apart about it and lashed shut (a knapped
/// flake is a fifth as thick as long: thinner than the haft's radius only up to 9 cm, and every
/// blow broke a 10 cm one off its stick, G2.8, the user's play) (the split haft of Neolithic axes: the Ötzi axe's
/// forked yew, split-hafted adzes), and a blow drives it against the split's two walls. They
/// bear across the grain over about the haft's diameter squared each, on top of what the
/// lashing holds by friction (it keeps the split closed). A thicker head is only lashed on.
pub fn bind_split(
    haft: Piece,
    head: Piece,
    binding: Piece,
    strength: f64,
    across: Option<f64>,
) -> Result<Assembly, String> {
    let Form::Rod { radius, length } = haft.form else {
        return Err("a haft is a stick".into());
    };
    let Form::Block { size: strip } = binding.form else {
        return Err("a binding is a strip".into());
    };
    let [long, mid, thin] = head.form.extents();
    // Once about the haft and the head together.
    let wrap = 2.0 * (2.0 * radius + thin) + 2.0 * mid;
    let wraps = (strip.x / wrap).floor();
    if wraps < 2.0 {
        return Err(format!("too short a binding: {wraps} wraps"));
    }
    let tension = strength * strip.y * strip.z;
    let lashed = 2.0 * wraps * tension * FRICTION;
    let diameter = 2.0 * radius;
    let wedged = match across {
        Some(across) if thin < diameter => across * 2.0 * diameter * diameter,
        _ => 0.0,
    };
    let hold = lashed + wedged;
    // The head's longest axis across the haft (along y), its edge outward.
    let turn = match head.form {
        Form::Block { size } | Form::Lump { size } if size.y > size.x => DQuat::IDENTITY,
        _ => DQuat::from_rotation_z(PI * 0.5),
    };
    let at = DVec3::new(length - radius, 0.25 * long, 0.0);
    let parts = vec![
        Part {
            piece: haft,
            at: DVec3::X * (0.5 * length),
            turn: DQuat::IDENTITY,
        },
        Part {
            piece: head,
            at,
            turn,
        },
    ];
    Ok(Assembly {
        parts,
        bindings: vec![binding],
        hold,
    })
}

/// The pieces a made thing falls into when it comes apart.
pub fn apart(made: Assembly) -> Vec<Item> {
    (made.parts.into_iter())
        .map(|p| p.piece)
        .chain(made.bindings)
        .map(|p| Item::Piece(p, Vec::new()))
        .collect()
}

#[cfg(test)]
mod tests;
