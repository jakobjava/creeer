//! The old saves' fixtures pin the format, not the motion (the user's choice, 2026-10-26): each
//! is read and re-saved in today's format, and that is compared with a reference written from
//! it (`fixtures/*.decoded.bin`). They had been compared with their scenes stepped by today's
//! physics, so every physics fix broke them, and fixes were bent round them (G2.0: a standing
//! character's books changed a fixture ball's motion, and seven fixtures with it).
//!
//! The references were written when the save code read every fixture as its scene stepped
//! (95e1f2b, all of them passing); the save code is what changes them, never the physics. A
//! change to the format renews them (`write_decoded_references`), after checking that what the
//! fixtures read is unchanged but for the format's own additions.

use super::Save;

/// Every old fixture whose reading is pinned: its name and its bytes.
const FIXTURES: [(&str, &[u8]); 7] = [
    ("v5_f6_world", include_bytes!("fixtures/v5_f6_world.bin")),
    ("v6_f7_world", include_bytes!("fixtures/v6_f7_world.bin")),
    ("v7_f7_world", include_bytes!("fixtures/v7_f7_world.bin")),
    ("v8_g1_world", include_bytes!("fixtures/v8_g1_world.bin")),
    ("v9_g1_world", include_bytes!("fixtures/v9_g1_world.bin")),
    ("v10_g1_world", include_bytes!("fixtures/v10_g1_world.bin")),
    ("v11_g1_world", include_bytes!("fixtures/v11_g1_world.bin")),
];

fn reference(name: &str) -> String {
    format!(
        "{}/src/save/fixtures/{name}.decoded.bin",
        env!("CARGO_MANIFEST_DIR")
    )
}

/// Fixture `name` read, and re-saved in today's format as it was before: the save.
///
/// # Panics
/// If it cannot be read, or reads otherwise than its reference.
pub(super) fn reads_as_before(name: &str) -> Save {
    let (_, bytes) = FIXTURES
        .iter()
        .find(|f| f.0 == name)
        .expect("a pinned fixture");
    let save = Save::from_bytes(bytes).expect("it reads");
    let expected = std::fs::read(reference(name)).expect("its reference");
    assert!(
        save.to_bytes() == expected,
        "{name} reads otherwise than its reference"
    );
    save
}

#[test]
#[ignore = "writes the fixtures' references (on a change of the format)"]
fn write_decoded_references() {
    for (name, bytes) in FIXTURES {
        let save = Save::from_bytes(bytes).expect("it reads");
        std::fs::write(reference(name), save.to_bytes()).unwrap();
    }
}
