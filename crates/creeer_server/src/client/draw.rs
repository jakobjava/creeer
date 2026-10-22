//! Where each entity is drawn now: interpolated, the player's own avatar (and what it holds)
//! as predicted.

use super::Connection;
use creeer_net::channel::TICK_RATE;
use creeer_net::entity::EntityId;
use creeer_net::interp::Pose;
use creeer_net::interp::sample;
use std::collections::BTreeMap;

impl Connection {
    /// Every entity's pose to draw it at now (the player's own avatar where it is predicted).
    pub fn poses(&self) -> BTreeMap<EntityId, Pose> {
        let mut poses = sample(&self.receiver, self.drawn_at, 1.0 / f64::from(TICK_RATE));
        poses.retain(|id, _| self.kinds.contains_key(id));
        if self.fallen {
            return poses;
        }
        let Some(p) = &self.predicted else {
            return poses;
        };
        // Bodies just out of the prediction slide to where they are interpolated; those in it
        // are drawn as predicted (ADR-0025).
        for (id, pose) in &mut poses {
            pose.position += p.released(*id);
        }
        for (id, pose) in p.nearby() {
            if poses.contains_key(&id) {
                poses.insert(id, pose);
            }
        }
        if let (Some(w), Some(pose)) = (&self.welcome, p.pose()) {
            let drawn = poses.insert(w.avatar, pose);
            self.in_own_hands(&mut poses, drawn, pose);
        }
        poses
    }
}
