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

    /// A fastening the player is told of ("nailing (nail_75, 2 blows): 1.0 s", "fastening
    /// (peg_25): 10.0 s"): its driver's blows over its time, each on the fastener and the part the
    /// crosshair was on. (The fastener is the note's: the one the hand reached for, G2.11.)
    pub(crate) fn fastenings(&mut self, h: &Heard<'_>) {
        let c = h.connection;
        if let Some((note, when)) = &c.noted
            && self.note != Some(*when)
        {
            self.note = Some(*when);
            let told = driving(note);
            let named = told.and_then(|t| t.0).or(h.fastener);
            let driver = named.and_then(|f| self.lib.as_ref()?.driver(f).cloned());
            if let (Some((_, blows, seconds)), Some(driver), Some((point, body))) =
                (told, driver, (h.aimed)())
            {
                let axis = (point - h.ears.0).normalize_or(DVec3::NEG_Y);
                let blows = blows.unwrap_or(driver.blows).max(1);
                for k in 0..blows {
                    self.blows.push(Blow {
                        at: self.clock + seconds * (f64::from(k) + 0.5) / f64::from(blows),
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

/// A note of driving a fastener: the fastener named in it, its blows if told, and its time (s).
/// "nailing (nail_75, 2 blows): 1.0 s", "fastening (peg_25): 10.0 s", "fastening: 2 s"; another
/// note (backfilling, tacking) is not.
fn driving(note: &str) -> Option<(Option<&str>, Option<u32>, f64)> {
    let (doing, rest) = note.rsplit_once(": ")?;
    let seconds = rest.strip_suffix(" s")?.parse::<f64>().ok()?;
    let (word, inside) = match doing.split_once(" (") {
        Some((word, inside)) => (word, inside.strip_suffix(')')),
        None => (doing, None),
    };
    if word != "nailing" && word != "fastening" {
        return None;
    }
    let mut parts = inside.into_iter().flat_map(|s| s.split(", "));
    let fastener = parts.next();
    let blows = parts.find_map(|p| p.strip_suffix(" blows").or(p.strip_suffix(" blow")));
    Some((fastener, blows.and_then(|b| b.parse().ok()), seconds))
}

#[cfg(test)]
mod tests {
    #[test]
    fn the_notes_of_driving_a_fastener_are_read() {
        let nail = super::driving("nailing (nail_75, 2 blows): 1.0 s");
        assert_eq!(nail, Some((Some("nail_75"), Some(2), 1.0)));
        let one = super::driving("nailing (nail_75, 1 blow): 0.5 s");
        assert_eq!(one, Some((Some("nail_75"), Some(1), 0.5)));
        let peg = super::driving("fastening (peg_25): 10.0 s");
        assert_eq!(peg, Some((Some("peg_25"), None, 10.0)));
        assert_eq!(super::driving("fastening: 2 s"), Some((None, None, 2.0)));
        assert_eq!(super::driving("backfilling: 8 s"), None);
        assert_eq!(super::driving("fastened with nail_75"), None);
    }
}
