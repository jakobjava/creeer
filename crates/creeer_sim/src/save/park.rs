//! The `PARK` section (version 6, F7): the islands of bodies parked far from every player.

use super::{ParkedBody, SaveError};

use creeer_math::codec::{Reader, Writer};
use creeer_physics::world::World;

/// The parked islands as the `PARK` section holds them (`dt`: the world's step).
pub(super) fn park_bytes(parked: &[Vec<ParkedBody>], dt: f64) -> Vec<u8> {
    let mut world = World::new(dt);
    world.bodies = parked.iter().flatten().map(|p| p.body.clone()).collect();
    let mut w = Writer::new();
    w.bytes_with_length(&world.to_bytes());
    w.varint(parked.len() as u64);
    for island in parked {
        w.varint(island.len() as u64);
        for p in island {
            w.u32(p.id);
            w.put(&p.player);
            w.varint(p.water.len() as u64);
            for &m in &p.water {
                w.f64(m);
            }
        }
    }
    w.finish()
}

pub(super) fn park_from(bytes: &[u8], version: u32) -> Result<Vec<Vec<ParkedBody>>, SaveError> {
    let mut r = Reader::new(bytes);
    let mut bodies = World::from_bytes_of(r.bytes_with_length()?, version)?
        .bodies
        .into_iter();
    let mut parked = Vec::new();
    for _ in 0..r.count(1)? {
        let mut island = Vec::new();
        for _ in 0..r.count(5)? {
            let id = r.u32()?;
            let player = r.get::<Option<String>>()?;
            let water = (0..r.count(8)?)
                .map(|_| r.f64())
                .collect::<Result<_, _>>()?;
            let body = bodies
                .next()
                .ok_or_else(|| SaveError::Damaged("fewer parked bodies than entities".into()))?;
            island.push(ParkedBody {
                id,
                player,
                body,
                water,
            });
        }
        parked.push(island);
    }
    r.end()?;
    if bodies.next().is_some() {
        return Err(SaveError::Damaged(
            "more parked bodies than entities".into(),
        ));
    }
    Ok(parked)
}
