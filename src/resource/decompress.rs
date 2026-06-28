use crate::error::SciError;

/// Decompress data using the specified SCI compression method.
pub fn decompress(
    data: &[u8],
    method: u16,
    unpacked_size: usize,
) -> Result<Vec<u8>, SciError> {
    match method {
        0 => Ok(data.to_vec()),
        18 | 19 | 20 => decompress_dcl(data, unpacked_size),
        2 => decompress_lzw1_v2(data, unpacked_size),
        1 => Err(SciError::DecompressionError(
            "Huffman compression (method 1) not yet implemented".into(),
        )),
        _ => Err(SciError::DecompressionError(format!(
            "Unknown compression method {method}"
        ))),
    }
}

/// DCL (PKWARE Data Compression Library) decompression using the `explode` crate.
fn decompress_dcl(data: &[u8], _unpacked_size: usize) -> Result<Vec<u8>, SciError> {
    explode::explode(data).map_err(|e| {
        SciError::DecompressionError(format!("DCL decompression failed: {e}"))
    })
}

/// Sierra's custom LZW1 decompression (SCI01/SCI1).
/// Ported from ScummVM's DecompressorLZW (engines/sci/resource/decompressor.cpp).
///
/// Key differences from standard LZW:
/// - MSB-first bit reading
/// - "Early change" bug: bit width increases when next_code reaches (1 << bits) - 1
/// - Code 256 = RESET, Code 257 = END
fn decompress_lzw1_v2(data: &[u8], unpacked_size: usize) -> Result<Vec<u8>, SciError> {
    let mut output = Vec::with_capacity(unpacked_size);
    let mut reader = BitReaderMsb::new(data);

    const TOKEN_RESET: u16 = 256;
    const TOKEN_END: u16 = 257;
    const FIRST_CODE: u16 = 258;
    const MAX_TABLE: usize = 4096;
    const MAX_BITS: u32 = 12;

    let mut num_bits: u32 = 9;
    let mut next_code: u16 = FIRST_CODE;
    // "Early change": bit width increases when next_code reaches (1 << num_bits) - 1
    let mut code_limit: u16 = (1u16 << num_bits) - 1;

    // Dictionary: prefix chain
    let mut dict_prefix = vec![0u16; MAX_TABLE];
    let mut dict_append = vec![0u8; MAX_TABLE];

    let mut old_code: Option<u16> = None;
    let mut decode_stack = Vec::with_capacity(MAX_TABLE);

    loop {
        if output.len() >= unpacked_size {
            break;
        }

        let code = reader.read_bits(num_bits)? as u16;

        if code == TOKEN_END {
            break;
        }

        if code == TOKEN_RESET {
            num_bits = 9;
            next_code = FIRST_CODE;
            code_limit = (1u16 << num_bits) - 1;
            old_code = None;
            continue;
        }

        let first_byte: u8;

        if code < 256 {
            // Literal
            first_byte = code as u8;
            output.push(first_byte);
        } else if code < next_code {
            // Known dictionary entry - decode via chain
            first_byte = decode_chain(&dict_prefix, &dict_append, code, &mut output, &mut decode_stack);
        } else if code == next_code {
            // cScSc special case
            let old = old_code.ok_or_else(|| {
                SciError::DecompressionError("LZW1: cScSc with no previous code".into())
            })?;
            first_byte = decode_chain(&dict_prefix, &dict_append, old, &mut output, &mut decode_stack);
            output.push(first_byte);
        } else {
            return Err(SciError::DecompressionError(format!(
                "LZW1: code {code} > next_code {next_code}"
            )));
        }

        // Add new dictionary entry
        if let Some(old) = old_code {
            if (next_code as usize) < MAX_TABLE {
                dict_prefix[next_code as usize] = old;
                dict_append[next_code as usize] = first_byte;
                next_code += 1;

                // "Early change" - increase bit width when next_code reaches limit
                if next_code >= code_limit && num_bits < MAX_BITS {
                    num_bits += 1;
                    code_limit = (1u16 << num_bits) - 1;
                }
            }
        }

        old_code = Some(code);
    }

    output.truncate(unpacked_size);
    Ok(output)
}

/// Decode an LZW dictionary chain, outputting the string and returning its first byte.
fn decode_chain(
    prefix: &[u16],
    append: &[u8],
    mut code: u16,
    output: &mut Vec<u8>,
    stack: &mut Vec<u8>,
) -> u8 {
    stack.clear();

    // Follow the chain to collect bytes in reverse
    while code >= 258 {
        stack.push(append[code as usize]);
        code = prefix[code as usize];
    }

    // code is now a literal (0-255)
    let first_byte = code as u8;
    output.push(first_byte);

    // Output rest in correct order
    for &b in stack.iter().rev() {
        output.push(b);
    }

    first_byte
}

/// MSB-first bit reader for Sierra's LZW1 decompression.
struct BitReaderMsb<'a> {
    data: &'a [u8],
    byte_pos: usize,
    bits_left: u32,
    current: u32,
}

impl<'a> BitReaderMsb<'a> {
    fn new(data: &'a [u8]) -> Self {
        Self {
            data,
            byte_pos: 0,
            bits_left: 0,
            current: 0,
        }
    }

    fn read_bits(&mut self, count: u32) -> Result<u32, SciError> {
        // MSB-first: accumulate bits from high to low
        let mut result: u32 = 0;
        let mut bits_needed = count;

        while bits_needed > 0 {
            if self.bits_left == 0 {
                if self.byte_pos >= self.data.len() {
                    return Err(SciError::DecompressionError(
                        "LZW1: unexpected end of data".into(),
                    ));
                }
                self.current = self.data[self.byte_pos] as u32;
                self.byte_pos += 1;
                self.bits_left = 8;
            }

            let take = bits_needed.min(self.bits_left);
            // Extract 'take' bits from the top of current
            let shift = self.bits_left - take;
            let mask = ((1u32 << take) - 1) << shift;
            let bits = (self.current & mask) >> shift;

            result = (result << take) | bits;
            self.bits_left -= take;
            bits_needed -= take;
        }

        Ok(result)
    }
}
