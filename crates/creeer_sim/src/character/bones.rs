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

/// What is wrong with a skinned model's skeleton (G1.3f, ADR-0026): its bones must be the
/// ragdoll's fifteen segments by name, each bone's head at its joint with its parent (the
/// pelvis's at its centre) within a millimetre, so the drawn body is the simulated one.
pub fn skeleton_problems(bones: &[creeer_art::model_skin::Bone]) -> Vec<String> {
    let mut out = Vec::new();
    for (k, name) in SEGMENT_NAMES.iter().enumerate() {
        let want = LINKS
            .iter()
            .find(|l| l.0 == k)
            .map_or(SEGMENTS[0].0, |l| l.2);
        match bones.iter().find(|b| b.name == *name) {
            None => out.push(format!("no bone {name}")),
            Some(bone) => {
                let off = (DVec3::from(bone.head.map(f64::from)) - want).length();
                if off > 1e-3 {
                    out.push(format!(
                        "bone {name}'s head is {:.1} mm from its joint",
                        off * 1e3
                    ));
                }
            }
        }
    }
    for bone in bones {
        if !SEGMENT_NAMES.contains(&bone.name.as_str()) {
            out.push(format!("bone {} is no segment of the ragdoll", bone.name));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_skeleton_is_held_to_the_ragdolls_segments_and_joints() {
        let at = |k: usize| {
            LINKS
                .iter()
                .find(|l| l.0 == k)
                .map_or(SEGMENTS[0].0, |l| l.2)
        };
        let mut bones: Vec<creeer_art::model_skin::Bone> = (SEGMENT_NAMES.iter().enumerate())
            .map(|(k, name)| {
                #[allow(clippy::cast_possible_truncation)] // metres, to a micrometre
                let head = at(k).to_array().map(|x| x as f32);
                creeer_art::model_skin::Bone {
                    name: (*name).to_owned(),
                    head,
                }
            })
            .collect();
        assert!(
            skeleton_problems(&bones).is_empty(),
            "{:?}",
            skeleton_problems(&bones)
        );
        bones[4].head[1] += 0.01;
        bones.pop();
        bones.push(creeer_art::model_skin::Bone {
            name: "tail".into(),
            head: [0.0; 3],
        });
        let problems = skeleton_problems(&bones);
        assert_eq!(problems.len(), 3, "{problems:?}");
        assert!(
            problems[0].contains("forearm.R's head is 10.0 mm"),
            "{problems:?}"
        );
        assert!(problems[1].contains("no bone foot.L"), "{problems:?}");
        assert!(problems[2].contains("tail is no segment"), "{problems:?}");
    }

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
