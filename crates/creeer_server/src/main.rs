//! The dedicated server: a world on a planet, players joining it over UDP.
//!
//! `creeer_server [--port <n>] [--planet <data file>] [--save <file>] [--autosave <s>]
//! [--ticks <n>] [--data <dir>] [--mods <dir>]`
//!
//! - `--port`: the UDP port to listen on (default 27015);
//! - `--planet`: the planet's file in the data packs (default `planets/home.ron`; a save's
//!   own planet wins);
//! - `--data`, `--mods`: where the base pack and the mods are (`creeer_data::locate`); clients
//!   must have the same data (the packs' hash is checked when they join);
//! - `--save`: the world's file: loaded if it is there (saves of every earlier version are
//!   read), saved every `--autosave` seconds (default 60) and when the server stops;
//! - `--ticks`: run so many ticks as fast as it can, save, and stop (for scripts and tests).
//!
//! While it runs it reads commands from its console: `save`, `players`, `quit`.

use creeer_data::{Data, Location};
use creeer_server::host::{Host, read_save, write_save};
use creeer_server::{Event, Server, ServerWorld};
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::mpsc::RecvTimeoutError;
use std::time::{Duration, Instant};

struct Options {
    data: Arc<Data>,
    port: u16,
    planet: String,
    save: Option<PathBuf>,
    autosave: Duration,
    ticks: Option<u64>,
}

fn value<'a>(args: &'a [String], flag: &str) -> Option<&'a str> {
    let i = args.iter().position(|a| a == flag)?;
    args.get(i + 1).map(String::as_str)
}

fn options() -> Result<Options, String> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let number = |flag: &str| -> Result<Option<u64>, String> {
        value(&args, flag)
            .map(|v| {
                v.parse()
                    .map_err(|_| format!("{flag} needs a number, not {v}"))
            })
            .transpose()
    };
    let location = Location::find(&args);
    let data = Arc::new(location.open().map_err(|e| e.to_string())?);
    Ok(Options {
        data,
        port: number("--port")?.map_or(Ok(27015), |p| {
            u16::try_from(p).map_err(|_| "--port: 0 to 65535")
        })?,
        planet: value(&args, "--planet")
            .unwrap_or("planets/home.ron")
            .to_owned(),
        save: value(&args, "--save").map(PathBuf::from),
        autosave: Duration::from_secs(number("--autosave")?.unwrap_or(60)),
        ticks: number("--ticks")?,
    })
}

fn world(options: &Options) -> Result<ServerWorld, String> {
    match &options.save {
        Some(path) if path.exists() => {
            let save = read_save(path)?;
            let terrain = creeer_server::world::load_terrain(&options.data, &save.planet)?;
            println!(
                "loaded {} (tick {}, {} bodies)",
                path.display(),
                save.tick,
                save.world.bodies.len()
            );
            Ok(ServerWorld::from_save(
                Arc::clone(&options.data),
                save,
                terrain,
            ))
        }
        _ => {
            let terrain = creeer_server::world::load_terrain(&options.data, &options.planet)?;
            println!("a new world on {}", options.planet);
            Ok(ServerWorld::new(
                Arc::clone(&options.data),
                terrain,
                &options.planet,
            ))
        }
    }
}

fn save(options: &Options, save: &creeer_sim::save::Save) {
    if let Some(path) = &options.save {
        match write_save(path, save) {
            Ok(()) => println!("saved {}", path.display()),
            Err(e) => eprintln!("could not save {}: {e}", path.display()),
        }
    }
}

fn show(event: &Event) {
    match event {
        Event::Joined(name) => println!("{name} joined"),
        Event::Left(name) => println!("{name} left"),
        Event::Refused(client, why) => println!("refused client {client:x}: {why}"),
        Event::CommandRefused(name, why) => println!("refused {name}'s command: {why}"),
    }
}

fn main() -> Result<(), String> {
    let options = options()?;
    for pack in options.data.packs() {
        println!(
            "pack {} {} ({})",
            pack.name(),
            pack.manifest.version,
            pack.root.display()
        );
    }
    let mut server = Server::new(world(&options)?);
    if let Some(ticks) = options.ticks {
        for _ in 0..ticks {
            server.tick();
        }
        save(&options, &server.save());
        return Ok(());
    }
    let at = server
        .listen(SocketAddr::from(([0, 0, 0, 0], options.port)))
        .map_err(|e| format!("could not listen on port {}: {e}", options.port))?;
    println!("listening on {at}; commands: save, players, quit");
    let host = Host::spawn(server);
    let (lines, console) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        for line in std::io::stdin().lines().map_while(Result::ok) {
            if lines.send(line).is_err() {
                break;
            }
        }
    });
    let mut saved = Instant::now();
    loop {
        for event in host.events() {
            show(&event);
        }
        if options.save.is_some() && saved.elapsed() >= options.autosave {
            save(&options, &host.save()?);
            saved = Instant::now();
        }
        match console.recv_timeout(Duration::from_millis(100)) {
            Ok(line) => match line.trim() {
                "save" => save(&options, &host.save()?),
                "players" => println!("{} playing", host.players()?.join(", ")),
                "quit" => break,
                "" => {}
                other => println!("unknown command {other:?}: save, players, quit"),
            },
            Err(RecvTimeoutError::Timeout) => {}
            // The console closed (run without one): run on until killed.
            Err(RecvTimeoutError::Disconnected) => std::thread::sleep(Duration::from_millis(100)),
        }
    }
    save(&options, &host.save()?);
    Ok(())
}
