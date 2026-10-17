//! The codec reads back what it wrote, bit for bit, and refuses what it did not write.

use super::*;
use crate::glam::{DMat3, DQuat, DVec3};

#[test]
fn values_come_back_bit_for_bit() {
    let mut w = Writer::new();
    w.u8(7);
    w.u16(0xbeef);
    w.u32(0xdead_beef);
    w.u64(u64::MAX - 3);
    w.f64(-0.1);
    w.f64(f64::MIN_POSITIVE / 4.0); // subnormal
    w.put(&DVec3::new(1.0e300, -2.5, 1.0 / 3.0));
    w.put(&DQuat::from_rotation_y(0.3));
    w.bytes_with_length(b"creeer");
    w.put(&vec![DVec3::X, DVec3::NEG_Z]);
    let bytes = w.finish();
    let mut r = Reader::new(&bytes);
    assert_eq!(r.u8(), Ok(7));
    assert_eq!(r.u16(), Ok(0xbeef));
    assert_eq!(r.u32(), Ok(0xdead_beef));
    assert_eq!(r.u64(), Ok(u64::MAX - 3));
    assert_eq!(r.f64().map(f64::to_bits), Ok((-0.1f64).to_bits()));
    assert_eq!(
        r.f64().map(f64::to_bits),
        Ok((f64::MIN_POSITIVE / 4.0).to_bits())
    );
    assert_eq!(r.get(), Ok(DVec3::new(1.0e300, -2.5, 1.0 / 3.0)));
    assert_eq!(r.get(), Ok(DQuat::from_rotation_y(0.3)));
    assert_eq!(r.bytes_with_length(), Ok(&b"creeer"[..]));
    assert_eq!(r.get(), Ok(vec![DVec3::X, DVec3::NEG_Z]));
    assert_eq!(r.end(), Ok(()));
}

#[test]
fn varints_take_as_few_bytes_as_their_value_needs() {
    for (v, len) in [
        (0u64, 1),
        (127, 1),
        (128, 2),
        (16_383, 2),
        (16_384, 3),
        (u64::MAX, 10),
    ] {
        let mut w = Writer::new();
        w.varint(v);
        let bytes = w.finish();
        assert_eq!(bytes.len(), len, "{v}");
        assert_eq!(Reader::new(&bytes).varint(), Ok(v));
    }
    for v in [0i64, 1, -1, 63, -64, 64, -65, i64::MAX, i64::MIN] {
        let mut w = Writer::new();
        w.signed(v);
        let bytes = w.finish();
        assert_eq!(Reader::new(&bytes).signed(), Ok(v));
        if (-64..64).contains(&v) {
            assert_eq!(bytes.len(), 1, "{v}");
        }
    }
}

#[test]
fn what_it_did_not_write_is_refused_where_it_goes_wrong() {
    // Ended early.
    assert_eq!(
        Reader::new(&[1, 2, 3]).u32(),
        Err(DecodeError {
            at: 0,
            what: "bytes ended early"
        })
    );
    // A varint longer than 64 bits.
    let long = [0xff; 11];
    assert!(Reader::new(&long).varint().is_err());
    // A count far beyond the bytes left must not allocate it.
    let mut w = Writer::new();
    w.varint(1 << 40);
    let bytes = w.finish();
    assert!(Reader::new(&bytes).get::<Vec<DVec3>>().is_err());
    // A float that is not finite where one is required.
    let mut w = Writer::new();
    w.f64(f64::NAN);
    let bytes = w.finish();
    assert!(Reader::new(&bytes).finite().is_err());
    // Bytes after the end.
    let r = Reader::new(&[0]);
    assert_eq!(r.end().map_err(|e| e.what), Err("bytes after the end"));
}

#[test]
fn matrices_flags_indices_and_options_come_back() {
    let m = DMat3::from_cols_array(&[1.0, -2.0, 3.5, 1e-300, 0.0, -0.0, 7.0, 8.0, 1.0 / 3.0]);
    let values = (
        m,
        [0.25f64, -1.0, 9.0],
        true,
        70_000u32,
        1usize << 40,
        Some(DVec3::X),
        None::<f64>,
    );
    let mut w = Writer::new();
    w.put(&values.0);
    w.put(&values.1);
    w.put(&values.2);
    w.put(&values.3);
    w.put(&values.4);
    w.put(&values.5);
    w.put(&values.6);
    let bytes = w.finish();
    let mut r = Reader::new(&bytes);
    assert_eq!(
        r.get::<DMat3>().unwrap().to_cols_array().map(f64::to_bits),
        m.to_cols_array().map(f64::to_bits)
    );
    assert_eq!(r.get::<[f64; 3]>().unwrap(), values.1);
    assert!(r.get::<bool>().unwrap());
    assert_eq!(r.get::<u32>().unwrap(), values.3);
    assert_eq!(r.get::<usize>().unwrap(), values.4);
    assert_eq!(r.get::<Option<DVec3>>().unwrap(), values.5);
    assert_eq!(r.get::<Option<f64>>().unwrap(), None);
    r.end().unwrap();
    // A flag or option tag that is neither is refused.
    assert!(Reader::new(&[2]).get::<bool>().is_err());
    assert!(Reader::new(&[7]).get::<Option<f64>>().is_err());
}
