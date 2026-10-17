//! Lua microcontrollers: they run, a runaway script and a memory hog stop for good (however they
//! try to catch it), the sandbox has nothing that differs between machines or runs, `^` and
//! `math` are the game's own, and a run hashes the same everywhere.

use super::*;

/// A script with one input and `outputs` outputs, run `ticks` times on input `x`.
fn run(source: &str, outputs: usize, ticks: usize, x: f64) -> (Script, Vec<Option<Signal>>) {
    let mut script = Script::new(source, 1, outputs, Limits::default(), 1);
    let mut last = vec![None; outputs];
    for _ in 0..ticks {
        last = script.run(&[Signal::Number(x)], 1.0 / 60.0);
    }
    (script, last)
}

#[test]
fn a_script_reads_its_inputs_and_sets_its_outputs() {
    let source = "function tick(dt) output[1] = input[1] * 2; output[2] = input[1] > 1 end";
    let (script, out) = run(source, 2, 3, 1.5);
    assert_eq!(script.halted(), None);
    assert_eq!(out, [Some(Signal::Number(3.0)), Some(Signal::Bool(true))]);
    let (script, _) = run("x = 1", 1, 1, 0.0);
    assert!(
        script.halted().is_some_and(|h| h.contains("tick")),
        "{:?}",
        script.halted()
    );
    let (script, _) = run("function tick( end", 1, 1, 0.0);
    assert!(script.halted().is_some(), "a syntax error stops it");
}

/// A script that never ends stops within its budget, and stays stopped: neither `pcall`,
/// `xpcall` nor a coroutine catches it.
#[test]
fn a_runaway_script_is_stopped_by_its_budget() {
    let runaways = [
        "function tick(dt) while true do end end",
        "function tick(dt) while true do pcall(function() while true do end end) end end",
        "function tick(dt) while true do xpcall(function() while true do end end, print) end end",
        "function tick(dt)\n  while true do\n    local co = coroutine.create(function() while true do end end)\n    coroutine.resume(co)\n  end\nend",
    ];
    for source in runaways {
        let (script, out) = run(source, 1, 3, 0.0);
        let why = script.halted().unwrap_or_default();
        assert!(why.contains("instruction budget"), "{source}: {why:?}");
        assert_eq!(out, [None]);
    }
    // Endless recursion caught each time: its stack fills the memory first.
    let (script, _) = run(
        "function f() return f() + 1 end function tick(dt) while true do pcall(f) end end",
        1,
        3,
        0.0,
    );
    assert!(
        script.halted().is_some_and(|h| h.contains("memory")),
        "{:?}",
        script.halted()
    );
    // A loop at the top level stops while the script starts (within a hundred ticks' budget).
    let (script, _) = run("while true do end", 1, 1, 0.0);
    assert!(
        script
            .halted()
            .is_some_and(|h| h.contains("instruction budget"))
    );
}

/// A script that takes ever more memory stops at its limit, and cannot catch that either.
#[test]
fn a_memory_hog_is_stopped_by_its_limit() {
    let hogs = [
        "function tick(dt) local t = {} for i = 1, 1e9 do t[i] = i end end",
        "function tick(dt) local s = string.rep('x', 1e8) end",
        "function tick(dt) while true do pcall(function() local t = {} for i = 1, 1e9 do t[i] = {} end end) end end",
    ];
    for source in hogs {
        let mut script = Script::new(
            source,
            0,
            0,
            Limits {
                instructions: 100_000_000,
                memory: 1 << 20,
            },
            1,
        );
        script.run(&[], 0.1);
        let why = script.halted().unwrap_or_default();
        assert!(why.contains("memory"), "{source}: {why:?}");
        assert!(script.memory_used() <= 1 << 20);
    }
}

#[test]
fn the_sandbox_has_no_way_out() {
    let source = "function tick(dt)\n\
        output[1] = io == nil and os == nil and debug == nil and package == nil\n\
        output[2] = load == nil and loadfile == nil and dofile == nil and require == nil\n\
        output[3] = string.dump == nil and next == nil and collectgarbage == nil\n\
        output[4] = tostring({}) == 'table' and tostring(print) == 'function'\n\
        output[5] = not pcall(string.format, '%p', {}) and string.format('%s', {}) == 'table'\n\
        output[6] = not pcall(function() for k in pairs({[{}] = 1}) do end end)\nend";
    let (script, out) = run(source, 6, 1, 0.0);
    assert_eq!(script.halted(), None);
    for (k, v) in out.into_iter().enumerate() {
        assert_eq!(v, Some(Signal::Bool(true)), "check {}", k + 1);
    }
    let (script, _) = run("function tick(dt) print('a', 1, {}, true) end", 0, 2, 0.0);
    assert_eq!(script.log(), ["a\t1\ttable\ttrue", "a\t1\ttable\ttrue"]);
}

/// `^` is the game's pow and `math` the game's maths, bit for bit.
#[test]
fn powers_and_maths_are_the_games_own() {
    let source = "function tick(dt)\n  local x = input[1]\n\
        output[1] = x ^ 1.37\n  output[2] = (x * 0.731) ^ -2.5\n  output[3] = math.sin(x)\n\
        output[4] = math.exp(x / 7)\n  output[5] = math.log(x, 10)\n  output[6] = math.atan(x, 3)\nend";
    let mut script = Script::new(source, 1, 6, Limits::default(), 1);
    let mut x = 0.013;
    for _ in 0..2000 {
        x = x * 1.0071 + 0.37;
        let out = script.run(&[Signal::Number(x)], 0.1);
        let want = [
            libm::pow(x, 1.37),
            libm::pow(x * 0.731, -2.5),
            libm::sin(x),
            libm::exp(x / 7.0),
            libm::log10(x),
            libm::atan2(x, 3.0),
        ];
        for (k, (got, want)) in out.into_iter().zip(want).enumerate() {
            let got = got.expect("set").number();
            assert_eq!(
                got.to_bits(),
                want.to_bits(),
                "output {} at {x}: {got} {want}",
                k + 1
            );
        }
    }
}

/// A run of gates and scripts that use string keys in `pairs`, `^`, `math` and random numbers,
/// formatted numbers and a table's growth hashes to one value on every platform and in every
/// run (regenerate it only for a deliberate change of the logic, saying why).
#[test]
fn a_run_hashes_the_same_everywhere() {
    let source = "memory.words = {alpha = 1, beta = 2, gamma = 3, delta = 4, epsilon = 5}\n\
        function tick(dt)\n  local s, i = '', 0\n\
        for k, v in pairs(memory.words) do i = i + 1; s = s .. k .. v end\n\
        memory.words['w' .. math.random(1, 50)] = i\n\
        output[1] = #s + math.random() + input[1] ^ 0.5 + math.cos(i)\n\
        output[2] = tonumber(string.format('%.6g', output[1] / 3))\nend";
    let mut net = Network::new();
    let clock = net.add(Kind::Pulse(0.25));
    let not = net.add(Kind::Not);
    net.wire((not, 0), clock, 0);
    net.wire((clock, 0), not, 0);
    let count = net.add(Kind::Add);
    net.wire((count, 0), count, 0);
    net.wire((clock, 0), count, 1);
    let scripts: Vec<BlockId> = (0..4)
        .map(|seed| {
            let s = net.add_script(Script::new(source, 1, 2, Limits::default(), seed));
            net.wire((count, 0), s, 0);
            s
        })
        .collect();
    let mut hash = 0u64;
    for _ in 0..300 {
        net.tick(1.0 / 60.0);
        for &s in &scripts {
            for k in 0..2 {
                hash = hash.rotate_left(13) ^ net.value((s, k)).bits();
            }
        }
    }
    assert_eq!(hash, 0x65d3_0974_3d57_9ef9, "logic hash {hash:#018x}");
}
