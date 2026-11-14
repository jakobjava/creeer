//! Writing sound to a WAV file (16-bit PCM, stereo): what the offline renderer writes for the
//! user to listen to.

use std::io::Write;
use std::path::Path;

/// Writes `frames` (each within ±1) at `rate` samples a second to `path`.
///
/// # Errors
/// If the file cannot be written.
pub fn write(path: &Path, frames: &[[f32; 2]], rate: u32) -> std::io::Result<()> {
    let data =
        u32::try_from(frames.len() * 4).map_err(|_| std::io::Error::other("too long for a WAV"))?;
    let mut bytes = Vec::with_capacity(44 + frames.len() * 4);
    bytes.extend_from_slice(b"RIFF");
    bytes.extend_from_slice(&(36 + data).to_le_bytes());
    bytes.extend_from_slice(b"WAVEfmt ");
    bytes.extend_from_slice(&16u32.to_le_bytes());
    bytes.extend_from_slice(&1u16.to_le_bytes()); // PCM
    bytes.extend_from_slice(&2u16.to_le_bytes()); // stereo
    bytes.extend_from_slice(&rate.to_le_bytes());
    bytes.extend_from_slice(&(rate * 4).to_le_bytes());
    bytes.extend_from_slice(&4u16.to_le_bytes());
    bytes.extend_from_slice(&16u16.to_le_bytes());
    bytes.extend_from_slice(b"data");
    bytes.extend_from_slice(&data.to_le_bytes());
    for frame in frames {
        for s in frame {
            #[allow(clippy::cast_possible_truncation)] // within ±32767 by the clamp
            let v = (s.clamp(-1.0, 1.0) * 32767.0).round() as i16;
            bytes.extend_from_slice(&v.to_le_bytes());
        }
    }
    std::fs::File::create(path)?.write_all(&bytes)
}

#[cfg(test)]
mod tests {
    #[test]
    fn a_wav_has_its_header_and_its_samples() {
        let dir = std::env::temp_dir().join(format!("creeer_wav_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("t.wav");
        super::write(&path, &[[0.5, -0.5]; 10], 48_000).unwrap();
        let bytes = std::fs::read(&path).unwrap();
        assert_eq!(bytes.len(), 44 + 40);
        assert_eq!(&bytes[0..4], b"RIFF");
        assert_eq!(i16::from_le_bytes([bytes[44], bytes[45]]), 16384);
        std::fs::remove_dir_all(&dir).ok();
    }
}
