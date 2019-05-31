mod static_table;
mod dynamic_table;
mod huffman;
mod huffman_codes;

use std::slice;
use static_table::*;
use dynamic_table::*;
use super::*;

pub struct Decoder {
    dyntbl: DynamicTable,
}

impl Decoder {
    pub fn with_capacity(cap: usize) -> Decoder {
        Decoder{
            dyntbl: DynamicTable::with_capacity(cap),
        }
    }

    pub fn parse_header_field<'a>(
        &'a mut self,
        input: &'a [u8],
    ) -> Result<(&'a [u8], DecodeResult<'a>), &'static str> {
        if input.is_empty() {
            return Err("shortage of input on deserialization.");
        }

        const INDEXED: (u8, u8) = (0x80, 0x80);
        const LITERAL_WITH_INDEXING: (u8, u8) = (0xC0, 0x40);
        const LITERAL_WITHOUT_INDEXING: (u8, u8) = (0xF0, 0);
        const LITERAL_NEVER_INDEXING: (u8, u8) = (0xF0, 0x10);
        match input[0] {
            x if check_prefix(x, INDEXED) => {
                let (rem, idx) = parse_uint(input, 7)?;
                let (name, value) = self.get_from_index_table(idx as usize)?;
                if value.is_none() {
                    warn!("request a indexed no-value header field. index: {}", idx);
                    return Err("request a indexed no-value header field.");
                }
                let value = value.unwrap();
                Ok((rem, DecodeResult::Normal((name, value))))
            },
            x if check_prefix(x, LITERAL_WITH_INDEXING) => {
                let (rem, idx) = parse_uint(input, 6)?;
                if idx > 0 {
                    let (name, _) = self.get_from_index_table(idx as usize)?;
                    // `name` must be cloned because the referenced can be
                    // dropped during truncation.
                    // Because names are usually short, it is not necessary
                    // to optimize.
                    let name = EnhancedSlice::new_with_vec(
                        name.as_slice().to_vec());
                    let (rem, value) = parse_string(rem)?;
                    self.dyntbl.prepend(name.as_slice(), value.as_slice());
                    Ok((rem, DecodeResult::Normal((name, value))))
                } else {
                    let (rem, name) = parse_string(rem)?;
                    let (rem, value) = parse_string(rem)?;
                    self.dyntbl.prepend(name.as_slice(), value.as_slice());
                    Ok((rem, DecodeResult::Normal((name, value))))
                }
            },
            x if check_prefix(x, LITERAL_WITHOUT_INDEXING) => {
                let (rem, idx) = parse_uint(input, 4)?;
                if idx > 0 {
                    let (name, _) = self.get_from_index_table(idx as usize)?;
                    let (rem, value) = parse_string(rem)?;
                    Ok((rem, DecodeResult::Normal((name, value))))
                } else {
                    let (rem, name) = parse_string(rem)?;
                    let (rem, value) = parse_string(rem)?;
                    Ok((rem, DecodeResult::Normal((name, value))))
                }
            },
            x if check_prefix(x, LITERAL_NEVER_INDEXING) => {
                let (rem, idx) = parse_uint(input, 4)?;
                if idx > 0 {
                    let (name, _) = self.get_from_index_table(idx as usize)?;
                    let (rem, value) = parse_string(rem)?;
                    let (raw, _) = input.split_at(input.len() - rem.len());
                    let raw = EnhancedSlice::new_with_slice(raw);
                    Ok((rem, DecodeResult::NeverIndex((name, value, raw))))
                } else {
                    let (rem, name) = parse_string(rem)?;
                    let (rem, value) = parse_string(rem)?;
                    let (raw, _) = input.split_at(input.len() - rem.len());
                    let raw = EnhancedSlice::new_with_slice(raw);
                    Ok((rem, DecodeResult::NeverIndex((name, value, raw))))
                }
            },
            _ => unreachable!(),
        }
    }


}


#[derive(Debug)]
pub enum DecodeResult<'a> {
    Normal((EnhancedSlice<'a>, EnhancedSlice<'a>)),
    NeverIndex((EnhancedSlice<'a>, EnhancedSlice<'a>, EnhancedSlice<'a>)),
}

impl Decoder {
    // private methods
    fn get_from_index_table(
        &self,
        idx: usize,
    ) -> Result<(EnhancedSlice, Option<EnhancedSlice>), &'static str> {
        if idx < RAW_TABLE.len() {
            self.get_from_static_table(idx)
        } else {
            self.get_from_dynamic_table(idx)
        }
    }

    fn get_from_static_table(
        &self,
        idx: usize,
    ) -> Result<(EnhancedSlice, Option<EnhancedSlice>), &'static str> {
        if idx < 1 {
            warn!("request a out-of-index header field. index: {}", idx);
            return Err("index out of space.");
        }
        let item = &RAW_TABLE[idx];
        let name = EnhancedSlice::new_with_slice(item.name);
        let value = match item.value {
            Some(x) => Some(EnhancedSlice::new_with_slice(x)),
            None => None,
        };
        Ok((name, value))
    }

    fn get_from_dynamic_table(
        &self,
        idx: usize,
    ) -> Result<(EnhancedSlice, Option<EnhancedSlice>), &'static str> {
        if idx >= RAW_TABLE.len() +  self.dyntbl.len() {
            warn!("request a out-of-index header field. index: {}", idx);
            return Err("index out of space.");
        }
        let idx = idx - RAW_TABLE.len();
        let item = self.dyntbl.get(idx).unwrap();
        let name = EnhancedSlice::new_with_slice(item.name);
        let value = match item.value {
            Some(x) => Some(EnhancedSlice::new_with_slice(x)),
            None => None,
        };
        Ok((name, value))
    }
}

fn check_prefix(x: u8, criteria: (u8, u8)) -> bool {
    (x & criteria.0) == criteria.1
}

fn parse_uint(
    input: &[u8],
    prefix_bits: usize,
) -> Result<(&[u8], u64), &'static str> {
    if input.is_empty() {
        return Err("shortage of input on deserialization.");
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
    let mut buf = [0u8; 10];
    let mut buf_len = 0usize;
    loop {
        if input.is_empty() {
            return Err("shortage of input on deserialization.");
        }
        let (byte, inp) = input.split_first().unwrap();
        input = inp;
        buf[buf_len] = byte & 0x7Fu8;
        buf_len += 1;
        if buf_len > buf.len() {
            return Err("corrupted data.");
        }
        if byte & 0x80u8 == 0 {
            break;
        }
    }

    let mut res = 0u64;
    while buf_len > 0 {
        buf_len -= 1;
        res <<= 7;
        res |= buf[buf_len] as u64;
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
        return Err("shortage of input on deserialization.");
    }

    let huffman_encoded = (input[0] & 0x80) == 0;
    let (buf, len) = parse_uint(input, 7)?;
    let len = len as usize;
    if buf.len() < len {
        return Err("shortage of input on deserialization.");
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
    fn test_serialize_uint_4() {
        let mut buf: Vec<u8> = vec!();
        let _ = serialize_uint(&mut buf, u64::max_value(), 1, 0).unwrap();
        let (b, trial_value) = parse_uint(buf.as_slice(), 1).unwrap();

        assert_eq!(trial_value, u64::max_value());
        assert!(b.is_empty());
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

    #[test]
    fn test_parse_header_field_indexed_static_table() {
        let buf = vec![0x82u8];
        let mut decoder = Decoder::with_capacity(1);
        let (rem, res) = decoder.parse_header_field(buf.as_slice()).unwrap();
        assert!(rem.is_empty());
        match res {
            DecodeResult::Normal((name, value)) => {
                assert_eq!(name.as_slice(), b":method");
                assert_eq!(value.as_slice(), b"GET");
            },
            _ => panic!(),
        }
    }

    #[test]
    fn test_parse_header_field_indexed_dynamic_table() {
        let mut decoder = Decoder::with_capacity(100);
        const NAME0: &[u8] = b"NAME0";
        const VALUE0: &[u8] = b"VALUE0";
        const NAME1: &[u8] = b"NAME1";
        const VALUE1: &[u8] = b"VALUE1";
        decoder.dyntbl.prepend(NAME0, VALUE0);
        decoder.dyntbl.prepend(NAME1, VALUE1);

        let buf = vec![0xBEu8]; // 62
        let (rem, res) = decoder.parse_header_field(buf.as_slice()).unwrap();
        assert!(rem.is_empty());
        match res {
            DecodeResult::Normal((name, value)) => {
                assert_eq!(name.as_slice(), NAME1);
                assert_eq!(value.as_slice(), VALUE1);
            },
            _ => panic!(),
        }
        
    }

    #[test]
    fn test_parse_header_field_literal_value_incr_index() {
        const AGE: &[u8] = b"123";
        let mut buf: Vec<u8> = vec!();
        {
            let _ = serialize_uint(&mut buf, 21, 6, 0x40).unwrap();
            let _ = serialize_raw_string(&mut buf, AGE).unwrap();
        }
        let mut decoder = Decoder::with_capacity(100);

        let (rem, res) = decoder.parse_header_field(buf.as_slice()).unwrap();
        assert!(rem.is_empty());
        match res {
            DecodeResult::Normal((name, value)) => {
                assert_eq!(name.as_slice(), b"age");
                assert_eq!(value.as_slice(), AGE);
            },
            _ => panic!(),
        };
        assert_eq!(decoder.dyntbl.len(), 1);
        let res = decoder.dyntbl.get(0).unwrap();
        assert_eq!(res.name, b"age");
        assert!(res.value.is_some());
        assert_eq!(res.value.unwrap(), AGE);
    }

    #[test]
    fn test_parse_header_field_literal_name_value_incr_index() {
        const AGE: &[u8] = b"123";
        let mut buf: Vec<u8> = vec!();
        {
            let _ = serialize_uint(&mut buf, 0, 6, 0x40).unwrap();
            let _ = serialize_raw_string(&mut buf, b"age").unwrap();
            let _ = serialize_raw_string(&mut buf, AGE).unwrap();
        }
        let mut decoder = Decoder::with_capacity(100);

        let (rem, res) = decoder.parse_header_field(buf.as_slice()).unwrap();
        assert!(rem.is_empty());
        match res {
            DecodeResult::Normal((name, value)) => {
                assert_eq!(name.as_slice(), b"age");
                assert_eq!(value.as_slice(), AGE);
            },
            _ => panic!(),
        };
        assert_eq!(decoder.dyntbl.len(), 1);
        let res = decoder.dyntbl.get(0).unwrap();
        assert_eq!(res.name, b"age");
        assert!(res.value.is_some());
        assert_eq!(res.value.unwrap(), AGE);
    }

    #[test]
    fn test_parse_header_field_literal_value_without_indexing() {
        const AGE: &[u8] = b"123";
        let mut buf: Vec<u8> = vec!();
        {
            let _ = serialize_uint(&mut buf, 21, 4, 0).unwrap();
            let _ = serialize_raw_string(&mut buf, AGE).unwrap();
        }
        let mut decoder = Decoder::with_capacity(100);

        let (rem, res) = decoder.parse_header_field(buf.as_slice()).unwrap();
        assert!(rem.is_empty());
        match res {
            DecodeResult::Normal((name, value)) => {
                assert_eq!(name.as_slice(), b"age");
                assert_eq!(value.as_slice(), AGE);
            },
            _ => panic!(),
        };
        assert_eq!(decoder.dyntbl.len(), 0);
    }

    #[test]
    fn test_parse_header_field_literal_name_value_without_indexing() {
        const AGE: &[u8] = b"123";
        let mut buf: Vec<u8> = vec!();
        {
            let _ = serialize_uint(&mut buf, 0, 4, 0).unwrap();
            let _ = serialize_raw_string(&mut buf, b"age").unwrap();
            let _ = serialize_raw_string(&mut buf, AGE).unwrap();
        }
        let mut decoder = Decoder::with_capacity(100);

        let (rem, res) = decoder.parse_header_field(buf.as_slice()).unwrap();
        assert!(rem.is_empty());
        match res {
            DecodeResult::Normal((name, value)) => {
                assert_eq!(name.as_slice(), b"age");
                assert_eq!(value.as_slice(), AGE);
            },
            _ => panic!(),
        };
        assert_eq!(decoder.dyntbl.len(), 0);
    }

    #[test]
    fn test_parse_header_field_literal_value_never_indexing() {
        const AGE: &[u8] = b"123";
        let mut buf: Vec<u8> = vec!();
        {
            let _ = serialize_uint(&mut buf, 21, 4, 0x10).unwrap();
            let _ = serialize_raw_string(&mut buf, AGE).unwrap();
        }
        let mut decoder = Decoder::with_capacity(100);

        let (rem, res) = decoder.parse_header_field(buf.as_slice()).unwrap();
        assert!(rem.is_empty());
        match res {
            DecodeResult::NeverIndex((name, value, raw)) => {
                assert_eq!(name.as_slice(), b"age");
                assert_eq!(value.as_slice(), AGE);
                assert_eq!(raw.as_slice(), buf.as_slice());
            },
            _ => {
                println!("{:?}", res);
                panic!()
            },
        };
        assert_eq!(decoder.dyntbl.len(), 0);
    }

    #[test]
    fn test_parse_header_field_literal_name_value_never_indexing() {
        const AGE: &[u8] = b"123";
        let mut buf: Vec<u8> = vec!();
        {
            let _ = serialize_uint(&mut buf, 0, 4, 0x10).unwrap();
            let _ = serialize_raw_string(&mut buf, b"age").unwrap();
            let _ = serialize_raw_string(&mut buf, AGE).unwrap();
        }
        let mut decoder = Decoder::with_capacity(100);

        let (rem, res) = decoder.parse_header_field(buf.as_slice()).unwrap();
        assert!(rem.is_empty());
        match res {
            DecodeResult::NeverIndex((name, value, raw)) => {
                assert_eq!(name.as_slice(), b"age");
                assert_eq!(value.as_slice(), AGE);
                assert_eq!(raw.as_slice(), buf.as_slice());
            },
            _ => {
                println!("{:?}", res);
                panic!()
            },
        };
        assert_eq!(decoder.dyntbl.len(), 0);
    }
}
