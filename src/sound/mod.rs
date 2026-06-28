pub mod parser;

/// A parsed SCI sound resource containing tracks for different audio devices.
#[derive(Debug)]
pub struct SoundResource {
    pub tracks: Vec<Track>,
}

/// A track within a sound resource, targeting a specific audio device.
#[derive(Debug)]
pub struct Track {
    pub device_type: u8,
    pub channels: Vec<Channel>,
    pub digital_channel: Option<usize>,
}

/// A channel within a track containing MIDI event data.
#[derive(Debug)]
pub struct Channel {
    pub midi_channel: u8,
    pub flags: u8,
    pub polyphony: u8,
    pub priority: u8,
    pub data: Vec<u8>,
}

/// Device type constants for track identification.
pub const DEVICE_MT32: u8 = 0x00;
pub const DEVICE_MT32_V2: u8 = 0x08;
pub const DEVICE_ADLIB: u8 = 0x06;
pub const DEVICE_GM: u8 = 0x07;
pub const DEVICE_PC_SPEAKER: u8 = 0x0C;

/// A timed MIDI event for synthesis.
#[derive(Debug, Clone)]
pub struct TimedMidiEvent {
    /// Tick offset from the start of the song (1 tick = 1/60th second).
    pub tick: u64,
    /// The raw MIDI message bytes (1-3 bytes for channel messages, variable for sysex).
    pub message: Vec<u8>,
}

impl SoundResource {
    /// Find the MT-32 track (type 0x00), falling back to type 0x08.
    pub fn mt32_track(&self) -> Option<&Track> {
        self.tracks
            .iter()
            .find(|t| t.device_type == DEVICE_MT32)
            .or_else(|| {
                self.tracks
                    .iter()
                    .find(|t| t.device_type == DEVICE_MT32_V2)
            })
    }
}
