mod static_table;
mod dynamic_table;
mod huffman;
mod huffman_codes;

use std::slice;
use std::ptr;
use static_table::*;
use dynamic_table::*;
use super::*;

pub struct Context {
    static_table_seeker: static_table::Seeker,
}

impl Context {
    pub fn new() -> Context {
        Context{
            static_table_seeker: static_table::Seeker::new()}
    }
}


fn parse_uint(
    input: &[u8],
    prefix_bits: usize,
) -> Result<(&[u8], u64), &'static str> {
    if input.is_empty() {
        return Err("shortage of buf on deserialization.");
    }

    let mask = ((1 << prefix_bits) - 1) as u8;
    let (first_byte, input) = {
        let (byte, buf) = input.split_first().unwrap();
        (byte & mask, buf)
    };
    if first_byte < mask {
        return Ok((input, first_byte as u64))
    }

    let mut input = input;
    let mut buf = vec!();
    loop {
        if input.is_empty() {
            return Err("shortage of buf on deserialization.");
        }
        let (byte, inp) = input.split_first().unwrap();
        input = inp;
        buf.push(byte & 0x7Fu8);
        if byte & 0x80u8 == 0 {
            break;
        }
    }

    let mut res = 0u64;
    loop {
        match buf.pop() {
            Some(b) => {
                res <<= 7;
                res |= b as u64;
            },
            None => {
                break;
            }
        }
    }
    res += mask as u64;
    
    Ok((input, res))
}

fn serialize_uint(
    out: &mut Vec<u8>,
    v: u64,
    prefix_bits: usize,
    first_byte_flags: u8,
) -> Result<(), &'static str> {
    let prefix_mask = ((1 << prefix_bits) - 1) as u8;
    let flag_mask = !prefix_mask;
    let mut v = v;

    if v < prefix_mask as u64 {
        let mut res = (v & prefix_mask as u64) as u8;
        res |= first_byte_flags & flag_mask;
        out.push(res);
        return Ok(())
    }

    {
        let res = (first_byte_flags & flag_mask) | prefix_mask;
        out.push(res);
        v -= prefix_mask as u64;
    }

    while v > 0x7F {
        let res = 0x80 | ((v & 0x7F) as u8);
        out.push(res);
        v >>= 7;
    }

    {
        let res = (v & 0x7F) as u8;
        out.push(res);
    }

    Ok(())
}

fn serialize_raw_string(
    out: &mut Vec<u8>,
    value: &[u8],
) -> Result<(), &'static str> {
    let _ = serialize_uint(out, value.len() as u64, 7, 0).unwrap();
    out.extend_from_slice(value);
    Ok(())
}

fn parse_string(input: &[u8]) -> Result<(&[u8], EnhancedSlice), &'static str> {
    if input.is_empty() {
        return Err("shortage of buf on deserialization.");
    }

    let huffman_encoded = (input[0] & 0x80) == 0;
    let (buf, len) = parse_uint(input, 7)?;
    let len = len as usize;
    if buf.len() < len {
        return Err("shortage of buf on deserialization.");
    }
    let (buf, rem) = buf.split_at(len);

    if huffman_encoded {
        let res = EnhancedSlice::new_with_slice(buf);
        Ok((rem, res))
    } else {
        let buf = huffman::decode(buf)?;
        let res = EnhancedSlice::new_with_vec(buf);
        Ok((rem, res))
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_parse_uint_0() {
        let buf = vec!(0u8);
        let (b, res) = parse_uint(buf.as_slice(), 5).unwrap();
        assert!(b.is_empty(), "{:?}", b);
        assert_eq!(res, 0);
    }

    #[test]
    fn test_parse_uint_1() {
        let buf = vec!(0x0Au8);
        let (b, res) = parse_uint(buf.as_slice(), 5).unwrap();
        assert!(b.is_empty(), "{:?}", b);
        assert_eq!(res, 10);
    }

    #[test]
    fn test_parse_uint_2() {
        let buf = vec!(31u8, 154u8, 10u8);
        let (b, res) = parse_uint(buf.as_slice(), 5).unwrap();
        assert!(b.is_empty(), "{:?}", b);
        assert_eq!(res, 1337);
    }

    #[test]
    fn test_parse_uint_err0() {
        let buf: Vec<u8> = vec!();
        let err = parse_uint(buf.as_slice(), 5);
        assert!(err.is_err());
    }
    
    #[test]
    fn test_parse_uint_err1() {
        let buf: Vec<u8> = vec!(31u8, 154u8);
        let err = parse_uint(buf.as_slice(), 5);
        assert!(err.is_err());
    }

    #[test]
    fn test_serialize_uint_0() {
        let mut buf: Vec<u8> = vec!();
        let _ = serialize_uint(&mut buf, 0, 5, 0).unwrap();
        assert_eq!(buf, [0]);
    }

    #[test]
    fn test_serialize_uint_1() {
        let mut buf: Vec<u8> = vec!();
        let _ = serialize_uint(&mut buf, 10, 5, 0xA0).unwrap();
        assert_eq!(buf, [0xAA]);
    }

    #[test]
    fn test_serialize_uint_2() {
        let mut buf: Vec<u8> = vec!();
        let _ = serialize_uint(&mut buf, 1337, 5, 0).unwrap();
        assert_eq!(buf, [31u8, 154u8, 10u8]);
    }

    #[test]
    fn test_serialize_uint_3() {
        let mut buf: Vec<u8> = vec!();
        let _ = serialize_uint(&mut buf, 31, 5, 0).unwrap();
        assert_eq!(buf, [31, 0]);
    }

    #[test]
    fn test_serialize_parse_uint_exhaustive() {
        for prefix_bits in 1usize..9usize {
            for oracle_value in 0u64..2000u64 {
                let mut buf = vec!();
                let _ = serialize_uint(&mut buf, oracle_value, prefix_bits, 0)
                    .unwrap();

                let (b, trial_value) = parse_uint(buf.as_slice(), prefix_bits)
                    .unwrap();

                assert_eq!(trial_value, oracle_value);
                assert!(b.is_empty());
            }
        }
    }

    #[test]
    fn test_serialize_raw_string_0() {
        let mut buf: Vec<u8> = vec!();
        let res = serialize_raw_string(&mut buf, b"");
        assert!(res.is_ok());
        assert_eq!(buf, [0]);
    }

    #[test]
    fn test_serialize_raw_string_1() {
        let mut buf: Vec<u8> = vec!();
        let res = serialize_raw_string(&mut buf, b"custom-key");
        assert!(res.is_ok());
        assert_eq!(
            buf,
            [0x0A, 0x63, 0x75, 0x73, 0x74, 0x6F, 0x6D, 0x2D, 0x6B, 0x65, 0x79]);
    }

    #[test]
    fn test_parse_raw_string_0() {
        let buf = vec!(0u8);
        let (b, res) = parse_string(buf.as_slice()).unwrap();
        assert!(b.is_empty(), "{:?}", b);
        assert_eq!(res.as_slice(), []);
    }

    #[test]
    fn test_parse_raw_string_1() {
        let buf = vec![0x0A, 0x63, 0x75, 0x73, 0x74, 0x6F, 0x6D, 0x2D, 0x6B, 0x65, 0x79];
        let (b, res) = parse_string(buf.as_slice()).unwrap();
        assert!(b.is_empty(), "{:?}", b);
        assert_eq!(res.as_slice(), b"custom-key");
    }

    #[test]
    fn test_parse_huffman_string_0() {
        let buf = vec!(0x80u8);
        let (b, res) = parse_string(buf.as_slice()).unwrap();
        assert!(b.is_empty(), "{:?}", b);
        assert_eq!(res.as_slice(), []);
    }


    #[test]
    fn test_parse_huffman_string_1() {
        let buf = vec![
            0x8C, 0xF1, 0xE3, 0xC2, 0xE5,
            0xF2, 0x3A, 0x6B, 0xA0, 0xAB,
            0x90, 0xF4, 0xFF];
        let (b, res) = parse_string(buf.as_slice()).unwrap();
        assert!(b.is_empty(), "{:?}", b);
        assert_eq!(res.as_slice(), b"www.example.com");
    }
}
