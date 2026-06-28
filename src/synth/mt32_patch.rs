use crate::error::SciError;

/// Parsed MT-32 patch data from SCI Patch resource #1.
/// Contains custom timbres, patch mappings, rhythm key map, and partial reserve
/// that must be loaded onto the MT-32/CM-32L before playback.
#[derive(Debug)]
pub struct Mt32PatchData {
    /// Patches 1-48: 256 bytes (part 1) + 128 bytes (part 2) = 384 bytes
    pub patches_1_48: Vec<u8>,
    /// Custom timbres, each 246 bytes
    pub timbres: Vec<Vec<u8>>,
    /// Patches 49-96: 384 bytes (optional)
    pub patches_49_96: Option<Vec<u8>>,
    /// Rhythm key map: 256 bytes (optional)
    pub rhythm_key_map: Option<Vec<u8>>,
    /// Partial reserve: 9 bytes (optional)
    pub partial_reserve: Option<Vec<u8>>,
    /// MT-32 master volume (0-100)
    pub volume: u8,
}

/// Parse MT-32 patch data from a decompressed Patch resource #1.
///
/// Format (from ScummVM's readMt32Patch):
///   0-19:    after-SysEx message (display text)
///   20-39:   before-SysEx message (display text)
///   40-59:   goodbye message (display text)
///   60-61:   volume (u16 LE, clamped to 100)
///   62:      reverb default
///   63-73:   reverb SysEx message (11 bytes)
///   74-106:  reverb data (3 * 11 bytes)
///   107-490: patches 1-48 (256 + 128 = 384 bytes)
///   491:     number of timbres (max 64)
///   492+:    timbre data (246 bytes each)
///   then:    flag 0xABCD -> patches 49-96 (256 + 128 bytes)
///   then:    flag 0xDCBA -> rhythm key map (256 bytes) + partial reserve (9 bytes)
pub fn parse_mt32_patch(data: &[u8]) -> Result<Mt32PatchData, SciError> {
    if data.len() < 492 {
        return Err(SciError::InvalidResource(format!(
            "MT-32 patch resource too small: {} bytes (need at least 492)",
            data.len()
        )));
    }

    // Volume at bytes 60-61, clamped to 100
    let volume_raw = u16::from_le_bytes([data[60], data[61]]);
    let volume = volume_raw.min(100) as u8;

    // Patches 1-48 at bytes 107-490 (384 bytes)
    let patches_1_48 = data[107..491].to_vec();

    // Timbre count at byte 491
    let timbre_count = data[491] as usize;
    if timbre_count > 64 {
        return Err(SciError::InvalidResource(format!(
            "Invalid timbre count: {} (max 64)",
            timbre_count
        )));
    }

    let timbres_start = 492;
    let timbres_end = timbres_start + timbre_count * 246;
    if timbres_end > data.len() {
        return Err(SciError::InvalidResource(format!(
            "MT-32 patch resource too small for {} timbres: need {} bytes, have {}",
            timbre_count, timbres_end, data.len()
        )));
    }

    let mut timbres = Vec::with_capacity(timbre_count);
    for i in 0..timbre_count {
        let start = timbres_start + i * 246;
        timbres.push(data[start..start + 246].to_vec());
    }

    let mut pos = timbres_end;
    let mut patches_49_96 = None;
    let mut rhythm_key_map = None;
    let mut partial_reserve = None;

    // Check for extended patches flag 0xABCD
    if pos + 2 <= data.len() {
        let flag = u16::from_be_bytes([data[pos], data[pos + 1]]);
        pos += 2;

        if flag == 0xABCD && pos + 384 <= data.len() {
            patches_49_96 = Some(data[pos..pos + 384].to_vec());
            pos += 384;

            // Read next flag
            if pos + 2 <= data.len() {
                let flag2 = u16::from_be_bytes([data[pos], data[pos + 1]]);
                pos += 2;

                if flag2 == 0xDCBA {
                    if pos + 256 <= data.len() {
                        rhythm_key_map = Some(data[pos..pos + 256].to_vec());
                        pos += 256;
                    }
                    if pos + 9 <= data.len() {
                        partial_reserve = Some(data[pos..pos + 9].to_vec());
                    }
                }
            }
        } else if flag == 0xDCBA {
            // No extended patches, but rhythm map present
            if pos + 256 <= data.len() {
                rhythm_key_map = Some(data[pos..pos + 256].to_vec());
                pos += 256;
            }
            if pos + 9 <= data.len() {
                partial_reserve = Some(data[pos..pos + 9].to_vec());
            }
        }
    }

    Ok(Mt32PatchData {
        patches_1_48,
        timbres,
        patches_49_96,
        rhythm_key_map,
        partial_reserve,
        volume,
    })
}

/// Build an MT-32 SysEx message with Roland framing.
///
/// Format: F0 41 10 16 12 [addr_hi] [addr_mid] [addr_lo] [data...] [checksum] F7
///
/// Checksum = (128 - (sum of address + data bytes) % 128) % 128
fn build_mt32_sysex(address: u32, data: &[u8]) -> Vec<u8> {
    let addr_hi = ((address >> 16) & 0xFF) as u8;
    let addr_mid = ((address >> 8) & 0xFF) as u8;
    let addr_lo = (address & 0xFF) as u8;

    // Calculate checksum: subtract address + data bytes, take & 0x7F
    let mut chk: u16 = 0;
    chk = chk.wrapping_sub(addr_hi as u16);
    chk = chk.wrapping_sub(addr_mid as u16);
    chk = chk.wrapping_sub(addr_lo as u16);
    for &b in data {
        chk = chk.wrapping_sub(b as u16);
    }
    let checksum = (chk & 0x7F) as u8;

    let mut msg = Vec::with_capacity(data.len() + 9);
    msg.push(0xF0); // SysEx start
    msg.push(0x41); // Roland manufacturer ID
    msg.push(0x10); // Device ID
    msg.push(0x16); // Model ID (MT-32)
    msg.push(0x12); // Command (DT1 - data set)
    msg.push(addr_hi);
    msg.push(addr_mid);
    msg.push(addr_lo);
    msg.extend_from_slice(data);
    msg.push(checksum);
    msg.push(0xF7); // SysEx end
    msg
}

/// Build the MT-32 factory reset SysEx message.
pub fn build_reset_sysex() -> Vec<u8> {
    build_mt32_sysex(0x7F0000, &[0x01, 0x00])
}

/// Build the MT-32 volume SysEx message.
fn build_volume_sysex(volume: u8) -> Vec<u8> {
    build_mt32_sysex(0x100016, &[volume])
}

/// Build the "mystery SysEx" that ScummVM sends after patches.
fn build_mystery_sysex() -> Vec<u8> {
    build_mt32_sysex(0x52000A, &[0x16, 0x16, 0x16, 0x16, 0x16, 0x16])
}

/// Build all SysEx messages needed to load patch data onto the MT-32.
/// Returns messages in the order they should be sent.
pub fn build_patch_sysex_messages(patch_data: &Mt32PatchData) -> Vec<Vec<u8>> {
    let mut messages = Vec::new();

    // Set volume
    messages.push(build_volume_sysex(patch_data.volume));

    // Patches 1-48: 256 bytes to 0x050000, 128 bytes to 0x050200
    messages.push(build_mt32_sysex(0x050000, &patch_data.patches_1_48[..256]));
    messages.push(build_mt32_sysex(0x050200, &patch_data.patches_1_48[256..384]));

    // Custom timbres
    for (i, timbre) in patch_data.timbres.iter().enumerate() {
        let address = 0x080000 + ((i as u32) << 9);
        messages.push(build_mt32_sysex(address, timbre));
    }

    // Extended patches 49-96
    if let Some(ref patches) = patch_data.patches_49_96 {
        messages.push(build_mt32_sysex(0x050300, &patches[..256]));
        messages.push(build_mt32_sysex(0x050500, &patches[256..384]));
    }

    // Rhythm key map
    if let Some(ref rhythm) = patch_data.rhythm_key_map {
        messages.push(build_mt32_sysex(0x030110, rhythm));
    }

    // Partial reserve
    if let Some(ref reserve) = patch_data.partial_reserve {
        messages.push(build_mt32_sysex(0x100004, reserve));
    }

    // Mystery SysEx (sent by ScummVM after all patches)
    messages.push(build_mystery_sysex());

    messages
}
