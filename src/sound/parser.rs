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
        // ScummVM: first byte is _soundPriority, then skip 6 bytes (one channel entry), then 0xFF
        if track_type == 0xF0 {
            // Skip channel entries until 0xFF terminator
            while pos < data.len() && data[pos] != 0xFF {
                pos += 6;
            }
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
/// Follows ScummVM's midiMixChannels/parseNextEvent logic.
fn parse_channel_midi(data: &[u8], midi_channel: u8) -> Result<Vec<TimedMidiEvent>, SciError> {
    let mut events = Vec::new();
    let mut pos = 0;
    let mut tick: u64 = 0;
    let mut running_status: Option<u8> = None;

    // Channel 15 is SCI's control channel (loop markers, cues, reverb, etc.)
    // These events should not be sent to the synthesizer.
    let is_control_channel = midi_channel == 0x0F;

    while pos < data.len() {
        // Read delta time
        // In SCI, 0xF8 = 240-tick delay marker (no event follows).
        // Other values < 0x80 are delta tick values preceding an event.
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
        // ScummVM: if (midiCommand & 0x80) { read param } else { param = byte; cmd = prev }
        let (status, first_param) = if data[pos] >= 0x80 {
            let s = data[pos];
            pos += 1;
            running_status = Some(s);
            // First data byte follows
            if pos >= data.len() {
                break;
            }
            let p = data[pos];
            pos += 1;
            (s, p)
        } else if let Some(s) = running_status {
            // Running status: the byte IS the first data byte
            let p = data[pos];
            pos += 1;
            (s, p)
        } else {
            // No running status and no status byte - skip
            pos += 1;
            continue;
        };

        // Parse based on status type
        let msg_type = status & 0xF0;

        match msg_type {
            0x80 | 0x90 | 0xA0 | 0xB0 | 0xE0 => {
                // Two data bytes: note off, note on, aftertouch, control change, pitch bend
                if pos >= data.len() {
                    break;
                }
                let d2 = data[pos];
                pos += 1;

                // Skip all channel 15 events - they are SCI control messages
                if is_control_channel {
                    continue;
                }

                // Use the MIDI channel from the channel header, not from the status byte
                let actual_status = msg_type | midi_channel;
                events.push(TimedMidiEvent {
                    tick,
                    message: vec![actual_status, first_param, d2],
                });
            }
            0xC0 | 0xD0 => {
                // One data byte: program change, channel pressure
                // first_param is the only data byte (already read above)

                // Skip channel 15 control events
                if is_control_channel {
                    continue;
                }

                let actual_status = msg_type | midi_channel;
                events.push(TimedMidiEvent {
                    tick,
                    message: vec![actual_status, first_param],
                });
            }
            0xF0 => {
                if status == 0xF0 {
                    // SysEx message - first_param is the first SysEx data byte
                    let mut sysex = vec![0xF0, first_param];
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
                    // Meta event - first_param is the meta type
                    let meta_type = first_param;

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
            }
        }
    }

    Ok(events)
}
