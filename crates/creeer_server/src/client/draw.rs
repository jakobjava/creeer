//! Where each entity is drawn now: interpolated, the player's own avatar (and what it holds)
//! as predicted.

use super::Connection;
use creeer_net::channel::TICK_RATE;
use creeer_net::entity::EntityId;
use creeer_net::interp::Pose;
use creeer_net::interp::sample;
use std::collections::BTreeMap;

impl Connection {
    /// The player's own avatar as drawn, once it has come.
    pub fn avatar(&self) -> Option<Pose> {
        if self.fallen {
            // Down: its pelvis as the server has it.
            let pelvis = self.welcome.as_ref()?.avatar;
            let poses = sample(&self.receiver, self.drawn_at, 1.0 / f64::from(TICK_RATE));
            return poses.get(&pelvis).copied();
        }
        self.predicted.as_ref()?.pose()
    }

    /// The server tick drawn at (fractional; a little in the past).
    pub fn drawn_at(&self) -> f64 {
        self.drawn_at
    }

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
