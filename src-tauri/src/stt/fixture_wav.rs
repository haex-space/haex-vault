//! Reads the checked-in 16 kHz mono WAV fixtures for tests (spec 008 STT, spec 066 pause
//! detection) and the platform probe (spec 066, T005).

#[cfg(test)]
use std::io::Read;
#[cfg(test)]
use std::path::Path;

/// Minimal RIFF/WAVE chunk walker for the checked-in fixtures only — reads 16-bit PCM straight
/// into normalized `f32`. Not a general decoder: the fixtures are already 16 kHz mono
/// (`stt::SAMPLE_RATE_HZ`), so this deliberately does not exercise resampling/downmixing (that is
/// `audio::`'s job, tested there against synthetic buffers instead, T006).
#[cfg(test)]
pub(crate) fn read_fixture_wav(path: &Path) -> Vec<f32> {
    let mut bytes = Vec::new();
    std::fs::File::open(path)
        .unwrap()
        .read_to_end(&mut bytes)
        .unwrap();
    decode_fixture_wav(&bytes)
}

/// [`read_fixture_wav`] for bytes already in memory (the platform probe builds its sample in).
pub(crate) fn decode_fixture_wav(bytes: &[u8]) -> Vec<f32> {
    assert_eq!(&bytes[0..4], b"RIFF");
    assert_eq!(&bytes[8..12], b"WAVE");

    let mut pos = 12;
    let mut data: Option<&[u8]> = None;
    while pos + 8 <= bytes.len() {
        let chunk_id = &bytes[pos..pos + 4];
        let chunk_size = u32::from_le_bytes(bytes[pos + 4..pos + 8].try_into().unwrap()) as usize;
        let body_start = pos + 8;
        if chunk_id == b"data" {
            data = Some(&bytes[body_start..body_start + chunk_size]);
            break;
        }
        // Chunks are padded to an even number of bytes.
        pos = body_start + chunk_size + (chunk_size % 2);
    }
    let data = data.expect("fixture wav has no data chunk");
    data.as_chunks::<2>()
        .0
        .iter()
        .map(|b| i16::from_le_bytes(*b) as f32 / i16::MAX as f32)
        .collect()
}
