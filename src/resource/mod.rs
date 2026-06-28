pub mod decompress;
pub mod map;
pub mod volume;

/// SCI engine version for resource format differences.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SciVersion {
    Sci1,
    Sci11,
}

/// Resource type identifiers as stored in the map type directory.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum ResourceType {
    View = 0x80,
    Pic = 0x81,
    Script = 0x82,
    Text = 0x83,
    Sound = 0x84,
    Memory = 0x85,
    Vocab = 0x86,
    Font = 0x87,
    Cursor = 0x88,
    Patch = 0x89,
    Bitmap = 0x8A,
    Palette = 0x8B,
    CdAudio = 0x8C,
    Audio = 0x8D,
    Sync = 0x8E,
    Message = 0x8F,
    Map = 0x90,
    Heap = 0x91,
}

impl ResourceType {
    pub fn from_byte(b: u8) -> Option<Self> {
        match b {
            0x80 => Some(Self::View),
            0x81 => Some(Self::Pic),
            0x82 => Some(Self::Script),
            0x83 => Some(Self::Text),
            0x84 => Some(Self::Sound),
            0x85 => Some(Self::Memory),
            0x86 => Some(Self::Vocab),
            0x87 => Some(Self::Font),
            0x88 => Some(Self::Cursor),
            0x89 => Some(Self::Patch),
            0x8A => Some(Self::Bitmap),
            0x8B => Some(Self::Palette),
            0x8C => Some(Self::CdAudio),
            0x8D => Some(Self::Audio),
            0x8E => Some(Self::Sync),
            0x8F => Some(Self::Message),
            0x90 => Some(Self::Map),
            0x91 => Some(Self::Heap),
            _ => None,
        }
    }
}

/// An entry in the resource map pointing to a specific resource in a volume file.
#[derive(Debug, Clone)]
pub struct ResourceEntry {
    pub resource_type: u8,
    pub number: u16,
    pub volume: u8,
    pub offset: u32,
}
