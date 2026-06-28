use std::fs;
use std::path::{Path, PathBuf};

use crate::error::SciError;
use crate::resource::decompress;
use crate::resource::{ResourceEntry, SciVersion};

/// Read and decompress a resource from a volume file.
pub fn read_resource(
    game_dir: &Path,
    entry: &ResourceEntry,
    version: SciVersion,
) -> Result<Vec<u8>, SciError> {
    let vol_path = find_volume_file(game_dir, entry.volume)?;

    let file_data = fs::read(&vol_path)?;

    let offset = entry.offset as usize;
    if offset + 9 > file_data.len() {
        return Err(SciError::InvalidResource(format!(
            "Resource offset {:#x} exceeds file size {}",
            offset,
            file_data.len()
        )));
    }

    // Parse 9-byte resource header:
    // type(1) + number(2) + packed_size(2) + unpacked_size(2) + compression(2)
    let header = &file_data[offset..offset + 9];
    let _res_type = header[0];
    let _res_number = u16::from_le_bytes([header[1], header[2]]);
    let packed_size_raw = u16::from_le_bytes([header[3], header[4]]) as usize;
    let unpacked_size = u16::from_le_bytes([header[5], header[6]]) as usize;
    let compression_method = u16::from_le_bytes([header[7], header[8]]);

    // SCI1 with methods 0 (none) and 2 (LZW1) stores packed_size + 4 in header.
    // SCI1 with method 18 (DCL) and SCI1.1 store actual packed size.
    let packed_size = match version {
        SciVersion::Sci1 if compression_method != 18 => {
            if packed_size_raw >= 4 {
                packed_size_raw - 4
            } else {
                packed_size_raw
            }
        }
        _ => packed_size_raw,
    };

    let data_start = offset + 9;
    let data_end = data_start + packed_size;

    if data_end > file_data.len() {
        return Err(SciError::InvalidResource(format!(
            "Resource data extends past end of file: {data_end} > {}",
            file_data.len()
        )));
    }

    let compressed_data = &file_data[data_start..data_end];

    decompress::decompress(compressed_data, compression_method, unpacked_size)
}

/// Find the volume file (e.g., resource.000, RESOURCE.001) with case-insensitive matching.
fn find_volume_file(game_dir: &Path, volume_number: u8) -> Result<PathBuf, SciError> {
    let target = format!("resource.{:03}", volume_number);

    // Try exact match first
    let exact = game_dir.join(&target);
    if exact.exists() {
        return Ok(exact);
    }

    // Case-insensitive search
    if let Ok(entries) = fs::read_dir(game_dir) {
        for entry in entries.flatten() {
            let name = entry.file_name();
            let name_str = name.to_string_lossy();
            if name_str.eq_ignore_ascii_case(&target) {
                return Ok(entry.path());
            }
        }
    }

    // Also check subdirectories (some games like LB2 have files in a GAME/ subdir)
    if let Ok(entries) = fs::read_dir(game_dir) {
        for entry in entries.flatten() {
            if entry.file_type().map(|t| t.is_dir()).unwrap_or(false) {
                let subdir = entry.path();
                if let Ok(sub_entries) = fs::read_dir(&subdir) {
                    for sub_entry in sub_entries.flatten() {
                        let name = sub_entry.file_name();
                        let name_str = name.to_string_lossy();
                        if name_str.eq_ignore_ascii_case(&target) {
                            return Ok(sub_entry.path());
                        }
                    }
                }
            }
        }
    }

    Err(SciError::InvalidResource(format!(
        "Volume file {target} not found in {}",
        game_dir.display()
    )))
}
