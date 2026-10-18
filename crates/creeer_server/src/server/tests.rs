//! Whole games: clients joining a server in process or over a simulated bad network, seeing
//! its world, moving their avatars, changing the world for each other, within the budget.

use super::*;
use crate::client::{Connection, Status};
use crate::world::tests::{terrain, world};
use creeer_math::glam::DVec3;
use creeer_net::entity::Kind;
use creeer_net::link::{Conditions, Relay, SimulatedLink};
use creeer_net::message::Command;

const STEP: Duration = Duration::from_nanos(16_666_667);

/// A server and its clients (some through a relay), stepped together in lockstep.
pub(crate) struct Game {
    pub server: Server,
    pub clients: Vec<(Connection, Option<Relay>)>,
    pub now: Duration,
}

impl Game {
    pub fn new(world: ServerWorld) -> Self {
        Self {
            server: Server::new(world),
            clients: Vec::new(),
            now: Duration::ZERO,
        }
    }

    /// A client named `name`, through a link with `conditions` if any; returns its index.
    pub fn join(&mut self, name: &str, conditions: Option<Conditions>) -> usize {
        let end = self.server.connect_memory();
        let (end, relay) = match conditions {
            Some(c) => {
                let (relay, end) =
                    Relay::new(end, SimulatedLink::new(c, 7 + self.clients.len() as u64));
                (end, Some(relay))
            }
            None => (end, None),
        };
        let mut client = Connection::memory(end, name, creeer_data::Data::base().hash());
        client.attach_terrain(terrain(), &creeer_data::Data::base());
        self.clients.push((client, relay));
        self.clients.len() - 1
    }

    /// One tick, every client asking `input(index)` of its avatar.
    pub fn tick(&mut self, input: impl Fn(usize) -> (DVec3, bool)) {
        for (k, (client, relay)) in self.clients.iter_mut().enumerate() {
            if let Some(relay) = relay {
                relay.pump(self.now);
            }
            client.update(STEP);
            let (walk, jump) = input(k);
            client.input(walk, jump, DVec3::ZERO);
            client.flush();
            if let Some(relay) = relay {
                relay.pump(self.now);
            }
        }
        self.server.tick();
        for (_, relay) in &mut self.clients {
            if let Some(relay) = relay {
                relay.pump(self.now);
            }
        }
        self.now += STEP;
    }

    pub fn run(&mut self, ticks: usize, input: impl Fn(usize) -> (DVec3, bool)) {
        for _ in 0..ticks {
            self.tick(&input);
        }
    }

    /// The server's avatar of client `k`.
    pub fn avatar(&self, k: usize) -> DVec3 {
        let id = self.clients[k].0.welcome.as_ref().expect("joined").avatar;
        let i = self.server.world.index(id).expect("in the world");
        self.server.world.world.bodies[i].structure.position[0]
    }
}

fn still(_: usize) -> (DVec3, bool) {
    (DVec3::ZERO, false)
}

/// Crates on the ground in a ring around where avatars appear.
fn crates(world: &mut ServerWorld, n: usize, radius: f64) {
    let up = world.spawn().normalize();
    let (east, north) = creeer_terrain::geology::deposits::tangent_frame(up);
    for k in 0..n {
        #[allow(clippy::cast_precision_loss)]
        let angle = std::f64::consts::TAU * k as f64 / n as f64;
        let (s, c) = creeer_math::det::sin_cos(angle);
        let at = world.ground_under(world.spawn() + (east * c + north * s) * radius);
        world
            .apply(&Command::Crate(at + at.normalize() * 0.6), None)
            .unwrap();
    }
}

#[test]
fn a_player_joins_sees_the_world_and_walks_in_it() {
    let mut w = world();
    crates(&mut w, 6, 8.0);
    let mut game = Game::new(w);
    game.join("ada", None);
    game.run(90, still);
    let client = &game.clients[0].0;
    assert_eq!(client.status(), &Status::Joined);
    let avatar = client.welcome.as_ref().unwrap().avatar;
    assert!(matches!(&client.kinds[&avatar], Kind::Character { player } if player == "ada"));
    let boxes = client
        .kinds
        .values()
        .filter(|k| matches!(k, Kind::Body(_)))
        .count();
    assert_eq!(boxes, 6);
    // Each crate drawn where the server has it (they rest: to the quantisation).
    let poses = client.poses();
    for (id, body) in game
        .server
        .world
        .ids()
        .iter()
        .zip(&game.server.world.world.bodies)
    {
        let off = (poses[id].position - body.structure.position[0]).length();
        assert!(off < 0.05, "{id:?} drawn {off} m off");
    }
    // Two seconds walking east at 2 m/s: predicted at once, and the server agrees.
    let up = game.avatar(0).normalize();
    let (east, _) = creeer_terrain::geology::deposits::tangent_frame(up);
    let start = game.avatar(0);
    game.run(120, |_| (east * 2.0, false));
    let walked = (game.avatar(0) - start).dot(east);
    assert!(walked > 3.0, "walked {walked} m");
    let predicted = game.clients[0].0.avatar().unwrap().position;
    // The prediction is ahead of the server by the inputs on their way (none here) and the
    // jitter buffer's two ticks: 2 m/s · 33 ms.
    let ahead = (predicted - game.avatar(0)).length();
    assert!(ahead < 0.2, "predicted {ahead} m from the server");
}

#[test]
fn what_one_player_does_the_other_sees() {
    let mut game = Game::new(world());
    game.join("ada", None);
    game.join("bob", None);
    game.run(30, still);
    let at = game.server.world.ground_under(
        game.avatar(0) + game.avatar(0).normalize().cross(DVec3::Y).normalize() * 5.0,
    );
    game.clients[0]
        .0
        .command(Command::Crate(at + at.normalize() * 0.6));
    let mut edits = creeer_terrain::edits::Edits::default();
    edits.push(creeer_terrain::edits::Edit::Dig(
        creeer_terrain::edits::Brush::Sphere {
            center: at,
            radius: 2.0,
        },
    ));
    game.clients[0].0.command(Command::Edit(edits.to_bytes()));
    game.run(30, still);
    for (client, _) in &game.clients {
        let boxes = client
            .kinds
            .values()
            .filter(|k| matches!(k, Kind::Body(_)))
            .count();
        assert_eq!(boxes, 1);
        assert_eq!(client.edits.len(), 1);
        // The other avatar too.
        let avatars = client
            .kinds
            .values()
            .filter(|k| matches!(k, Kind::Character { .. }))
            .count();
        assert_eq!(avatars, 2);
    }
    // The dig is in each client's terrain: the ground at its centre is gone.
    for (client, _) in &game.clients {
        let density = client.terrain().unwrap().density(at);
        assert!(density > 1.0, "{density}");
    }
}

#[test]
fn a_name_in_use_or_too_long_is_refused_and_told_why() {
    let mut game = Game::new(world());
    game.join("ada", None);
    game.join("ada", None);
    game.join(&"x".repeat(LONGEST_NAME + 1), None);
    game.run(20, still);
    assert_eq!(game.clients[0].0.status(), &Status::Joined);
    assert!(matches!(game.clients[1].0.status(), Status::Refused(why) if why.contains("playing")));
    assert!(
        matches!(game.clients[2].0.status(), Status::Refused(why) if why.contains("characters"))
    );
    game.run(60, still);
    assert_eq!(game.server.players(), vec!["ada"]);
}

#[test]
fn a_client_with_other_data_is_refused_and_told_the_servers_packs() {
    let mut game = Game::new(world());
    let end = game.server.connect_memory();
    let mut other = Connection::memory(end, "bea", creeer_data::Data::base().hash() ^ 1);
    other.attach_terrain(terrain(), &creeer_data::Data::base());
    game.clients.push((other, None));
    game.run(20, still);
    let Status::Refused(why) = game.clients[0].0.status() else {
        panic!("joined: {:?}", game.clients[0].0.status())
    };
    assert!(
        why.contains("data differs") && why.contains("base 0.0.1"),
        "{why}"
    );
}

#[test]
fn a_player_who_comes_again_gets_their_avatar_back() {
    let mut game = Game::new(world());
    game.join("ada", None);
    game.run(30, still);
    let up = game.avatar(0).normalize();
    let (east, _) = creeer_terrain::geology::deposits::tangent_frame(up);
    game.run(60, |_| (east * 3.0, false));
    let id = game.clients[0].0.welcome.as_ref().unwrap().avatar;
    game.clients.clear();
    game.run(60, still);
    assert!(game.server.players().is_empty());
    let left_at = game.server.world.world.bodies[game.server.world.index(id).unwrap()]
        .structure
        .position[0];
    game.join("ada", None);
    game.run(30, still);
    assert_eq!(game.clients[0].0.welcome.as_ref().unwrap().avatar, id);
    assert!((game.avatar(0) - left_at).length() < 0.5);
}

#[test]
fn a_saved_world_loaded_again_is_told_to_every_player() {
    let mut w = world();
    crates(&mut w, 3, 6.0);
    let mut game = Game::new(w);
    game.join("ada", None);
    game.run(30, still);
    let save = game.server.save();
    crates(&mut game.server.world, 5, 10.0);
    game.run(30, still);
    let count = |game: &Game| {
        game.clients[0]
            .0
            .kinds
            .values()
            .filter(|k| matches!(k, Kind::Body(_)))
            .count()
    };
    assert_eq!(count(&game), 8);
    game.server.load(save).unwrap();
    game.run(30, still);
    assert_eq!(count(&game), 3);
    assert_eq!(game.clients[0].0.status(), &Status::Joined);
}

#[test]
fn a_player_joins_over_udp() {
    let mut server = Server::new(world());
    let at = server.listen("127.0.0.1:0".parse().unwrap()).unwrap();
    let mut client = Connection::udp(at, "ada", creeer_data::Data::base().hash()).unwrap();
    client.attach_terrain(terrain(), &creeer_data::Data::base());
    let start = std::time::Instant::now();
    while client.status() != &Status::Joined || client.kinds.is_empty() {
        assert!(
            start.elapsed().as_secs() < 10,
            "not joined: {:?}",
            client.status()
        );
        client.update(STEP);
        client.input(DVec3::ZERO, false, DVec3::ZERO);
        client.flush();
        server.tick();
        std::thread::sleep(Duration::from_millis(2));
    }
    assert_eq!(server.players(), vec!["ada"]);
    let avatar = client.welcome.as_ref().unwrap().avatar;
    assert!(matches!(&client.kinds[&avatar], Kind::Character { .. }));
}

#[test]
fn a_client_that_asks_is_sent_the_servers_profile_once_a_second() {
    let mut w = world();
    crates(&mut w, 4, 8.0);
    let mut game = Game::new(w);
    game.join("ada", None);
    game.run(30, still);
    assert!(game.clients[0].0.profile.is_none(), "not asked");
    game.clients[0].0.watch_profile(true);
    game.run(150, still);
    let report = game.clients[0].0.profile.clone().expect("a report");
    assert!(!report.durations.is_empty() && report.durations.len() <= 600);
    let names = &report.worst.names;
    // (The longest spans: in a test run, other tests' servers fill the same frames.)
    for phase in ["server tick", "physics"] {
        assert!(names.iter().any(|n| n == phase), "{phase} in {names:?}");
    }
    // Asked no more: profiling stops (as far as this server wants it).
    game.clients[0].0.watch_profile(false);
    game.run(10, still);
    assert!(game.server.profiling.watchers.is_empty());
    // (Reports queued before may still come, paced by its budget: other tests' servers
    // stepping meanwhile fill them; none after.)
    game.run(240, still);
    game.clients[0].0.profile = None;
    game.run(130, still);
    assert!(game.clients[0].0.profile.is_none());
}
