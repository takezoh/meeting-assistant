//! Minimal 16-bit PCM WAV framing: enough to write chunks, read them back and repair a `.part`.

pub const HEADER_LEN: usize = 44;

pub fn header(sample_rate: u32, channels: u16, data_len: u32) -> [u8; HEADER_LEN] {
    let mut h = [0u8; HEADER_LEN];
    h[0..4].copy_from_slice(b"RIFF");
    h[4..8].copy_from_slice(&(36 + data_len).to_le_bytes());
    h[8..12].copy_from_slice(b"WAVE");
    h[12..16].copy_from_slice(b"fmt ");
    h[16..20].copy_from_slice(&16u32.to_le_bytes());
    h[20..22].copy_from_slice(&1u16.to_le_bytes());
    h[22..24].copy_from_slice(&channels.to_le_bytes());
    h[24..28].copy_from_slice(&sample_rate.to_le_bytes());
    let block_align = channels * 2;
    h[28..32].copy_from_slice(&(sample_rate * block_align as u32).to_le_bytes());
    h[32..34].copy_from_slice(&block_align.to_le_bytes());
    h[34..36].copy_from_slice(&16u16.to_le_bytes());
    h[36..40].copy_from_slice(b"data");
    h[40..44].copy_from_slice(&data_len.to_le_bytes());
    h
}

pub fn encode(sample_rate: u32, channels: u16, samples: &[i16]) -> Vec<u8> {
    let mut out = Vec::with_capacity(HEADER_LEN + samples.len() * 2);
    out.extend_from_slice(&header(sample_rate, channels, (samples.len() * 2) as u32));
    for s in samples {
        out.extend_from_slice(&s.to_le_bytes());
    }
    out
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Decoded {
    pub sample_rate: u32,
    pub channels: u16,
    pub samples: Vec<i16>,
}

/// Decode a complete WAV file; a header whose data length disagrees with the file is an error.
pub fn decode(bytes: &[u8]) -> Result<Decoded, String> {
    if bytes.len() < HEADER_LEN
        || &bytes[0..4] != b"RIFF"
        || &bytes[8..12] != b"WAVE"
        || &bytes[36..40] != b"data"
    {
        return Err("not a wav".into());
    }
    let channels = u16::from_le_bytes([bytes[22], bytes[23]]);
    let sample_rate = u32::from_le_bytes([bytes[24], bytes[25], bytes[26], bytes[27]]);
    let data_len = u32::from_le_bytes([bytes[40], bytes[41], bytes[42], bytes[43]]) as usize;
    if bytes.len() != HEADER_LEN + data_len {
        return Err(format!(
            "data length {data_len} disagrees with file length {}",
            bytes.len()
        ));
    }
    let samples = bytes[HEADER_LEN..]
        .as_chunks::<2>()
        .0
        .iter()
        .map(|c| i16::from_le_bytes(*c))
        .collect();
    Ok(Decoded {
        sample_rate,
        channels,
        samples,
    })
}

/// Repair a partial file: keep every complete frame and rewrite the header to the real length.
/// Returns `None` when not even one complete frame is present.
pub fn repair_part(bytes: &[u8]) -> Option<Vec<u8>> {
    if bytes.len() < HEADER_LEN + 2 || &bytes[0..4] != b"RIFF" {
        return None;
    }
    let channels = u16::from_le_bytes([bytes[22], bytes[23]]).max(1);
    let sample_rate = u32::from_le_bytes([bytes[24], bytes[25], bytes[26], bytes[27]]);
    let frame = channels as usize * 2;
    let complete = (bytes.len() - HEADER_LEN) / frame * frame;
    if complete == 0 {
        return None;
    }
    let mut out = Vec::with_capacity(HEADER_LEN + complete);
    out.extend_from_slice(&header(sample_rate, channels, complete as u32));
    out.extend_from_slice(&bytes[HEADER_LEN..HEADER_LEN + complete]);
    Some(out)
}
