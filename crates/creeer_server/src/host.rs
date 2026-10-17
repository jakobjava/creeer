//! Running a server: on a thread of its own at [`TICK_RATE`] ticks a second, saving and
//! loading on request. A singleplayer game hosts its world so (its client joined to it by
//! an in-process connection); the dedicated server binary runs the same loop.

use crate::server::{Event, Server};
use crate::world::ServerWorld;
use creeer_core::TimingStats;
use creeer_net::channel::TICK_RATE;
use creeer_sim::save::Save;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{Receiver, Sender, channel};
use std::sync::{Arc, Mutex, PoisonError};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

/// Writes `save` to `path` whole or not at all: to a file beside it first, then moved over
/// it (a crash while saving leaves the last save as it was).
///
/// # Errors
/// If the file cannot be written.
pub fn write_save(path: &Path, save: &Save) -> std::io::Result<()> {
    let mut beside = path.as_os_str().to_owned();
    beside.push(".part");
    let beside = PathBuf::from(beside);
    std::fs::write(&beside, save.to_bytes())?;
    std::fs::rename(&beside, path)
}

/// Reads a save (of any version this game reads).
///
/// # Errors
/// If the file cannot be read or is no save this game reads.
pub fn read_save(path: &Path) -> Result<Save, String> {
    let bytes = std::fs::read(path).map_err(|e| format!("{}: {e}", path.display()))?;
    Save::from_bytes(&bytes).map_err(|e| format!("{}: {e}", path.display()))
}

impl Server {
    /// The world as a save.
    pub fn save(&self) -> Save {
        self.world.to_save()
    }

    /// Replaces the world by the one `save` holds (of the same planet): every player gets
    /// their avatar in it (the one they left in it, or a new one) and is told the world anew.
    ///
    /// # Errors
    /// If the save is of another planet than the world's.
    pub fn load(&mut self, save: Save) -> Result<(), String> {
        if save.planet != self.world.planet {
            return Err(format!(
                "a save of {}, not of {}",
                save.planet, self.world.planet
            ));
        }
        let terrain = std::sync::Arc::clone(&self.world.terrain);
        let data = std::sync::Arc::clone(&self.world.data);
        self.world = ServerWorld::from_save(data, save, terrain);
        self.restart_sessions();
        Ok(())
    }
}

/// What the host's thread is asked.
enum Control {
    Save(Sender<Save>),
    Players(Sender<Vec<String>>),
    Load(Box<Save>, Sender<Result<(), String>>),
    Stop,
}

/// A server running on a thread of its own.
pub struct Host {
    control: Sender<Control>,
    events: Receiver<Event>,
    thread: Option<JoinHandle<()>>,
    timing: Arc<Timing>,
}

/// How the server's ticks go: their durations, and how many.
#[derive(Debug)]
struct Timing {
    ticks: Mutex<TimingStats>,
    tick: AtomicU64,
}

impl std::fmt::Debug for Host {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Host").finish_non_exhaustive()
    }
}

impl Host {
    /// Runs `server` from now on, at [`TICK_RATE`] ticks a second.
    pub fn spawn(mut server: Server) -> Self {
        let (control, orders) = channel();
        let (tell, events) = channel();
        let timing = Arc::new(Timing {
            ticks: Mutex::new(TimingStats::new(240)),
            tick: AtomicU64::new(0),
        });
        let shared = Arc::clone(&timing);
        let thread = std::thread::Builder::new()
            .name("creeer server".into())
            .spawn(move || run(&mut server, &orders, (&tell, &shared)))
            .expect("a thread for the server");
        Self {
            control,
            events,
            thread: Some(thread),
            timing,
        }
    }

    /// The world as it is now, as a save.
    ///
    /// # Errors
    /// If the server's thread has ended.
    pub fn save(&self) -> Result<Save, String> {
        let (reply, answer) = channel();
        self.control
            .send(Control::Save(reply))
            .map_err(|_| "the server stopped")?;
        answer.recv().map_err(|_| "the server stopped".into())
    }

    /// Replaces the world by `save`'s.
    ///
    /// # Errors
    /// If the save is of another planet, or the server's thread has ended.
    pub fn load(&self, save: Save) -> Result<(), String> {
        let (reply, answer) = channel();
        self.control
            .send(Control::Load(Box::new(save), reply))
            .map_err(|_| "the server stopped")?;
        answer.recv().map_err(|_| "the server stopped".to_owned())?
    }

    /// The players in the game.
    ///
    /// # Errors
    /// If the server's thread has ended.
    pub fn players(&self) -> Result<Vec<String>, String> {
        let (reply, answer) = channel();
        self.control
            .send(Control::Players(reply))
            .map_err(|_| "the server stopped")?;
        answer.recv().map_err(|_| "the server stopped".into())
    }

    /// What happened since the last call.
    pub fn events(&self) -> Vec<Event> {
        self.events.try_iter().collect()
    }

    /// How long its last ticks took.
    pub fn tick_times(&self) -> TimingStats {
        self.timing
            .ticks
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }

    /// Ticks run so far.
    pub fn tick(&self) -> u64 {
        self.timing.tick.load(Ordering::Relaxed)
    }
}

impl Drop for Host {
    fn drop(&mut self) {
        let _ = self.control.send(Control::Stop);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

/// Ticks `server` in real time until told to stop, doing what it is asked between ticks.
/// Ticks that fall behind (a slow machine) are caught up a few at a time, and given up past
/// a quarter of a second rather than run ever later.
fn run(server: &mut Server, orders: &Receiver<Control>, (tell, timing): (&Sender<Event>, &Timing)) {
    let tick = Duration::from_secs_f64(1.0 / f64::from(TICK_RATE));
    let mut next = Instant::now();
    loop {
        for order in orders.try_iter() {
            match order {
                Control::Save(reply) => {
                    let _ = reply.send(server.save());
                }
                Control::Players(reply) => {
                    let _ = reply.send(server.players().into_iter().map(str::to_owned).collect());
                }
                Control::Load(save, reply) => {
                    let _ = reply.send(server.load(*save));
                }
                Control::Stop => return,
            }
        }
        let now = Instant::now();
        if now < next {
            std::thread::sleep(next - now);
            continue;
        }
        if now > next + tick * 15 {
            next = now;
        }
        let started = Instant::now();
        server.tick();
        timing
            .ticks
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push(started.elapsed());
        timing.tick.fetch_add(1, Ordering::Relaxed);
        for event in server.events() {
            let _ = tell.send(event);
        }
        next += tick;
    }
}
