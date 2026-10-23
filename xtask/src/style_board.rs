//! `cargo xtask style-board [--save]` (G1.3h): renders the style board, every ground at noon and
//! in the evening, the walk sheet and the icons, into `target/style-board`, and checks it
//! against the approved golden images in `tests/golden/style-board` by how different they look
//! (`creeer_client`'s `golden.rs`). `--save` makes the current images the golden ones: only
//! for an intended change, once the user has approved it.

use std::process::{Command, ExitCode};

const OUT: &str = "target/style-board";
const GOLDEN: &str = "tests/golden/style-board";

pub fn style_board(args: &[&str]) -> ExitCode {
    let mode = match args {
        [] => "--check",
        ["--save"] => "--save",
        _ => {
            eprintln!("usage: cargo xtask style-board [--save]");
            return ExitCode::FAILURE;
        }
    };
    let status = Command::new(env!("CARGO"))
        .args(["run", "--release", "-p", "creeer_client", "--"])
        .args(["--style-board", OUT, "--all", mode, GOLDEN])
        .status();
    match status {
        Ok(s) if s.success() => {
            if mode == "--save" {
                println!("saved the golden images in {GOLDEN}");
            }
            ExitCode::SUCCESS
        }
        _ => {
            eprintln!("xtask style-board: the images in {OUT} differ from {GOLDEN}");
            ExitCode::FAILURE
        }
    }
}
