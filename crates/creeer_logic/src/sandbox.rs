//! The Lua environment a microcontroller runs in: no files, no clock, no bytecode, nothing that
//! differs from machine to machine or run to run.
//!
//! - **Left out:** `io`, `os`, `debug`, `package`/`require`, `load`, `loadfile`, `dofile`,
//!   `collectgarbage`, `string.dump`, and `next` (`pairs` iterates instead).
//! - **`pairs`** visits keys in the table's own order, which the fixed string hash seed makes
//!   the same everywhere (`vendor/lua-src`), for keys that are numbers, strings or booleans. A
//!   table keyed by tables or functions would be visited in the order of their addresses: it
//!   is an error instead.
//! - **`tostring`, `print`, `string.format`** name a table or function by its type, never by
//!   its address (`%p` is refused).
//! - **`math`**: Lua's own for what IEEE arithmetic fixes (floor, fmod, sqrt, …); the game's
//!   own (`libm`) for the rest; `math.random` the script's own generator, seeded by the game.
//! - **Stops cannot be caught.** A script out of instructions or memory stops for good: `pcall`,
//!   `xpcall` and `coroutine.resume` raise those again instead of returning them.

use mlua::{Lua, Result, Value, Variadic};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

/// Lines kept of what a script prints.
const LOG_LINES: usize = 64;

/// Run once, with `__stopped` and `__print` given, before the script.
const PRELUDE: &str = r##"
local stopped, print_line = ...
local rawnext, rawtostring, rawpcall, rawxpcall = next, tostring, pcall, xpcall
local type, error, getmetatable, select = type, error, getmetatable, select
local pack, unpack, concat = table.pack, table.unpack, table.concat
local rawformat, gsub, find = string.format, string.gsub, string.find
local resume = coroutine.resume

local function iterate(t, k)
  local key, value = rawnext(t, k)
  if key ~= nil then
    local kind = type(key)
    if kind ~= "string" and kind ~= "number" and kind ~= "boolean" then
      error("pairs: a table keyed by a " .. kind .. " has no defined order", 2)
    end
  end
  return key, value
end
function pairs(t)
  local meta = getmetatable(t)
  if type(meta) == "table" and meta.__pairs then return meta.__pairs(t) end
  return iterate, t, nil
end

local function named(v)
  local kind = type(v)
  if kind == "table" or kind == "function" or kind == "thread" or kind == "userdata" then
    local meta = getmetatable(v)
    if type(meta) == "table" and meta.__tostring then return rawtostring(v) end
    return kind
  end
  return v
end
function tostring(v) return rawtostring(named(v)) end
function print(...)
  local n = select("#", ...)
  local parts = {}
  for i = 1, n do parts[i] = tostring((select(i, ...))) end
  print_line(concat(parts, "\t"))
end
function string.format(format, ...)
  if find(gsub(format, "%%%%", ""), "%%[%-%+ #0-9%.]*p") then
    error("string.format: %p gives an address", 2)
  end
  local args = pack(...)
  for i = 1, args.n do args[i] = named(args[i]) end
  return rawformat(format, unpack(args, 1, args.n))
end

-- Out of instructions or memory: raised again, never returned.
local function again(ok, ...)
  if not ok then
    if stopped() then error("out of its instruction budget", 0) end
    if (...) == "not enough memory" then error("not enough memory", 0) end
  end
  return ok, ...
end
function pcall(...) return again(rawpcall(...)) end
function xpcall(...) return again(rawxpcall(...)) end
function coroutine.resume(...) return again(resume(...)) end

next, load, loadfile, dofile, collectgarbage, require = nil, nil, nil, nil, nil, nil
string.dump = nil
"##;

/// Installs the environment in `lua`: `stopped` tells whether the script ran out of its
/// budget, `log` collects what it prints, `seed` seeds `math.random`.
pub(crate) fn install(
    lua: &Lua,
    stopped: Arc<AtomicBool>,
    log: Arc<Mutex<Vec<String>>>,
    rng: Arc<Mutex<[u64; 4]>>,
) -> Result<()> {
    let is_stopped = lua.create_function(move |_, ()| Ok(stopped.load(Ordering::Relaxed)))?;
    let print_line = lua.create_function(move |_, line: String| {
        let mut log = log
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if log.len() == LOG_LINES {
            log.remove(0);
        }
        log.push(line);
        Ok(())
    })?;
    lua.load(PRELUDE)
        .set_name("=sandbox")
        .call::<()>((is_stopped, print_line))?;
    install_math(lua, rng)
}

/// The game's maths in `math`, and its random numbers.
fn install_math(lua: &Lua, rng: Arc<Mutex<[u64; 4]>>) -> Result<()> {
    let math: mlua::Table = lua.globals().get("math")?;
    let one = |f: fn(f64) -> f64| lua.create_function(move |_, x: f64| Ok(f(x)));
    math.set("sin", one(libm::sin)?)?;
    math.set("cos", one(libm::cos)?)?;
    math.set("tan", one(libm::tan)?)?;
    math.set("asin", one(libm::asin)?)?;
    math.set("acos", one(libm::acos)?)?;
    math.set("exp", one(libm::exp)?)?;
    math.set(
        "atan",
        lua.create_function(|_, (y, x): (f64, Option<f64>)| Ok(libm::atan2(y, x.unwrap_or(1.0))))?,
    )?;
    math.set(
        "log",
        lua.create_function(|_, (x, base): (f64, Option<f64>)| {
            // Lua's own special bases, exactly.
            Ok(match base {
                None => libm::log(x),
                Some(2.0) => libm::log2(x),
                Some(10.0) => libm::log10(x),
                Some(b) => libm::log(x) / libm::log(b),
            })
        })?,
    )?;
    let state = Arc::clone(&rng);
    math.set(
        "random",
        lua.create_function(move |_, bounds: Variadic<i64>| {
            let mut s = state
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            random(&mut s, &bounds)
        })?,
    )?;
    math.set(
        "randomseed",
        lua.create_function(move |_, seed: Option<i64>| {
            let mut s = rng
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            #[allow(clippy::cast_sign_loss)] // the seed's bits
            if let Some(seed) = seed {
                *s = seeded(seed as u64);
            }
            Ok(())
        })?,
    )?;
    Ok(())
}

/// Lua's `math.random`: a float in [0, 1), an integer in [1, m] or in [m, n].
fn random(state: &mut [u64; 4], bounds: &[i64]) -> Result<Value> {
    let (low, high) = match *bounds {
        [] => {
            #[allow(clippy::cast_precision_loss)] // 53 bits, exactly
            let x = (next(state) >> 11) as f64 * (1.0 / 9_007_199_254_740_992.0);
            return Ok(Value::Number(x));
        }
        [0] => return Ok(Value::Integer(next(state).cast_signed())),
        [m] => (1, m),
        [m, n] => (m, n),
        _ => {
            return Err(mlua::Error::runtime(
                "math.random: wrong number of arguments",
            ));
        }
    };
    if low > high {
        return Err(mlua::Error::runtime("math.random: interval is empty"));
    }
    let span = high.wrapping_sub(low).cast_unsigned();
    let draw = if span == u64::MAX {
        next(state)
    } else {
        // Uniform by rejection: drop the draws past the last whole multiple of the range.
        let range = span + 1;
        let limit = u64::MAX - u64::MAX % range;
        loop {
            let r = next(state);
            if r < limit {
                break r % range;
            }
        }
    };
    Ok(Value::Integer(low.wrapping_add(draw.cast_signed())))
}

/// xoshiro256** (Blackman and Vigna), as Lua 5.4's own generator.
fn next(s: &mut [u64; 4]) -> u64 {
    let result = s[1].wrapping_mul(5).rotate_left(7).wrapping_mul(9);
    let t = s[1] << 17;
    s[2] ^= s[0];
    s[3] ^= s[1];
    s[1] ^= s[2];
    s[0] ^= s[3];
    s[2] ^= t;
    s[3] = s[3].rotate_left(45);
    result
}

/// The generator's state from a seed (splitmix64).
pub(crate) fn seeded(mut seed: u64) -> [u64; 4] {
    std::array::from_fn(|_| {
        seed = seed.wrapping_add(0x9e37_79b9_7f4a_7c15);
        let mut z = seed;
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        z ^ (z >> 31)
    })
}
