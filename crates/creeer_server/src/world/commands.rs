//! What players ask of the world: crates, balls, frames, machines and kit items put down, items
//! put away and taken out (G1.6a, `items.rs`), terrain dug and
//! filled, the world cleared, each checked against the limits (`world.rs`).

use super::*;

impl ServerWorld {
    /// Does what a player asks, `by` their avatar (none: the host itself, unchecked).
    ///
    /// # Errors
    /// What was refused, and why.
    pub fn apply(&mut self, command: &Command, by: Option<EntityId>) -> Result<(), String> {
        let _scope = profile::scope("command");
        // The sandbox's commands, within 500 m: a creative or test server's alone (G1.4d).
        let sandbox = !matches!(
            command,
            Command::Look { .. }
                | Command::Wield { .. }
                | Command::Strike { .. }
                | Command::Grab(_)
                | Command::Drop
                | Command::Throw(_)
                | Command::Stow(_)
                | Command::TakeOut { .. }
                | Command::PutIn(_)
                | Command::TakeFrom { .. }
                | Command::Open(_)
                | Command::Pick { .. }
                | Command::Strip
                | Command::Bind(_)
                | Command::BindLying(_)
                | Command::StrikeLoose { .. }
                | Command::StrikeTree { .. }
                | Command::Fasten { .. }
                | Command::Unfasten(_)
        );
        if sandbox && by.is_some() && !self.creative {
            return Err("only on a creative server".into());
        }
        let near = by
            .and_then(|id| self.index(id))
            .map(|i| self.world.bodies[i].structure.position[0]);
        let within = |p: DVec3| -> Result<(), String> {
            if !p.is_finite() {
                return Err("a point that is not finite".into());
            }
            match near {
                Some(a) if (p - a).length() > REACH => Err(format!("beyond reach ({REACH} m)")),
                _ => Ok(()),
            }
        };
        match command {
            Command::Look { skin, hair } => {
                let player = (by.and_then(|id| self.index(id)))
                    .and_then(|i| self.players[i].clone())
                    .ok_or("only a player has a look")?;
                self.set_look(&player, *skin, *hair);
            }
            Command::Strike { target, started } => {
                self.strike(by.ok_or("only a player strikes")?, target, *started)?;
            }
            Command::Grab(target) => self.grab(by.ok_or("only a player grabs")?, target)?,
            Command::Drop => self.let_go(by.ok_or("only a player lets go")?),
            Command::Throw(direction) => {
                self.throw(by.ok_or("only a player throws")?, *direction)?
            }
            Command::Stow(target) => self.stow(by.ok_or("only a player")?, target.as_ref())?,
            Command::TakeOut { pocket, item } => {
                self.take_out(by.ok_or("only a player")?, *pocket, *item)?;
            }
            Command::PutIn(target) => self.put_in(by.ok_or("only a player")?, target)?,
            Command::TakeFrom { container, item } => {
                self.take_from(by.ok_or("only a player")?, container, *item)?;
            }
            Command::Open(target) => self.open(by.ok_or("only a player")?, target)?,
            Command::Pick { loose, pocket } => {
                self.pick(by.ok_or("only a player")?, *loose, *pocket)?
            }
            Command::Strip => self.strip(by.ok_or("only a player")?)?,
            Command::Bind(target) => {
                let by = by.ok_or("only a player")?;
                self.reached(by, target.body, target.at, target.seen)?;
                let head = target.body.ok_or("nothing there to bind on")?;
                self.bind(by, making::Head::Body(head))?;
            }
            Command::Fasten { target, fastener } => {
                let by = by.ok_or("only a player")?;
                self.reached(by, target.body, target.at, target.seen)?;
                let part = target.body.ok_or("nothing there to fasten")?;
                self.begin_fastening(by, part, target.at, fastener)?;
            }
            Command::Unfasten(target) => {
                let by = by.ok_or("only a player")?;
                self.reached(by, target.body, target.at, target.seen)?;
                let built = target.body.ok_or("nothing there to take apart")?;
                self.begin_unfastening(by, built, target.at)?;
            }
            Command::BindLying(id) => {
                self.bind(by.ok_or("only a player")?, making::Head::Lying(*id))?
            }
            Command::StrikeLoose { loose, started } => {
                self.strike_loose(by.ok_or("only a player")?, *loose, *started)?;
            }
            Command::StrikeTree {
                tree,
                height,
                started,
            } => {
                self.strike_tree(by.ok_or("only a player")?, *tree, *height, *started)?;
            }
            Command::Kit { item, at } => {
                within(*at)?;
                self.clear_of_trunks(*at, 0.3)?;
                self.drop_kit(*item, *at)?;
            }
            Command::Wield { tool } => {
                let player = (by.and_then(|id| self.index(id)))
                    .and_then(|i| self.players[i].clone())
                    .ok_or("only a player holds a tool")?;
                self.set_wield(&player, *tool)?;
            }
            Command::Crate(at) => {
                within(*at)?;
                self.clear_of_trunks(*at, 0.71)?;
                let mut body = Body::rigid(
                    Shape::Box {
                        half: DVec3::splat(0.5),
                    },
                    200.0,
                    *at,
                    DT,
                );
                body.structure.orientation[0] = upright(*at);
                body.friction = 0.7;
                self.add(body, None);
            }
            Command::Box(at) => {
                within(*at)?;
                self.clear_of_trunks(*at, 0.29)?;
                // As the play scene's: 0.4 m of wood, 15 kg (G1.4f: one to carry).
                let half = DVec3::splat(0.2);
                let mut body = Body::rigid(Shape::Box { half }, 15.0, *at, DT);
                body.structure.orientation[0] = upright(*at);
                body.friction = 0.6;
                self.add(body, None);
            }
            Command::Ball(at) => {
                within(*at)?;
                self.clear_of_trunks(*at, 0.4)?;
                let mut body = Body::rigid(Shape::Sphere { radius: 0.4 }, 300.0, *at, DT);
                body.friction = 0.7;
                body.restitution = 0.3;
                self.add(body, None);
            }
            Command::Frame(at) => {
                within(*at)?;
                self.clear_of_trunks(*at, 1.5)?;
                self.add(frame(*at), None);
            }
            Command::Powerhouse(at) => {
                within(*at)?;
                self.place_powerhouse(*at);
            }
            Command::Edit(bytes) => {
                let edits = Edits::from_bytes(bytes).map_err(|e| e.to_string())?;
                let [edit] = edits.log() else {
                    return Err("an edit command holds one edit".into());
                };
                let (lo, hi) = edit.brush().bounds();
                within((lo + hi) * 0.5)?;
                check(edit)?;
                self.terrain.edit(*edit);
                // Bodies resting on removed ground must fall.
                self.world
                    .wake_near((lo + hi) * 0.5, (hi - lo).length() * 0.5 + 1.0);
            }
            Command::Clear => {
                self.stand_all();
                self.flush();
                while let Some(key) = self.parked.find(|_| true) {
                    self.unpark(key);
                }
                let players = &self.players;
                let map = self.world.retain_bodies(|i, _| players[i].is_some());
                self.surroundings.retain(&map);
                (self.ids, self.players) = (kept(&map, &self.ids), kept(&map, &self.players));
                self.reindex();
            }
        }
        Ok(())
    }
}

/// Whether an edit is one a tool makes.
fn check(edit: &Edit) -> Result<(), String> {
    match *edit.brush() {
        Brush::Sphere { center, radius } if center.is_finite() && radius <= MOST_BRUSH => Ok(()),
        Brush::Capsule { a, b, radius }
            if a.is_finite()
                && b.is_finite()
                && radius <= MOST_BRUSH
                && (b - a).length() <= MOST_TUNNEL =>
        {
            Ok(())
        }
        _ => Err(format!(
            "a brush larger than a tool's ({MOST_BRUSH} m, tunnels {MOST_TUNNEL} m)"
        )),
    }
}

/// The items of `v` whose bodies `map` kept.
pub(super) fn kept<T: Clone>(map: &[Option<usize>], v: &[T]) -> Vec<T> {
    map.iter()
        .zip(v)
        .filter(|(m, _)| m.is_some())
        .map(|(_, x)| x.clone())
        .collect()
}
