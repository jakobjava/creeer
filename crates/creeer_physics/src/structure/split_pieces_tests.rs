//! A structure falling apart (`split.rs`): its pieces as structures keep its mass, momentum
//! and the state of every member.

use super::section::STEEL_S235;
use super::shapes::cantilever;
use super::*;

/// A 2 m cantilever in 4 members, moving, its middle member broken: two pieces.
fn broken() -> Structure {
    let mut s = cantilever(2.0, 4, Section::solid_round(0.05), STEEL_S235);
    for (i, v) in s.velocity.iter_mut().enumerate() {
        *v = DVec3::new(0.1, -0.2 * i as f64, 0.05);
    }
    s.elements[2].broken = true;
    s
}

#[test]
fn a_structure_held_together_is_one_piece() {
    let s = cantilever(2.0, 4, Section::solid_round(0.05), STEEL_S235);
    assert_eq!(s.pieces(), vec![(0..5).collect::<Vec<_>>()]);
}

#[test]
fn a_broken_member_leaves_two_pieces_with_all_it_held() {
    let s = broken();
    let pieces = s.pieces();
    assert_eq!(pieces, vec![vec![0, 1, 2], vec![3, 4]]);
    let parts: Vec<(Structure, Vec<usize>)> = pieces.iter().map(|p| s.piece(p)).collect();
    let mass: f64 = parts.iter().map(|(p, _)| p.total_mass()).sum();
    assert!((mass - s.total_mass()).abs() < 1e-12);
    let momentum = |s: &Structure| -> DVec3 {
        (s.masses().iter().zip(&s.velocity))
            .map(|(m, v)| *v * *m)
            .sum()
    };
    let p: DVec3 = parts.iter().map(|(p, _)| momentum(p)).sum();
    assert!((p - momentum(&s)).length() < 1e-12);
    // The broken member belongs to neither; the others to theirs, renumbered.
    assert_eq!(parts[0].1, vec![0, 1]);
    assert_eq!(parts[1].1, vec![3]);
    assert_eq!((parts[1].0.elements[0].a, parts[1].0.elements[0].b), (0, 1));
    // The fixed root stays with the first piece; the second is free.
    assert!(parts[0].0.is_anchored() && !parts[1].0.is_anchored());
}
