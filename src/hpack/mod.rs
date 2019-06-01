mod static_table;
mod dynamic_table;
mod int;
mod huffman;
mod huffman_codes;
mod string;

use int::*;
use string::*;
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


pub struct Encoder {
    dyntbl: DynamicTable,
    static_seeker: static_table::Seeker,
}

impl Encoder {
    pub fn with_capacity(cap: usize) -> Encoder {
        Encoder{
            dyntbl: DynamicTable::with_capacity(cap),
            static_seeker: static_table::Seeker::new(),
        }
    }

    pub fn encode_header_field(
        &mut self,
        out: &mut Vec<u8>,
        hint: CacheHint,
        name: &[u8],
        value: &[u8],
    ) -> () {
        match hint {
            CacheHint::PREFER_CACHE => {
                let with_caching = |out: &mut Vec<u8>, idx: usize| {
                    serialize_uint(out, idx as u64, 6, 0x40);
                };
                self.encode_(out, name, value, with_caching);
            },
            CacheHint::PREFER_NOT_CACHE => {
                let without_caching = |out: &mut Vec<u8>, idx: usize| {
                    serialize_uint(out, idx as u64, 4, 0x00);
                };
                self.encode_(out, name, value, without_caching);
            },
            CacheHint::NEVER_CACHE => {
                let never_caching = |out: &mut Vec<u8>, idx: usize| {
                    serialize_uint(out, idx as u64, 4, 0x10);
                };
                self.encode_(out, name, value, never_caching);
            },
        };
    }
}

pub enum CacheHint {
    PREFER_CACHE,
    PREFER_NOT_CACHE,
    NEVER_CACHE,
}

impl Encoder {
    // private methods
    fn encode_<T>(
        &mut self,
        out: &mut Vec<u8>,
        name: &[u8],
        value: &[u8],
        idx_encoder: T,
    ) -> ()
    where T: 'static + Fn(&mut Vec<u8>, usize) -> () {
        let idx = self.static_seeker.seek_with_name_value(name, value)
            .or_else(|| {self.dyntbl.seek_with_name_value(name, value)});
        match idx {
            Some(idx) => {
                serialize_uint(out, idx as u64, 7, 0x80);
                return;
            },
            None => (),
        }

        let idx = self.static_seeker.seek_with_name(name)
            .or_else(|| {self.dyntbl.seek_with_name(name)});
        match idx {
            Some(idx) => {
                idx_encoder(out, idx);
                serialize_string(out, value);
                return;
            },
            None => (),
        }

        idx_encoder(out, 0);
        serialize_string(out, name);
        serialize_string(out, value);
    }
}

#[cfg(test)]
mod test {
    use random::Source;
    use super::*;

    #[test]
    fn parse_header_field_indexed_static_table() {
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
    fn parse_header_field_indexed_dynamic_table() {
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
    fn parse_header_field_literal_value_incr_index() {
        const AGE: &[u8] = b"123";
        let mut buf: Vec<u8> = vec!();
        serialize_uint(&mut buf, 21, 6, 0x40);
        serialize_string(&mut buf, AGE);
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
    fn parse_header_field_literal_name_value_incr_index() {
        const AGE: &[u8] = b"123";
        let mut buf: Vec<u8> = vec!();
        serialize_uint(&mut buf, 0, 6, 0x40);
        serialize_string(&mut buf, b"age");
        serialize_string(&mut buf, AGE);
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
    fn parse_header_field_literal_value_without_indexing() {
        const AGE: &[u8] = b"123";
        let mut buf: Vec<u8> = vec!();
        serialize_uint(&mut buf, 21, 4, 0);
        serialize_string(&mut buf, AGE);
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
    fn parse_header_field_literal_name_value_without_indexing() {
        const AGE: &[u8] = b"123";
        let mut buf: Vec<u8> = vec!();
        serialize_uint(&mut buf, 0, 4, 0);
        serialize_string(&mut buf, b"age");
        serialize_string(&mut buf, AGE);
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
    fn parse_header_field_literal_value_never_indexing() {
        const AGE: &[u8] = b"123";
        let mut buf: Vec<u8> = vec!();
        serialize_uint(&mut buf, 21, 4, 0x10);
        serialize_string(&mut buf, AGE);
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
    fn parse_header_field_literal_name_value_never_indexing() {
        const AGE: &[u8] = b"123";
        let mut buf: Vec<u8> = vec!();
        serialize_uint(&mut buf, 0, 4, 0x10);
        serialize_string(&mut buf, b"age");
        serialize_string(&mut buf, AGE);
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
    fn random() {
        let names: Vec<&'static [u8]> = vec![
            b":method",
            b":status",
            b"accept",
            b"smile",
            b"another-smile"];
        let values: Vec<&'static [u8]> = vec![
            b"GET",
            b"POST",
            b"200",
            b"haha",
            b"hoho",
            b"hehe"];
        const CAP: usize = 50;
        const REPEAT: usize = 10000;
        let mut encoder = Encoder::with_capacity(CAP);
        let mut decoder = Decoder::with_capacity(CAP);
        let mut rng = random::default();
        for _ in 0..REPEAT {
            let hint = match rng.read_u64() % 3 {
                0 => CacheHint::PREFER_CACHE,
                1 => CacheHint::PREFER_NOT_CACHE,
                2 => CacheHint::NEVER_CACHE,
                _ => unreachable!(),
            };
            let o_name = names[(rng.read_u64() as usize) % names.len()];
            let o_value = values[(rng.read_u64() as usize) % values.len()];
            let mut buf: Vec<u8> = vec!();
            encoder.encode_header_field(&mut buf, hint, o_name, o_value);
            let (rem, res) = decoder.parse_header_field(buf.as_slice()).unwrap();
            assert!(rem.is_empty(), "{:?}=>{:?}", o_name, o_value);
            match res {
                DecodeResult::Normal((t_name, t_value)) => {
                    assert_eq!(t_name.as_slice(), o_name);
                    assert_eq!(t_value.as_slice(), o_value);
                },
                DecodeResult::NeverIndex((t_name, t_value, t_raw)) => {
                    assert_eq!(t_name.as_slice(), o_name);
                    assert_eq!(t_value.as_slice(), o_value);
                    assert_eq!(t_raw.as_slice(), buf.as_slice());
                },
            }
        }
    }
}
