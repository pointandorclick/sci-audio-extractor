use crate::error::SciError;
use crate::sound::{Channel, SoundResource, TimedMidiEvent, Track};

/// Parse a decompressed SCI1/SCI1.1 sound resource body into tracks and channels.
pub fn parse_sound_resource(data: &[u8]) -> Result<SoundResource, SciError> {
    let mut tracks = Vec::new();
    let mut pos = 0;

    while pos < data.len() {
        let track_type = data[pos];
        pos += 1;

        if track_type == 0xFF {
            break;
        }

        // Digital track marker (0xF0) - skip it
        if track_type == 0xF0 {
            // First byte of channel list is priority, then 6 bytes of channel data, then 0xFF
            pos += 6;
            if pos < data.len() && data[pos] == 0xFF {
                pos += 1;
            }
            continue;
        }

        // Read channel entries (6 bytes each) until 0xFF
        let mut channels = Vec::new();
        let mut digital_channel = None;

        while pos + 5 < data.len() && data[pos] != 0xFF {
            let _unknown = u16::from_le_bytes([data[pos], data[pos + 1]]);
            let channel_offset = u16::from_le_bytes([data[pos + 2], data[pos + 3]]) as usize;
            let channel_size = u16::from_le_bytes([data[pos + 4], data[pos + 5]]) as usize;
            pos += 6;

            if channel_offset >= data.len() || channel_size == 0 {
                continue;
            }

            let actual_size = channel_size.min(data.len() - channel_offset);
            if actual_size < 2 {
                continue;
            }

            let ch_number_byte = data[channel_offset];
            let ch_info_byte = data[channel_offset + 1];

            if ch_number_byte == 0xFE {
                // Digital audio channel
                digital_channel = Some(channels.len());
                channels.push(Channel {
                    midi_channel: 0xFE,
                    flags: 0,
                    polyphony: 0,
                    priority: 0,
                    data: data[channel_offset + 2..channel_offset + actual_size].to_vec(),
                });
            } else {
                let midi_channel = ch_number_byte & 0x0F;
                let flags = ch_number_byte >> 4;
                let polyphony = ch_info_byte & 0x0F;
                let priority = ch_info_byte >> 4;

                channels.push(Channel {
                    midi_channel,
                    flags,
                    polyphony,
                    priority,
                    data: data[channel_offset + 2..channel_offset + actual_size].to_vec(),
                });
            }
        }

        // Skip the 0xFF terminator
        if pos < data.len() && data[pos] == 0xFF {
            pos += 1;
        }

        tracks.push(Track {
            device_type: track_type,
            channels,
            digital_channel,
        });
    }

    Ok(SoundResource { tracks })
}

/// Extract timed MIDI events from an MT-32 track by merging all channels chronologically.
pub fn extract_midi_events(track: &Track) -> Result<Vec<TimedMidiEvent>, SciError> {
    // Parse each channel into its own event list
    let mut all_events: Vec<TimedMidiEvent> = Vec::new();

    for channel in &track.channels {
        if channel.midi_channel == 0xFE {
            continue; // Skip digital channels
        }

        let events = parse_channel_midi(&channel.data, channel.midi_channel)?;
        all_events.extend(events);
    }

    // Sort by tick time for chronological playback
    all_events.sort_by_key(|e| e.tick);

    Ok(all_events)
}

/// Parse MIDI events from a single channel's data.
/// Uses Sierra's modified MIDI format with 0xF8 delay markers.
fn parse_channel_midi(data: &[u8], midi_channel: u8) -> Result<Vec<TimedMidiEvent>, SciError> {
    let mut events = Vec::new();
    let mut pos = 0;
    let mut tick: u64 = 0;
    let mut running_status: Option<u8> = None;

    while pos < data.len() {
        // Read delta time
        // In SCI, delta time bytes precede each event.
        // 0xF8 = 240-tick delay marker (can appear multiple times).
        // Other values < 0x80 are small delta values.
        loop {
            if pos >= data.len() {
                return Ok(events);
            }

            if data[pos] == 0xF8 {
                tick += 240;
                pos += 1;
            } else {
                break;
            }
        }

        if pos >= data.len() {
            break;
        }

        // Read the delta byte (non-0xF8)
        let delta = data[pos] as u64;
        if delta < 0x80 {
            tick += delta;
            pos += 1;
        }
        // If >= 0x80, it's a status byte with no additional delta

        if pos >= data.len() {
            break;
        }

        // Read status byte or use running status
        let status = if data[pos] >= 0x80 {
            let s = data[pos];
            pos += 1;
            running_status = Some(s);
            s
        } else if let Some(s) = running_status {
            s
        } else {
            // No running status and no status byte - skip
            pos += 1;
            continue;
        };

        // Parse based on status type
        let msg_type = status & 0xF0;
        let channel_in_status = status & 0x0F;

        // For channel messages, replace the channel with the actual MIDI channel from the header
        let actual_status = msg_type | midi_channel;

        match msg_type {
            0x80 | 0x90 | 0xA0 | 0xB0 | 0xE0 => {
                // Two data bytes: note off, note on, aftertouch, control change, pitch bend
                if pos + 1 >= data.len() {
                    break;
                }
                let d1 = data[pos];
                let d2 = data[pos + 1];
                pos += 2;

                // Channel 15 special commands (control channel)
                if channel_in_status == 0x0F {
                    if msg_type == 0xB0 && d1 == 0x7F {
                        // Loop point marker - ignore for single playthrough
                        continue;
                    }
                    // Other channel 15 events: end marker etc.
                    // 0xFC on channel 15 means end of track
                }

                events.push(TimedMidiEvent {
                    tick,
                    message: vec![actual_status, d1, d2],
                });
            }
            0xC0 | 0xD0 => {
                // One data byte: program change, channel pressure
                if pos >= data.len() {
                    break;
                }
                let d1 = data[pos];
                pos += 1;

                events.push(TimedMidiEvent {
                    tick,
                    message: vec![actual_status, d1],
                });
            }
            0xF0 => {
                if status == 0xF0 {
                    // SysEx message - read until 0xF7
                    let mut sysex = vec![0xF0];
                    while pos < data.len() && data[pos] != 0xF7 {
                        sysex.push(data[pos]);
                        pos += 1;
                    }
                    if pos < data.len() {
                        sysex.push(0xF7);
                        pos += 1;
                    }
                    events.push(TimedMidiEvent {
                        tick,
                        message: sysex,
                    });
                    running_status = None;
                } else if status == 0xFC {
                    // End of track
                    break;
                } else if status == 0xFF {
                    // Meta event - read type and length
                    if pos + 1 >= data.len() {
                        break;
                    }
                    let meta_type = data[pos];
                    pos += 1;

                    if meta_type == 0x2F {
                        // End of track
                        break;
                    }

                    // Read variable-length size
                    let mut length: usize = 0;
                    while pos < data.len() {
                        let b = data[pos];
                        pos += 1;
                        length = (length << 7) | (b & 0x7F) as usize;
                        if b & 0x80 == 0 {
                            break;
                        }
                    }
                    // Skip meta event data
                    pos += length.min(data.len() - pos);
                    running_status = None;
                } else {
                    // Other system messages (0xF1-0xFE) - skip
                    running_status = None;
                }
            }
            _ => {
                // Unknown - skip
                pos += 1;
            }
        }
    }

    Ok(events)
}
