use std::collections::HashMap;
use std::fs;
use std::path::Path;

use crate::error::SciError;
use crate::resource::{ResourceEntry, SciVersion};

/// Parsed resource map with version info and entries grouped by type.
pub struct ResourceMap {
    pub version: SciVersion,
    pub entries: Vec<ResourceEntry>,
}

impl ResourceMap {
    /// Parse a resource.map file, auto-detecting SCI version.
    pub fn parse(path: &Path) -> Result<Self, SciError> {
        let data = fs::read(path)?;

        if data.len() < 6 {
            return Err(SciError::InvalidMap("File too small".into()));
        }

        // SCI0 detection: last 4 bytes are 0xFFFFFFFF
        let len = data.len();
        if len >= 4
            && data[len - 4] == 0xFF
            && data[len - 3] == 0xFF
            && data[len - 2] == 0xFF
            && data[len - 1] == 0xFF
        {
            return Err(SciError::UnsupportedVersion(
                "SCI0 maps are not yet supported".into(),
            ));
        }

        // Parse SCI1/SCI1.1 type directory
        let (type_dir, version) = parse_type_directory(&data)?;

        // Parse resource entries for all types
        let entries = parse_resource_entries(&data, &type_dir, version)?;

        Ok(ResourceMap { version, entries })
    }

    /// Get all sound resource entries.
    pub fn sound_entries(&self) -> Vec<&ResourceEntry> {
        self.entries
            .iter()
            .filter(|e| e.resource_type == 0x84)
            .collect()
    }

    /// Get a specific patch resource entry by number.
    pub fn patch_entry(&self, number: u16) -> Option<&ResourceEntry> {
        self.entries
            .iter()
            .find(|e| e.resource_type == 0x89 && e.number == number)
    }
}

/// A type directory entry: maps a resource type byte to its lookup table offset.
#[derive(Debug)]
struct TypeDirEntry {
    type_byte: u8,
    offset: u16,
}

/// Parse the 3-byte type directory entries at the start of the map.
/// Returns the directory entries and the detected SCI version.
fn parse_type_directory(data: &[u8]) -> Result<(Vec<TypeDirEntry>, SciVersion), SciError> {
    let mut entries = Vec::new();
    let mut pos = 0;

    while pos + 2 < data.len() {
        let type_byte = data[pos];

        if type_byte == 0xFF {
            // End marker - the offset here should point to EOF
            break;
        }

        if type_byte < 0x80 {
            return Err(SciError::InvalidMap(format!(
                "Invalid type byte {:#04x} at offset {pos}",
                type_byte
            )));
        }

        let offset = u16::from_le_bytes([data[pos + 1], data[pos + 2]]);
        entries.push(TypeDirEntry { type_byte, offset });
        pos += 3;
    }

    if entries.is_empty() {
        return Err(SciError::InvalidMap("No type directory entries".into()));
    }

    // Detect SCI1 vs SCI1.1 by checking entry size divisibility.
    // The size of a lookup table section = difference between consecutive offsets.
    let version = detect_version(&entries, data.len())?;

    Ok((entries, version))
}

/// Detect whether this is SCI1 (6-byte entries) or SCI1.1 (5-byte entries)
/// by checking the divisibility of directory section sizes.
fn detect_version(dir: &[TypeDirEntry], file_size: usize) -> Result<SciVersion, SciError> {
    // Build a sorted list of all offsets including the end-of-file
    let mut offsets: Vec<u16> = dir.iter().map(|e| e.offset).collect();
    offsets.push(file_size as u16);
    offsets.sort();
    offsets.dedup();

    for i in 0..offsets.len() - 1 {
        let size = (offsets[i + 1] - offsets[i]) as usize;
        if size == 0 {
            continue;
        }
        if size % 5 == 0 && size % 6 != 0 {
            return Ok(SciVersion::Sci11);
        }
        if size % 6 == 0 && size % 5 != 0 {
            return Ok(SciVersion::Sci1);
        }
    }

    // If ambiguous (divisible by both), default to SCI1
    // This can happen with very small sections
    Ok(SciVersion::Sci1)
}

/// Parse resource entries from the lookup tables for all types.
fn parse_resource_entries(
    data: &[u8],
    type_dir: &[TypeDirEntry],
    version: SciVersion,
) -> Result<Vec<ResourceEntry>, SciError> {
    // Build a map of type -> (start_offset, end_offset)
    let mut sorted_offsets: Vec<u16> = type_dir.iter().map(|e| e.offset).collect();
    sorted_offsets.sort();
    sorted_offsets.dedup();

    // Map each type's offset to the next offset (end boundary)
    let mut offset_to_end: HashMap<u16, u16> = HashMap::new();
    for i in 0..sorted_offsets.len() {
        let end = if i + 1 < sorted_offsets.len() {
            sorted_offsets[i + 1]
        } else {
            data.len() as u16
        };
        offset_to_end.insert(sorted_offsets[i], end);
    }

    let mut entries = Vec::new();
    let entry_size: usize = match version {
        SciVersion::Sci1 => 6,
        SciVersion::Sci11 => 5,
    };

    for dir_entry in type_dir {
        let start = dir_entry.offset as usize;
        let end = *offset_to_end.get(&dir_entry.offset).unwrap_or(&0) as usize;

        let mut pos = start;
        while pos + entry_size <= end {
            let number = u16::from_le_bytes([data[pos], data[pos + 1]]);

            let (volume, offset) = match version {
                SciVersion::Sci1 => {
                    let packed = u32::from_le_bytes([
                        data[pos + 2],
                        data[pos + 3],
                        data[pos + 4],
                        data[pos + 5],
                    ]);
                    let vol = (packed >> 28) as u8;
                    let off = packed & 0x0FFF_FFFF;
                    (vol, off)
                }
                SciVersion::Sci11 => {
                    let raw =
                        data[pos + 2] as u32 | ((data[pos + 3] as u32) << 8) | ((data[pos + 4] as u32) << 16);
                    let off = raw << 1;
                    (0u8, off)
                }
            };

            entries.push(ResourceEntry {
                resource_type: dir_entry.type_byte,
                number,
                volume,
                offset,
            });

            pos += entry_size;
        }
    }

    Ok(entries)
}
