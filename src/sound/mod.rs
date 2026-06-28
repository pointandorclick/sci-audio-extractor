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

/// Device type constants for SCI1/SCI1.1 track identification.
/// These follow ScummVM's getPlayId() mapping for SCI1+.
pub const DEVICE_ADLIB: u8 = 0x00;
pub const DEVICE_GM: u8 = 0x07;
pub const DEVICE_MT32: u8 = 0x0C;
pub const DEVICE_PC_SPEAKER: u8 = 0x12;
pub const DEVICE_PCJR: u8 = 0x13;

/// A timed MIDI event for synthesis.
#[derive(Debug, Clone)]
pub struct TimedMidiEvent {
    /// Tick offset from the start of the song (1 tick = 1/60th second).
    pub tick: u64,
    /// The raw MIDI message bytes (1-3 bytes for channel messages, variable for sysex).
    pub message: Vec<u8>,
}

impl SoundResource {
    /// Find the MT-32 track (type 0x0C), falling back to GM (0x07).
    pub fn mt32_track(&self) -> Option<&Track> {
        self.tracks
            .iter()
            .find(|t| t.device_type == DEVICE_MT32)
            .or_else(|| {
                self.tracks
                    .iter()
                    .find(|t| t.device_type == DEVICE_GM)
            })
    }
}
