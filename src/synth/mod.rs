use std::fs;
use std::path::Path;

use moont::cm32l;
use moont::Synth;

use crate::error::SciError;
use crate::sound::TimedMidiEvent;

/// Sample rate of the CM-32L emulator output.
pub const SAMPLE_RATE: u32 = 32000;

/// Ticks per second in SCI's timing system (60 Hz).
const TICKS_PER_SECOND: f64 = 60.0;

/// Samples per tick at 32kHz output.
const SAMPLES_PER_TICK: f64 = SAMPLE_RATE as f64 / TICKS_PER_SECOND;

/// Extra samples to render after the last event for reverb/decay tail.
const TAIL_SECONDS: f64 = 2.0;
const TAIL_SAMPLES: u64 = (SAMPLE_RATE as f64 * TAIL_SECONDS) as u64;

/// Load CM-32L ROM data from a directory, returning raw bytes for control and PCM ROMs.
pub fn load_rom_data(rom_dir: &Path) -> Result<(Vec<u8>, Vec<u8>), SciError> {
    let control_path = find_rom_file(rom_dir, &["CM32L_CONTROL.ROM", "cm32l_ctrl"])?;
    let pcm_path = find_rom_file(rom_dir, &["CM32L_PCM.ROM", "cm32l_pcm.rom"])?;

    let control_data = fs::read(&control_path).map_err(|e| {
        SciError::RomError(format!("Failed to read control ROM {}: {e}", control_path.display()))
    })?;
    let pcm_data = fs::read(&pcm_path).map_err(|e| {
        SciError::RomError(format!("Failed to read PCM ROM {}: {e}", pcm_path.display()))
    })?;

    // Validate that the ROMs can be parsed
    cm32l::Rom::new(&control_data, &pcm_data)
        .map_err(|e| SciError::RomError(format!("Invalid ROM data: {e:?}")))?;

    Ok((control_data, pcm_data))
}

/// Find a ROM file by searching for known filenames (case-insensitive).
fn find_rom_file(dir: &Path, patterns: &[&str]) -> Result<std::path::PathBuf, SciError> {
    if let Ok(entries) = fs::read_dir(dir) {
        for entry in entries.flatten() {
            let name = entry.file_name();
            let name_str = name.to_string_lossy();
            let name_lower = name_str.to_lowercase();

            for pattern in patterns {
                let pattern_lower = pattern.to_lowercase();
                if name_lower == pattern_lower || name_lower.starts_with(&pattern_lower) {
                    return Ok(entry.path());
                }
            }
        }
    }

    Err(SciError::RomError(format!(
        "ROM file matching {:?} not found in {}",
        patterns, dir.display()
    )))
}

/// Render MIDI events using raw ROM data bytes.
pub fn render_to_pcm_with_rom_data(
    control_rom: &[u8],
    pcm_rom: &[u8],
    events: &[TimedMidiEvent],
) -> Result<Vec<i16>, SciError> {
    let rom = cm32l::Rom::new(control_rom, pcm_rom)
        .map_err(|e| SciError::RomError(format!("Invalid ROM data: {e:?}")))?;
    let mut device = cm32l::Device::new(rom);

    if events.is_empty() {
        return Ok(Vec::new());
    }

    // Calculate total duration in samples
    let last_tick = events.last().map(|e| e.tick).unwrap_or(0);
    let total_samples = (last_tick as f64 * SAMPLES_PER_TICK) as u64 + TAIL_SAMPLES;

    // Pre-allocate output buffer (stereo interleaved)
    let mut pcm: Vec<i16> = Vec::with_capacity(total_samples as usize * 2);

    let mut current_sample: u64 = 0;
    let mut event_idx = 0;

    // Render block size (number of stereo frames per render call)
    const BLOCK_SIZE: usize = 256;
    let mut frame_buf = vec![moont::Frame::default(); BLOCK_SIZE];

    while current_sample < total_samples {
        // Send any MIDI events that should fire at or before the current sample
        while event_idx < events.len() {
            let event_sample = (events[event_idx].tick as f64 * SAMPLES_PER_TICK) as u64;

            if event_sample > current_sample {
                // Render up to the next event
                let samples_to_render = (event_sample - current_sample).min(BLOCK_SIZE as u64);
                let frames = &mut frame_buf[..samples_to_render as usize];
                device.render(frames);

                for frame in frames.iter() {
                    pcm.push(frame.0);
                    pcm.push(frame.1);
                }

                current_sample += samples_to_render;
                continue;
            }

            // Send the MIDI event
            send_midi_event(&mut device, &events[event_idx]);
            event_idx += 1;
        }

        // Render remaining samples (including tail)
        let remaining = total_samples - current_sample;
        let to_render = remaining.min(BLOCK_SIZE as u64) as usize;

        if to_render == 0 {
            break;
        }

        let frames = &mut frame_buf[..to_render];
        device.render(frames);

        for frame in frames.iter() {
            pcm.push(frame.0);
            pcm.push(frame.1);
        }

        current_sample += to_render as u64;
    }

    Ok(pcm)
}

/// Send a MIDI event to the CM-32L device.
fn send_midi_event(device: &mut cm32l::Device, event: &TimedMidiEvent) {
    let msg = &event.message;

    if msg.is_empty() {
        return;
    }

    if msg[0] == 0xF0 {
        // SysEx message - send via play_msg with special encoding
        // moont expects sysex via the sysex interface if available,
        // or as packed u32 messages for short messages.
        // For now, skip sysex (most SCI1+ games don't rely on it for basic playback)
        return;
    }

    // Pack short MIDI message into u32 for play_msg
    let packed: u32 = match msg.len() {
        1 => msg[0] as u32,
        2 => (msg[0] as u32) | ((msg[1] as u32) << 8),
        3 => (msg[0] as u32) | ((msg[1] as u32) << 8) | ((msg[2] as u32) << 16),
        _ => return,
    };

    device.play_msg(packed);
}
