//! A script's `memory` table as a save keeps it: its data (booleans, numbers, strings and
//! tables of them, keyed by numbers, strings or booleans), in the order `pairs` visits it.
//! Functions, coroutines and the like are not data: they are left out, as are tables nested
//! deeper than [`DEPTH`] (a table that holds itself would never end).

use creeer_math::codec::{Reader, Writer};
use mlua::{Lua, Table, Value};

/// How deep tables nest in a save.
const DEPTH: usize = 32;

/// `memory` as bytes.
pub(crate) fn write(memory: &Value) -> Vec<u8> {
    let mut w = Writer::new();
    if let Value::Table(t) = memory {
        table(t, &mut w, 0);
    } else {
        w.varint(0);
    }
    w.finish()
}

fn table(t: &Table, w: &mut Writer, depth: usize) {
    let entries: Vec<(Value, Value)> = t
        .pairs::<Value, Value>()
        .filter_map(Result::ok)
        .filter(|(k, v)| is_key(k) && is_data(v, depth))
        .collect();
    w.varint(entries.len() as u64);
    for (k, v) in &entries {
        value(k, w, depth);
        value(v, w, depth);
    }
}

fn is_key(v: &Value) -> bool {
    matches!(
        v,
        Value::Boolean(_) | Value::Integer(_) | Value::Number(_) | Value::String(_)
    )
}

fn is_data(v: &Value, depth: usize) -> bool {
    is_key(v) || (matches!(v, Value::Table(_)) && depth + 1 < DEPTH)
}

fn value(v: &Value, w: &mut Writer, depth: usize) {
    match v {
        Value::Boolean(b) => w.u8(u8::from(*b)),
        Value::Integer(i) => {
            w.u8(2);
            w.signed(*i);
        }
        Value::Number(x) => {
            w.u8(3);
            w.f64(*x);
        }
        Value::String(s) => {
            w.u8(4);
            w.bytes_with_length(&s.as_bytes());
        }
        Value::Table(t) => {
            w.u8(5);
            table(t, w, depth + 1);
        }
        _ => unreachable!("only data is written"),
    }
}

/// The `memory` table from bytes written by [`write`] (an empty one from none).
pub(crate) fn read(lua: &Lua, bytes: &[u8]) -> mlua::Result<Table> {
    if bytes.is_empty() {
        return lua.create_table();
    }
    let mut r = Reader::new(bytes);
    let bad = |e: creeer_math::codec::DecodeError| mlua::Error::runtime(format!("memory: {e}"));
    let t = read_table(lua, &mut r).map_err(bad)?;
    r.end().map_err(bad)?;
    Ok(t)
}

fn read_table(lua: &Lua, r: &mut Reader<'_>) -> Result<Table, creeer_math::codec::DecodeError> {
    let t = lua
        .create_table()
        .map_err(|_| r.error("no memory for a table"))?;
    for _ in 0..r.count(2)? {
        let k = read_value(lua, r)?;
        let v = read_value(lua, r)?;
        t.raw_set(k, v).map_err(|_| r.error("not a key"))?;
    }
    Ok(t)
}

fn read_value(lua: &Lua, r: &mut Reader<'_>) -> Result<Value, creeer_math::codec::DecodeError> {
    Ok(match r.u8()? {
        0 => Value::Boolean(false),
        1 => Value::Boolean(true),
        2 => Value::Integer(r.signed()?),
        3 => Value::Number(r.f64()?),
        4 => {
            let bytes = r.bytes_with_length()?;
            Value::String(lua.create_string(bytes).map_err(|_| r.error("no memory"))?)
        }
        5 => Value::Table(read_table(lua, r)?),
        _ => return Err(r.error("not a value")),
    })
}
