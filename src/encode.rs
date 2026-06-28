use std::fs::File;
use std::io::Write;
use std::num::NonZero;
use std::path::Path;

use crate::error::SciError;
use crate::synth::SAMPLE_RATE;

/// Encode interleaved stereo i16 PCM to OGG Vorbis and write to file.
pub fn encode_ogg(pcm: &[i16], output_path: &Path, _quality: f32) -> Result<(), SciError> {
    use vorbis_rs::VorbisEncoderBuilder;

    if pcm.is_empty() {
        return Err(SciError::EncodingError("No PCM data to encode".into()));
    }

    let mut file = File::create(output_path).map_err(|e| {
        SciError::EncodingError(format!("Failed to create {}: {e}", output_path.display()))
    })?;

    let mut encoder = VorbisEncoderBuilder::new(
        NonZero::new(SAMPLE_RATE).unwrap(),
        NonZero::new(2u8).unwrap(),
        &mut file,
    )
    .map_err(|e| SciError::EncodingError(format!("Failed to create encoder: {e}")))?
    .build()
    .map_err(|e| SciError::EncodingError(format!("Failed to build encoder: {e}")))?;

    // Convert interleaved i16 to per-channel f32 samples
    let num_samples = pcm.len() / 2;
    let mut left = Vec::with_capacity(num_samples);
    let mut right = Vec::with_capacity(num_samples);

    for chunk in pcm.chunks_exact(2) {
        left.push(chunk[0] as f32 / 32768.0);
        right.push(chunk[1] as f32 / 32768.0);
    }

    // Encode in blocks
    const BLOCK_SIZE: usize = 4096;
    let mut offset = 0;

    while offset < num_samples {
        let end = (offset + BLOCK_SIZE).min(num_samples);
        let channels: [&[f32]; 2] = [&left[offset..end], &right[offset..end]];

        encoder
            .encode_audio_block(channels)
            .map_err(|e| SciError::EncodingError(format!("Encoding failed: {e}")))?;

        offset = end;
    }

    encoder
        .finish()
        .map_err(|e| SciError::EncodingError(format!("Failed to finalize: {e}")))?;

    Ok(())
}

/// Write interleaved stereo i16 PCM as a WAV file.
pub fn write_wav(pcm: &[i16], output_path: &Path) -> Result<(), SciError> {
    let mut file = File::create(output_path).map_err(|e| {
        SciError::EncodingError(format!("Failed to create {}: {e}", output_path.display()))
    })?;

    let num_channels: u16 = 2;
    let sample_rate = SAMPLE_RATE;
    let bits_per_sample: u16 = 16;
    let byte_rate = sample_rate * num_channels as u32 * bits_per_sample as u32 / 8;
    let block_align = num_channels * bits_per_sample / 8;
    let data_size = pcm.len() as u32 * 2; // 2 bytes per i16
    let file_size = 36 + data_size;

    // RIFF header
    file.write_all(b"RIFF")?;
    file.write_all(&file_size.to_le_bytes())?;
    file.write_all(b"WAVE")?;

    // fmt chunk
    file.write_all(b"fmt ")?;
    file.write_all(&16u32.to_le_bytes())?; // chunk size
    file.write_all(&1u16.to_le_bytes())?; // PCM format
    file.write_all(&num_channels.to_le_bytes())?;
    file.write_all(&sample_rate.to_le_bytes())?;
    file.write_all(&byte_rate.to_le_bytes())?;
    file.write_all(&block_align.to_le_bytes())?;
    file.write_all(&bits_per_sample.to_le_bytes())?;

    // data chunk
    file.write_all(b"data")?;
    file.write_all(&data_size.to_le_bytes())?;

    for sample in pcm {
        file.write_all(&sample.to_le_bytes())?;
    }

    Ok(())
}
