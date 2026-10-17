//! A small binary codec for saves and the network: little-endian integers, floats bit for bit
//! (a loaded world is the saved one, and a replicated value is the sent one), LEB128 varints
//! (zigzagged when signed) for counts and small numbers, and length-prefixed byte strings.
//! Writing never fails; reading checks every length and says where it stopped.

/// Bytes being written.
#[derive(Clone, Debug, Default)]
pub struct Writer {
    bytes: Vec<u8>,
}

/// Why bytes could not be read: what was being read, and at which byte.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DecodeError {
    /// The byte where reading stopped.
    pub at: usize,
    /// What was being read.
    pub what: &'static str,
}

impl std::fmt::Display for DecodeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} at byte {}", self.what, self.at)
    }
}

impl std::error::Error for DecodeError {}

/// Bytes being read.
#[derive(Clone, Debug)]
pub struct Reader<'a> {
    bytes: &'a [u8],
    at: usize,
    /// The version of the format the bytes were written in (a decoder of a value whose
    /// layout changed reads the old layout for an old version).
    version: u32,
}

/// A value that can be written.
pub trait Encode {
    /// Writes the value.
    fn encode(&self, w: &mut Writer);
}

/// A value that can be read back.
pub trait Decode: Sized {
    /// Reads a value.
    ///
    /// # Errors
    /// If the bytes end too soon or hold something no value is written as.
    fn decode(r: &mut Reader<'_>) -> Result<Self, DecodeError>;
}

impl Writer {
    /// Nothing written yet.
    pub fn new() -> Self {
        Self::default()
    }

    /// The bytes written so far.
    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }

    /// The bytes written.
    pub fn finish(self) -> Vec<u8> {
        self.bytes
    }

    /// Writes raw bytes, without their length.
    pub fn raw(&mut self, bytes: &[u8]) {
        self.bytes.extend_from_slice(bytes);
    }

    /// Writes a byte.
    pub fn u8(&mut self, v: u8) {
        self.bytes.push(v);
    }

    /// Writes 16 bits.
    pub fn u16(&mut self, v: u16) {
        self.raw(&v.to_le_bytes());
    }

    /// Writes 32 bits.
    pub fn u32(&mut self, v: u32) {
        self.raw(&v.to_le_bytes());
    }

    /// Writes 64 bits.
    pub fn u64(&mut self, v: u64) {
        self.raw(&v.to_le_bytes());
    }

    /// Writes a float bit for bit.
    pub fn f64(&mut self, v: f64) {
        self.u64(v.to_bits());
    }

    /// Writes an unsigned number in as few bytes as it needs (LEB128: 7 bits a byte).
    pub fn varint(&mut self, mut v: u64) {
        while v >= 0x80 {
            #[allow(clippy::cast_possible_truncation)] // the low seven bits
            self.u8((v as u8 & 0x7f) | 0x80);
            v >>= 7;
        }
        #[allow(clippy::cast_possible_truncation)] // below 0x80
        self.u8(v as u8);
    }

    /// Writes a signed number as a varint, small magnitudes in few bytes (zigzag).
    pub fn signed(&mut self, v: i64) {
        #[allow(clippy::cast_sign_loss)] // zigzag: the sign moves to the lowest bit
        self.varint(((v << 1) ^ (v >> 63)) as u64);
    }

    /// Writes a byte string with its length.
    pub fn bytes_with_length(&mut self, bytes: &[u8]) {
        self.varint(bytes.len() as u64);
        self.raw(bytes);
    }

    /// Writes a value.
    pub fn put<T: Encode + ?Sized>(&mut self, value: &T) {
        value.encode(self);
    }
}

impl<'a> Reader<'a> {
    /// Reads `bytes` from the start.
    pub fn new(bytes: &'a [u8]) -> Self {
        Self {
            bytes,
            at: 0,
            version: u32::MAX,
        }
    }

    /// Reads `bytes` written in version `version` of their format.
    pub fn versioned(bytes: &'a [u8], version: u32) -> Self {
        Self {
            bytes,
            at: 0,
            version,
        }
    }

    /// The version of the format being read ([`u32::MAX`]: the latest).
    pub fn version(&self) -> u32 {
        self.version
    }

    /// The byte reading is at.
    pub fn position(&self) -> usize {
        self.at
    }

    /// The bytes not read yet.
    pub fn remaining(&self) -> usize {
        self.bytes.len() - self.at
    }

    /// An error for what was being read, here.
    pub fn error(&self, what: &'static str) -> DecodeError {
        DecodeError { at: self.at, what }
    }

    /// Checks that everything was read.
    ///
    /// # Errors
    /// If bytes are left over.
    pub fn end(&self) -> Result<(), DecodeError> {
        if self.remaining() == 0 {
            Ok(())
        } else {
            Err(self.error("bytes after the end"))
        }
    }

    /// Reads `n` raw bytes.
    ///
    /// # Errors
    /// If fewer are left.
    pub fn raw(&mut self, n: usize) -> Result<&'a [u8], DecodeError> {
        let end = self
            .at
            .checked_add(n)
            .filter(|&e| e <= self.bytes.len())
            .ok_or_else(|| self.error("bytes ended early"))?;
        let out = &self.bytes[self.at..end];
        self.at = end;
        Ok(out)
    }

    fn array<const N: usize>(&mut self) -> Result<[u8; N], DecodeError> {
        Ok(self.raw(N)?.try_into().expect("N bytes"))
    }

    /// Reads a byte.
    ///
    /// # Errors
    /// If none is left.
    pub fn u8(&mut self) -> Result<u8, DecodeError> {
        Ok(self.array::<1>()?[0])
    }

    /// Reads 16 bits.
    ///
    /// # Errors
    /// If fewer are left.
    pub fn u16(&mut self) -> Result<u16, DecodeError> {
        Ok(u16::from_le_bytes(self.array()?))
    }

    /// Reads 32 bits.
    ///
    /// # Errors
    /// If fewer are left.
    pub fn u32(&mut self) -> Result<u32, DecodeError> {
        Ok(u32::from_le_bytes(self.array()?))
    }

    /// Reads 64 bits.
    ///
    /// # Errors
    /// If fewer are left.
    pub fn u64(&mut self) -> Result<u64, DecodeError> {
        Ok(u64::from_le_bytes(self.array()?))
    }

    /// Reads a float bit for bit.
    ///
    /// # Errors
    /// If fewer than eight bytes are left.
    pub fn f64(&mut self) -> Result<f64, DecodeError> {
        Ok(f64::from_bits(self.u64()?))
    }

    /// Reads a finite float.
    ///
    /// # Errors
    /// If fewer than eight bytes are left, or they are not a finite number.
    pub fn finite(&mut self) -> Result<f64, DecodeError> {
        let v = self.f64()?;
        if v.is_finite() {
            Ok(v)
        } else {
            Err(self.error("not a finite number"))
        }
    }

    /// Reads an unsigned varint.
    ///
    /// # Errors
    /// If the bytes end within it, or it does not fit 64 bits.
    pub fn varint(&mut self) -> Result<u64, DecodeError> {
        let mut v = 0u64;
        for shift in (0..64).step_by(7) {
            let byte = self.u8()?;
            let bits = u64::from(byte & 0x7f);
            if shift == 63 && bits > 1 {
                return Err(self.error("varint too long"));
            }
            v |= bits << shift;
            if byte & 0x80 == 0 {
                return Ok(v);
            }
        }
        Err(self.error("varint too long"))
    }

    /// Reads a signed (zigzagged) varint.
    ///
    /// # Errors
    /// As [`Self::varint`].
    pub fn signed(&mut self) -> Result<i64, DecodeError> {
        let v = self.varint()?;
        #[allow(clippy::cast_possible_wrap)] // zigzag: the lowest bit is the sign
        Ok(((v >> 1) as i64) ^ -((v & 1) as i64))
    }

    /// Reads a count of items, at most what the remaining bytes could hold at `size` bytes
    /// each (a corrupt count never allocates gigabytes).
    ///
    /// # Errors
    /// If the count is more than the bytes left could hold.
    pub fn count(&mut self, size: usize) -> Result<usize, DecodeError> {
        let n = self.varint()?;
        usize::try_from(n)
            .ok()
            .filter(|&n| n.saturating_mul(size.max(1)) <= self.remaining())
            .ok_or_else(|| self.error("count larger than the bytes left"))
    }

    /// Reads a byte string written with its length.
    ///
    /// # Errors
    /// If it is longer than the bytes left.
    pub fn bytes_with_length(&mut self) -> Result<&'a [u8], DecodeError> {
        let n = self.count(1)?;
        self.raw(n)
    }

    /// Reads a value.
    ///
    /// # Errors
    /// As the value's [`Decode`].
    pub fn get<T: Decode>(&mut self) -> Result<T, DecodeError> {
        T::decode(self)
    }
}

mod impls;

#[cfg(test)]
mod tests;
