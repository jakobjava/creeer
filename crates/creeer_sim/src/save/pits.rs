//! The `PITS` section (version 12, G1.5): what was dug and not yet cut from the terrain, by
//! spot: where it was first struck (m, planet-centred) and how much (m³). Without it (an
//! earlier version), nothing is owed.

use super::SaveError;
use creeer_math::codec::{Reader, Writer};
use creeer_math::glam::DVec3;

/// The ledgers as the section holds them.
pub(super) fn pits_bytes(pits: &[(DVec3, f64)]) -> Vec<u8> {
    let mut w = Writer::new();
    w.varint(pits.len() as u64);
    for (at, volume) in pits {
        for x in at.to_array() {
            w.f64(x);
        }
        w.f64(*volume);
    }
    w.finish()
}

/// The ledgers from the section.
pub(super) fn pits_from(bytes: &[u8]) -> Result<Vec<(DVec3, f64)>, SaveError> {
    let mut r = Reader::new(bytes);
    let mut pits = Vec::new();
    // Each 32 bytes: a point and a volume.
    for _ in 0..r.count(32)? {
        let at = DVec3::new(r.finite()?, r.finite()?, r.finite()?);
        let volume = r.finite()?;
        if volume < 0.0 {
            return Err(SaveError::Damaged(format!("a pit owing {volume} m³")));
        }
        pits.push((at, volume));
    }
    r.end()?;
    Ok(pits)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pits_round_trip_and_refuse_debts() {
        let pits = vec![(DVec3::new(1.0, -2.5, 3e5), 0.125), (DVec3::ZERO, 0.0)];
        assert_eq!(pits_from(&pits_bytes(&pits)), Ok(pits));
        let bad = pits_bytes(&[(DVec3::X, -1.0)]);
        assert!(pits_from(&bad).is_err());
    }
}
