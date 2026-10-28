//! The players of `--play-bench` (`play_bench.rs`): what a knapper and a feller do, frame by
//! frame, from what their connection knows (as a player sees it).

use crate::planet_view::PlanetView;
use creeer_math::glam::DVec3;
use creeer_net::entity::Kind;
use creeer_net::message::Command;
use creeer_server::client::Aimed;
use creeer_sim::trees::{Part, Tree};

/// What the scripted player does next.
#[derive(Debug, Default)]
pub(crate) struct Player {
    /// Seconds since joined.
    pub(crate) t: f64,
    /// When it last struck (s), and whether it holds its stone (the knapper) or its axe.
    struck: f64,
    holding: bool,
    /// The tree it fells.
    tree: Option<u64>,
}

/// A walk along the ground from `from` towards `to`, stopping `near` m from it (m/s).
fn towards(from: DVec3, to: DVec3, near: f64) -> DVec3 {
    let up = from.normalize();
    let d = (to - from).reject_from(up);
    if d.length() > near {
        d.normalize() * (d.length() - near).clamp(0.3, 1.2)
    } else {
        DVec3::ZERO
    }
}

/// The knapper's walk and acts this frame: where it looks.
pub(crate) fn knap(view: &mut PlanetView, p: &mut Player, me: DVec3) -> (DVec3, DVec3) {
    let c = &view.session.connection;
    // The station: the stones laid nearest the avatar's start, and where to stand before them.
    let mut stones: Vec<_> = (c.laid.values())
        .filter(|l| l.word == "stone" || l.word == "worked stone")
        .map(|l| (l.id, l.centre, l.mass))
        .collect();
    stones.sort_by(|a, b| (a.1 - me).length().total_cmp(&(b.1 - me).length()));
    let Some(&(_, centre, _)) = stones.first() else {
        return (DVec3::ZERO, me);
    };
    let near: Vec<_> = (stones.iter())
        .filter(|s| (s.1 - centre).length() < 0.8)
        .copied()
        .collect();
    let up = centre.normalize();
    let stand = centre - (centre - me).reject_from(up).normalize_or_zero() * 0.7;
    let walk = if p.t < 2.0 {
        towards(me, stand, 0.15)
    } else {
        DVec3::ZERO
    };
    if p.t > 2.5 && !p.holding {
        if let Some(h) = near.iter().min_by(|a, b| a.2.total_cmp(&b.2)) {
            let pick = Command::Pick {
                loose: h.0,
                pocket: false,
            };
            view.session.command(pick);
        }
        p.holding = true;
        p.struck = p.t;
    } else if p.holding && p.t - p.struck > 2.2 && p.t < 14.0 {
        if let Some(core) = near.iter().max_by(|a, b| a.2.total_cmp(&b.2)) {
            view.session.connection.strike_loose(core.0);
        }
        p.struck = p.t;
    }
    (walk, centre)
}

/// What the feller does this frame.
enum Blow {
    None,
    Tree(u64),
    Part(Aimed),
}

/// The feller's walk and acts this frame: where it looks.
pub(crate) fn fell(view: &mut PlanetView, p: &mut Player, me: DVec3) -> (DVec3, DVec3) {
    // The steel axe, once (commands are rate-limited: one a frame crowded out the blows).
    if !p.holding {
        view.session.command(Command::Wield { tool: Some(2) });
        p.holding = true;
    }
    let (walk, look, blow) = felling(view, p, me);
    if !matches!(blow, Blow::None) && walk == DVec3::ZERO && p.t - p.struck > 2.1 {
        match blow {
            Blow::Tree(id) => view.session.connection.strike_tree(id, 0.7),
            Blow::Part(aimed) => view.session.connection.strike(aimed),
            Blow::None => {}
        }
        p.struck = p.t;
    }
    (walk, look)
}

fn felling(view: &PlanetView, p: &mut Player, me: DVec3) -> (DVec3, DVec3, Blow) {
    let (Some(trees), c) = (&view.trees, &view.session.connection) else {
        return (DVec3::ZERO, me, Blow::None);
    };
    if p.tree.is_none() {
        let notched = (c.trees.values()).filter(|t| !t.felled).filter_map(|t| {
            let (s, _) = trees.standing(t.id)?;
            Some((t.id, (s.base - me).length()))
        });
        p.tree = notched.min_by(|a, b| a.1.total_cmp(&b.1)).map(|t| t.0);
    }
    let Some((standing, forest)) = p.tree.and_then(|id| trees.standing(id)) else {
        return (DVec3::ZERO, me, Blow::None);
    };
    let felled = c.trees.get(&standing.id).is_some_and(|t| t.felled);
    if !felled {
        let r = forest.grow(&standing).radius(0.7);
        let stand = standing.base + standing.turn * (DVec3::NEG_X * (r + 0.6));
        let cut = standing.base + standing.turn * DVec3::new(0.0, 0.0, 0.7);
        return (towards(me, stand, 0.15), cut, Blow::Tree(standing.id));
    }
    // Limbing: the nearest node with branches still on, of the parts in view.
    let poses = c.poses();
    let mut nodes = Vec::new();
    for (id, kind) in &c.kinds {
        let (Kind::Tree(t), Some(pose)) = (kind, poses.get(id)) else {
            continue;
        };
        let species = &forest.species[usize::from(t.species)];
        let tree = Tree::grow(species, t.seed, t.dbh);
        let part = Part {
            species: t.species,
            seed: t.seed,
            dbh: t.dbh,
            from: t.from,
            to: t.to,
            cut: t.cut.clone(),
        };
        for (k, n) in tree.nodes.iter().enumerate() {
            if n.z >= part.from && n.z < part.to && part.off(k) < n.branches.len() {
                let at = pose.position + pose.orientation * (DVec3::new(0.0, 0.0, n.z) - t.centre);
                nodes.push((*id, at));
            }
        }
    }
    let Some(&(id, at)) = nodes
        .iter()
        .min_by(|a, b| (a.1 - me).length().total_cmp(&(b.1 - me).length()))
    else {
        return (DVec3::ZERO, me, Blow::None);
    };
    let up = at.normalize();
    let aimed = c
        .aim(at + up * 2.0, -up, 3.0)
        .filter(|a| a.body == Some(id));
    let aimed = aimed.unwrap_or(Aimed {
        body: Some(id),
        point: at,
        distance: 0.0,
        seen: c.drawn_at(),
    });
    (towards(me, at, 1.0), at, Blow::Part(aimed))
}
