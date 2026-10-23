//! *lumberjack* (G1.10): four players in the forest, 45 m from the spawn's clearing, each with
//! the steel axe at a tree of 12–20 cm. The trees stand notched half through on their far side,
//! as a feller leaves them before the back cut (a notch takes minutes of blows, more than a
//! scene's 30 s): each player cuts in from the near side until its tree goes over, then bucks
//! its trunk where it lies. Trees fall, hinge, roll and come apart while the others work.

use super::Play;
use super::record::Act;
use super::scenes::world;
use super::work::{apart, at, towards};
use crate::ServerWorld;
use creeer_math::glam::DVec3;
use creeer_net::message::Command;
use creeer_sim::trees::Standing;
use creeer_terrain::Terrain;
use creeer_terrain::geology::deposits::tangent_frame;
use std::sync::Arc;

/// How far from the spawn the forest is worked (m: past the clearing), and how high the cut is.
const OUT: f64 = 45.0;
const CUT: f64 = 0.7;
/// The trees' girths (m at breast height), and how far apart they stand at least (m).
const GIRTH: (f64, f64) = (0.12, 0.20);
const APART: f64 = 6.0;
/// The ticks between a player's blows (a swing's cycle and a little).
const EVERY: u32 = 125;

/// The world: the spawn moved into the forest, and the four trees notched from the east.
pub fn lumber_world(terrain: Arc<Terrain>) -> ServerWorld {
    let mut w = world(terrain);
    let (east, _) = tangent_frame(w.spawn().normalize());
    let there = w.ground_under(w.spawn() + east * OUT);
    w.set_spawn(there);
    for t in trees(&w) {
        assert!(w.notch_tree(t.id, CUT, 0.0), "a standing tree");
    }
    w
}

/// The four trees worked, nearest the spawn first, each apart from the others.
pub fn trees(w: &ServerWorld) -> Vec<Standing> {
    let Some(forest) = w.forest() else {
        return Vec::new();
    };
    let spawn = w.spawn();
    let mut near: Vec<Standing> = (forest.cells_near(spawn, 30.0).into_iter())
        .flat_map(|c| forest.in_cell(&w.terrain, c))
        .filter(|t| (GIRTH.0..GIRTH.1).contains(&t.dbh))
        .collect();
    near.sort_by(|a, b| {
        let d = |t: &Standing| (t.base - spawn).length();
        d(a).total_cmp(&d(b)).then(a.id.cmp(&b.id))
    });
    let mut chosen: Vec<Standing> = Vec::new();
    for t in near {
        if chosen.iter().all(|c| (c.base - t.base).length() > APART) {
            chosen.push(t);
        }
        if chosen.len() == 4 {
            break;
        }
    }
    chosen
}

/// *lumberjack*'s acts this tick.
pub fn lumberjack(tick: u32, play: &Play) -> Vec<Act> {
    let w = &play.server.world;
    let trees = trees(w);
    let parts = w.tree_parts();
    (0..play.players())
        .map(|k| {
            let Some(t) = trees.get(k) else {
                return Act::default();
            };
            let me = play.avatar(k);
            let index = u32::try_from(k).unwrap_or(0);
            let mine = tick % EVERY == (31 * index) % EVERY;
            if tick == 1 {
                let mut act = Act::default();
                act.commands.push(Command::Wield { tool: Some(2) });
                return act;
            }
            if !w.felled(t.id) {
                return felling(play, k, t, me, mine);
            }
            // Its trunk where it lies (the parts in order: the k-th, or any left).
            let part = parts.get(k).or(parts.last());
            part.map_or_else(Act::default, |&id| bucking(play, k, id, me, mine))
        })
        .collect()
}

/// To the tree's west, its notch's far side, and a blow into it every cycle.
fn felling(play: &Play, k: usize, t: &Standing, me: DVec3, mine: bool) -> Act {
    let r = play
        .server
        .world
        .forest()
        .map_or(0.1, |f| f.grow(t).radius(CUT));
    let stand = t.base + t.turn * (DVec3::NEG_X * (r + 0.6));
    let mut act = Act::walk(towards(me, stand, 0.15));
    if mine && apart(me, stand) < 0.5 {
        act.commands
            .push(play.client(k).strike_tree_command(t.id, CUT));
    }
    act
}

/// To the felled trunk `id`, and a blow on its middle every cycle.
fn bucking(play: &Play, k: usize, id: creeer_net::entity::EntityId, me: DVec3, mine: bool) -> Act {
    let Some(there) = at(play, id) else {
        return Act::default();
    };
    let mut act = Act::walk(towards(me, there, 1.2));
    if mine && apart(me, there) < 1.6 {
        let client = play.client(k);
        let up = there.normalize();
        if let Some(a) = client
            .aim(there + up * 3.0, -up, 5.0)
            .filter(|a| a.body == Some(id))
        {
            act.commands.push(client.strike_command(a));
        }
    }
    act
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::play::scenes::{scene, terrain};

    /// The whole scene: every tree goes over, and blows then fall on the trunks where they lie.
    #[test]
    fn the_lumberjacks_fell_their_trees_and_buck_them() {
        let s = scene("lumberjack").expect("the scene");
        let mut play = Play::new((s.build)(terrain()), s.name, s.players, &terrain());
        let chosen = trees(&play.server.world);
        assert_eq!(chosen.len(), 4, "four trees to fell");
        let (mut asked, mut fell, mut on_trunks) = (0, 0, 0);
        for t in 0..s.ticks.0 {
            let acts = (s.script)(t, &play);
            asked += acts.iter().map(|a| a.commands.len()).sum::<usize>();
            play.tick(&acts);
            let blows = play.server.world.take_blows();
            let parts = play.server.world.tree_parts();
            on_trunks += blows
                .iter()
                .filter(|b| b.on.is_some_and(|on| parts.contains(&on)))
                .count();
            fell += blows.len();
        }
        let w = &play.server.world;
        let felled = chosen.iter().filter(|t| w.felled(t.id)).count();
        println!("{asked} asked, {fell} blows, {on_trunks} on trunks; {felled} felled");
        assert_eq!(felled, 4, "every tree went over");
        assert!(on_trunks >= 4, "blows on the trunks where they lie");
    }
}

#[cfg(test)]
mod probe {
    use crate::play::scenes::{scene, terrain};

    #[test]
    #[ignore = "prints whether the felled parts come to rest"]
    fn felled_parts_rest() {
        let s = scene("lumberjack").unwrap();
        let mut play = crate::play::Play::new((s.build)(terrain()), s.name, s.players, &terrain());
        for t in 0..3000 {
            let acts = if t < 1200 {
                (s.script)(t, &play)
            } else {
                vec![super::Act::default(); 4]
            };
            let c = play.tick(&acts);
            if t % 150 == 0 {
                let w = &play.server.world;
                let asleep: Vec<String> = (w.tree_parts().iter())
                    .map(|id| {
                        let b = &w.world.bodies[w.index(*id).unwrap()];
                        let s = &b.structure;
                        format!(
                            "{} v{:.3} w{:.3} h{}",
                            b.sleep.asleep,
                            s.velocity[0].length(),
                            s.angular_velocity[0].length(),
                            w.hinged_part(*id)
                        )
                    })
                    .collect();
                println!("tick {t}: {:.1} ms; parts asleep {asleep:?}", c.ms);
            }
        }
    }
}
