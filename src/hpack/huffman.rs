use super::huffman_codes::*;

pub fn decode(
    b: *const u8,
    e: *const u8,
) -> Result<(*const u8, Vec<u8>), &'static str> {
    Ok((b, vec!()))
}

