//! Items and stores as written: in saves and to clients' servers (`creeer_math::codec`).

use super::{Assembly, Bulk, Edge, Form, Item, Part, Piece, Store};
use creeer_math::codec::{Decode, DecodeError, Encode, Reader, Writer};

fn text(w: &mut Writer, s: &str) {
    w.bytes_with_length(s.as_bytes());
}

fn read_text(r: &mut Reader<'_>) -> Result<String, DecodeError> {
    let bytes = r.bytes_with_length()?;
    String::from_utf8(bytes.to_vec()).map_err(|_| r.error("text not UTF-8"))
}

impl Encode for Piece {
    fn encode(&self, w: &mut Writer) {
        text(w, &self.material);
        w.f64(self.density);
        match self.form {
            Form::Block { size } => {
                w.u8(0);
                w.put(&size);
            }
            Form::Ball { radius } => {
                w.u8(1);
                w.f64(radius);
            }
            Form::Rod { radius, length } => {
                w.u8(2);
                w.f64(radius);
                w.f64(length);
            }
            Form::Lump { size } => {
                w.u8(3);
                w.put(&size);
            }
        }
        w.f64(self.wall);
        w.f64(self.wear);
        match self.edge {
            Some(e) => {
                w.u8(1);
                w.put(&[e.angle, e.length, e.radius]);
            }
            None => w.u8(0),
        }
    }
}

impl Decode for Piece {
    fn decode(r: &mut Reader<'_>) -> Result<Self, DecodeError> {
        let material = read_text(r)?;
        let density = r.finite()?;
        let form = match r.u8()? {
            0 => Form::Block { size: r.get()? },
            1 => Form::Ball {
                radius: r.finite()?,
            },
            2 => Form::Rod {
                radius: r.finite()?,
                length: r.finite()?,
            },
            3 => Form::Lump { size: r.get()? },
            _ => return Err(r.error("unknown form")),
        };
        let (wall, wear) = (r.finite()?, r.finite()?);
        let edge = match r.u8()? {
            0 => None,
            1 => {
                let [angle, length, radius]: [f64; 3] = r.get()?;
                Some(Edge {
                    angle,
                    length,
                    radius,
                })
            }
            _ => return Err(r.error("not a flag")),
        };
        Ok(Self {
            material,
            density,
            form,
            wall,
            wear,
            edge,
        })
    }
}

impl Encode for Item {
    fn encode(&self, w: &mut Writer) {
        match self {
            Self::Piece(p, inside) => {
                w.u8(0);
                w.put(p);
                w.put(inside);
            }
            Self::Bulk(b) => {
                w.u8(1);
                w.f64(b.mass);
                w.varint(b.composition.len() as u64);
                for (name, f) in &b.composition {
                    text(w, name);
                    w.f64(*f);
                }
                w.f64(b.temperature);
                w.f64(b.specific_heat);
                w.f64(b.grain);
                w.f64(b.bulk_density);
            }
            Self::Made(a) => {
                w.u8(2);
                w.varint(a.parts.len() as u64);
                for p in &a.parts {
                    w.put(&p.piece);
                    w.put(&p.at);
                    w.put(&p.turn);
                }
                w.put(&a.bindings);
                w.f64(a.hold);
            }
        }
    }
}

impl Decode for Item {
    fn decode(r: &mut Reader<'_>) -> Result<Self, DecodeError> {
        Ok(match r.u8()? {
            0 => Self::Piece(r.get()?, r.get()?),
            1 => {
                let mass = r.finite()?;
                let composition = (0..r.count(9)?)
                    .map(|_| Ok((read_text(r)?, r.finite()?)))
                    .collect::<Result<_, DecodeError>>()?;
                Self::Bulk(Bulk {
                    mass,
                    composition,
                    temperature: r.finite()?,
                    specific_heat: r.finite()?,
                    grain: r.finite()?,
                    bulk_density: r.finite()?,
                })
            }
            2 => {
                let parts = (0..r.count(40)?)
                    .map(|_| {
                        Ok(Part {
                            piece: r.get()?,
                            at: r.get()?,
                            turn: r.get()?,
                        })
                    })
                    .collect::<Result<_, DecodeError>>()?;
                let bindings = r.get()?;
                // (The most a joint holds may be unbounded: a single piece's.)
                let hold = r.f64()?;
                Self::Made(Box::new(Assembly {
                    parts,
                    bindings,
                    hold,
                }))
            }
            _ => return Err(r.error("unknown item")),
        })
    }
}

impl Encode for Store {
    fn encode(&self, w: &mut Writer) {
        text(w, &self.name);
        w.put(&self.size);
        w.put(&self.items);
    }
}

impl Decode for Store {
    fn decode(r: &mut Reader<'_>) -> Result<Self, DecodeError> {
        Ok(Self {
            name: read_text(r)?,
            size: r.get()?,
            items: r.get()?,
        })
    }
}
