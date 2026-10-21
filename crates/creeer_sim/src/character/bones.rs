//! The ragdoll's bones as a table for the art scripts (G1.3f, ADR-0026): the engineer is
//! modelled and skinned on the fifteen segments and fourteen joints the physics uses, so the
//! drawn body is the simulated one. `creeer_tools bones` writes it to [`PATH`], where Blender's
//! scripts read it; a test keeps that file equal to this.

use super::ragdoll::{LINKS, SEGMENTS};
use creeer_math::glam::DVec3;
use creeer_physics::world::joint::Kind;

/// The segments' names, in the ragdoll's order (Blender's sides: `.L` is the person's left,
/// at +x).
pub const SEGMENT_NAMES: [&str; 15] = [
    "pelvis",
    "torso",
    "head",
    "upper_arm.R",
    "forearm.R",
    "hand.R",
    "thigh.R",
    "shank.R",
    "foot.R",
    "upper_arm.L",
    "forearm.L",
    "hand.L",
    "thigh.L",
    "shank.L",
    "foot.L",
];

/// The joints' names, in the ragdoll's order.
pub const JOINT_NAMES: [&str; 14] = [
    "back",
    "neck",
    "shoulder.R",
    "elbow.R",
    "wrist.R",
    "hip.R",
    "knee.R",
    "ankle.R",
    "shoulder.L",
    "elbow.L",
    "wrist.L",
    "hip.L",
    "knee.L",
    "ankle.L",
];

/// Where the table is kept, from the workspace's root.
pub const PATH: &str = "art/engineer/bones.json";

fn vector(v: DVec3) -> String {
    format!("[{}, {}, {}]", v.x, v.y, v.z)
}

fn maybe(x: Option<f64>) -> String {
    x.map_or_else(|| "null".into(), |x| x.to_string())
}

fn pair(x: Option<(f64, f64)>) -> String {
    x.map_or_else(|| "null".into(), |(a, b)| format!("[{a}, {b}]"))
}

/// The table as JSON: each segment's box (centre and half extents, standing) and share of the
/// mass; each joint's segments, place, axis and limits (rad).
pub fn json() -> String {
    let mut out = String::from(
        "{\n  \"frame\": \"metres, a person standing, above the feet: x across (+x the \
         person's left), y up, z forward\",\n  \"segments\": [\n",
    );
    for (k, (centre, half, share)) in SEGMENTS.iter().enumerate() {
        let comma = if k + 1 < SEGMENTS.len() { "," } else { "" };
        out += &format!(
            "    {{\"name\": \"{}\", \"centre\": {}, \"half\": {}, \"mass_share\": {share}}}{comma}\n",
            SEGMENT_NAMES[k],
            vector(*centre),
            vector(*half),
        );
    }
    out += "  ],\n  \"joints\": [\n";
    for (k, (child, parent, at, axis, kind)) in LINKS.iter().enumerate() {
        let comma = if k + 1 < LINKS.len() { "," } else { "" };
        let limits = match kind {
            Kind::Ball { cone, twist } => {
                format!(
                    "\"kind\": \"ball\", \"cone\": {}, \"twist\": {}",
                    maybe(*cone),
                    pair(*twist)
                )
            }
            Kind::Hinge { range } => format!("\"kind\": \"hinge\", \"range\": {}", pair(*range)),
        };
        out += &format!(
            "    {{\"name\": \"{}\", \"child\": \"{}\", \"parent\": \"{}\", \"at\": {}, \"axis\": {}, {limits}}}{comma}\n",
            JOINT_NAMES[k],
            SEGMENT_NAMES[*child],
            SEGMENT_NAMES[*parent],
            vector(*at),
            vector(*axis),
        );
    }
    out += "  ]\n}\n";
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_art_scripts_read_the_bones_the_physics_has() {
        let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../../art/engineer/bones.json");
        let committed = std::fs::read_to_string(path).unwrap_or_default();
        assert!(
            committed.replace("\r\n", "\n") == json(),
            "{PATH} differs from the ragdoll: run `cargo run -p creeer_tools -- bones`"
        );
    }

    #[test]
    fn sides_and_joints_are_where_their_names_say() {
        for (k, name) in SEGMENT_NAMES.iter().enumerate() {
            let x = SEGMENTS[k].0.x;
            if name.ends_with(".L") {
                assert!(x > 0.0, "{name}");
            } else if name.ends_with(".R") {
                assert!(x < 0.0, "{name}");
            }
        }
        // Each joint lies between its two segments, within 10 cm of both boxes (the shoulders
        // sit 9 cm outside the torso's, at the tops of the arms): a name on the wrong
        // segment would put it tens of centimetres away.
        for (k, (child, parent, at, _, _)) in LINKS.iter().enumerate() {
            for s in [child, parent] {
                let (centre, half, _) = SEGMENTS[*s];
                let outside = ((*at - centre).abs() - half).max(DVec3::ZERO).length();
                assert!(
                    outside < 0.1,
                    "{} is {outside} m off {}",
                    JOINT_NAMES[k],
                    SEGMENT_NAMES[*s]
                );
            }
        }
    }
}
