//! Blows heard (ADR-0037): the player's own tool's, when its swing reaches it, on what the
//! crosshair is on; and a fastener's driver's over a fastening the player is told of.

use crate::sounds::{Blow, Heard, Sounds};
use creeer_math::glam::DVec3;

impl Sounds {
    /// The player's own blow, when its swing reaches it, on what the crosshair is on.
    pub(crate) fn swings(&mut self, h: &Heard<'_>) {
        let c = h.connection;
        let Some(avatar) = c.welcome.as_ref().map(|w| w.avatar) else {
            return;
        };
        let now = c.swinging(avatar);
        let was = std::mem::replace(&mut self.swung, now);
        let blow = creeer_sim::strike::TO_BLOW;
        if !(was.is_some_and(|w| w < blow) && now.is_some_and(|n| n >= blow)) {
            return;
        }
        let Some((point, body)) = (h.aimed)() else {
            return;
        };
        let seed = self.next_seed();
        let Some(lib) = &self.lib else { return };
        let spec = crate::sounds_things::tool_blow(lib, &self.extra, h, (point, body), seed);
        if let Some(spec) = spec {
            self.play(spec, h.ears.0);
        }
    }

    /// A fastening the player is told of ("fastening: 2 s"): its driver's blows over its time,
    /// each on the fastener and the part the crosshair was on.
    pub(crate) fn fastenings(&mut self, h: &Heard<'_>) {
        let c = h.connection;
        if let Some((note, when)) = &c.noted
            && self.note != Some(*when)
        {
            self.note = Some(*when);
            let seconds = (note.strip_prefix("fastening: "))
                .and_then(|rest| rest.trim_end_matches(" s").parse::<f64>().ok());
            let driver = (h.fastener).and_then(|f| self.lib.as_ref()?.driver(f).cloned());
            if let (Some(seconds), Some(driver), Some((point, body))) =
                (seconds, driver, (h.aimed)())
            {
                let axis = (point - h.ears.0).normalize_or(DVec3::NEG_Y);
                for k in 0..driver.blows {
                    self.blows.push(Blow {
                        at: self.clock + seconds * (f64::from(k) + 0.5) / f64::from(driver.blows),
                        k,
                        fastener: driver.fastener.clone(),
                        point,
                        body,
                        axis,
                    });
                }
            }
        }
        let due: Vec<Blow> = (self.blows.iter())
            .filter(|b| b.at <= self.clock)
            .cloned()
            .collect();
        self.blows.retain(|b| b.at > self.clock);
        for b in due {
            let seed = self.next_seed();
            let Some(lib) = &self.lib else { return };
            let part = (b.body)
                .and_then(|id| crate::sounds_things::part(lib, &self.extra, c, id, b.point));
            let spec = crate::sounds_things::blow(
                lib,
                &self.extra,
                (&b.fastener, b.k),
                part,
                (b.point, b.axis),
                seed,
            );
            if let Some(spec) = spec {
                self.play(spec, h.ears.0);
            }
        }
    }
}
