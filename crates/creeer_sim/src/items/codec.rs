//! Items and stores as written: in saves and to clients' servers (`creeer_math::codec`).

use super::{Bulk, Form, Item, Piece, Store};
use creeer_math::codec::{Decode, DecodeError, Encode, Reader, Writer};

fn text(w: &mut Writer, s: &str) {
    w.bytes_with_length(s.as_bytes());
}

fn read_text(r: &mut Reader<'_>) -> Result<String, DecodeError> {
    let bytes = r.bytes_with_length()?;
    String::from_utf8(bytes.to_vec()).map_err(|_| r.error("text not UTF-8"))
}

impl Encode for Item {
    fn encode(&self, w: &mut Writer) {
        match self {
            Self::Piece(p, inside) => {
                w.u8(0);
                text(w, &p.material);
                w.f64(p.density);
                match p.form {
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
                }
                w.f64(p.wall);
                w.f64(p.wear);
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
        }
    }
}

impl Decode for Item {
    fn decode(r: &mut Reader<'_>) -> Result<Self, DecodeError> {
        Ok(match r.u8()? {
            0 => {
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
                    _ => return Err(r.error("unknown form")),
                };
                let piece = Piece {
                    material,
                    density,
                    form,
                    wall: r.finite()?,
                    wear: r.finite()?,
                };
                Self::Piece(piece, r.get()?)
            }
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
