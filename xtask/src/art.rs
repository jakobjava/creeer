//! `cargo xtask art [--check] [--blend] [--board <out.png>] [name…]` (G1.3b,
//! `docs/blender-mcp.md`): builds models from their scripts in `art/` with headless Blender,
//! into `data/models/<name>.glb`.
//!
//! - `--check`: builds into `target/art/` instead and fails if any differs from the committed
//!   file (a script changed without its export, or an export made by hand);
//! - `--blend`: also saves `target/art/<name>.blend`, to open by hand;
//! - `--board <out.png>`: renders the style board in Blender instead (`art/board.py`), the
//!   preview of the surfaces to set beside `creeer_client --style-board`.
//!
//! Blender is found through the `BLENDER` environment variable, or at its default install
//! path, and must be the pinned version: a new Blender must not change a model unnoticed.

use creeer_art::Palette;
use creeer_art::model::Model;
use creeer_data::Data;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};

/// The Blender every model is built with (its `--version` begins with this).
const VERSION: &str = "Blender 5.2.";
/// Scripts in `art/` that are not models.
const NOT_MODELS: [&str; 5] = ["bake", "board", "build", "common", "preview"];

fn default_blender() -> PathBuf {
    if cfg!(windows) {
        PathBuf::from(r"C:\Program Files\Blender Foundation\Blender 5.2\blender.exe")
    } else if cfg!(target_os = "macos") {
        PathBuf::from("/Applications/Blender.app/Contents/MacOS/Blender")
    } else {
        PathBuf::from("blender")
    }
}

/// Blender, checked to be the pinned version.
fn blender() -> Result<PathBuf, String> {
    let path = std::env::var_os("BLENDER").map_or_else(default_blender, PathBuf::from);
    let out = Command::new(&path)
        .arg("--version")
        .output()
        .map_err(|e| format!("Blender not found at {} ({e}); set BLENDER", path.display()))?;
    let version = String::from_utf8_lossy(&out.stdout);
    let first = version.lines().next().unwrap_or("").trim();
    if !first.starts_with(VERSION) {
        return Err(format!(
            "{} is {first:?}; models are built with {VERSION}x",
            path.display()
        ));
    }
    Ok(path)
}

/// Every model script in `art/`, by name.
fn models() -> Vec<String> {
    let mut names: Vec<String> = std::fs::read_dir("art")
        .map(|entries| {
            entries
                .filter_map(Result::ok)
                .filter_map(|e| {
                    let path = e.path();
                    let stem = path.file_stem()?.to_str()?.to_owned();
                    let python = path.extension().is_some_and(|x| x == "py");
                    (python && !NOT_MODELS.contains(&stem.as_str()) && !stem.starts_with('_'))
                        .then_some(stem)
                })
                .collect()
        })
        .unwrap_or_default();
    names.sort();
    names
}

/// Runs art/build.py for `name`, writing `glb` (and `blend`).
fn build(blender: &Path, name: &str, glb: &Path, blend: Option<&Path>) -> bool {
    let mut args = vec![name.as_ref(), glb.as_os_str()];
    args.extend(blend.map(Path::as_os_str));
    run(blender, "art/build.py", &args)
}

/// Runs `script` in a clean headless Blender with `args` after its `--`, printing its
/// `art: ` lines (and everything when it fails).
fn run(blender: &Path, script: &str, args: &[&std::ffi::OsStr]) -> bool {
    let mut command = Command::new(blender);
    command.args([
        "--background",
        "--factory-startup",
        "--python",
        script,
        "--",
    ]);
    command.args(args);
    // Python randomises string hashing per process; fixed, sets of names iterate alike.
    command.env("PYTHONHASHSEED", "0");
    match command.output() {
        Ok(out) => {
            let text = String::from_utf8_lossy(&out.stdout);
            text.lines()
                .filter(|l| l.starts_with("art: "))
                .for_each(|l| println!("{l}"));
            if !out.status.success() {
                eprintln!("{text}{}", String::from_utf8_lossy(&out.stderr));
            }
            out.status.success()
        }
        Err(e) => {
            eprintln!("could not run Blender: {e}");
            false
        }
    }
}

/// Whether the models in files `a` and `b` are the same geometry. Not their bytes: Blender's
/// normals differ in their last bits between runs, and with them how the exporter shares
/// vertices (measured: one build in six; ADR-0026).
fn same(a: &Path, b: &Path) -> Result<bool, String> {
    let palette = Palette::from_data(&Data::base())?;
    let read = |path: &Path| {
        let bytes = std::fs::read(path).map_err(|e| format!("{}: {e}", path.display()))?;
        Model::read(&bytes, &palette).map_err(|e| format!("{}: {e}", path.display()))
    };
    let (x, y) = (read(a)?.canonical(), read(b)?.canonical());
    // Equal to a unit of their quantisation: a value on a rounding boundary may go either way.
    Ok(x.len() == y.len()
        && x.iter().zip(&y).all(|(s, t)| {
            s.iter()
                .flatten()
                .zip(t.iter().flatten())
                .all(|(p, q)| (p - q).abs() <= 1)
        }))
}

pub fn art(args: &[&str]) -> ExitCode {
    let check = args.contains(&"--check");
    let blend = args.contains(&"--blend");
    let blender = match blender() {
        Ok(b) => b,
        Err(e) => {
            eprintln!("{e}");
            return ExitCode::FAILURE;
        }
    };
    if let Some(i) = args.iter().position(|a| *a == "--board") {
        let Some(out) = args.get(i + 1) else {
            eprintln!("--board needs a file name");
            return ExitCode::FAILURE;
        };
        return if run(&blender, "art/board.py", &[out.as_ref()]) {
            ExitCode::SUCCESS
        } else {
            ExitCode::FAILURE
        };
    }
    let all = models();
    let wanted: Vec<String> = args
        .iter()
        .filter(|a| !a.starts_with("--"))
        .map(|a| (*a).to_owned())
        .collect();
    if let Some(unknown) = wanted.iter().find(|w| !all.contains(w)) {
        eprintln!(
            "no model script art/{unknown}.py (models: {})",
            all.join(", ")
        );
        return ExitCode::FAILURE;
    }
    let names = if wanted.is_empty() { all } else { wanted };
    let scratch = Path::new("target/art");
    let mut failed = Vec::new();
    for name in &names {
        let committed = Path::new("data/models").join(format!("{name}.glb"));
        let glb = if check {
            scratch.join(format!("{name}.glb"))
        } else {
            committed.clone()
        };
        let blend_path = blend.then(|| scratch.join(format!("{name}.blend")));
        if !build(&blender, name, &glb, blend_path.as_deref()) {
            failed.push(format!("{name}: the build failed"));
            continue;
        }
        if check {
            match same(&glb, &committed) {
                Ok(true) => {}
                Ok(false) => failed.push(format!(
                    "{name}: {} differs from what art/{name}.py makes (run `cargo xtask art {name}`)",
                    committed.display()
                )),
                Err(e) => failed.push(format!("{name}: {e}")),
            }
        }
    }
    if failed.is_empty() {
        println!(
            "xtask art: {} model(s) {}",
            names.len(),
            if check {
                "match their scripts"
            } else {
                "built"
            }
        );
        ExitCode::SUCCESS
    } else {
        failed.iter().for_each(|f| eprintln!("{f}"));
        ExitCode::FAILURE
    }
}
