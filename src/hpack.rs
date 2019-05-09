use std::collections::BTreeMap;

pub struct Context {
    static_table_seeker: StaticTableSeeker,
}

impl Context {
    pub fn new() -> Context {
        Context{
            static_table_seeker: StaticTableSeeker::new()}
    }
}


fn parse_uint(
    b: *const u8,
    e: *const u8,
    prefix_bits: usize,
) -> Result<(*const u8, u64), &'static str> {
    if b >= e {
        return Err("shortage of buf on deserialization.");
    }

    let mask = ((1 << prefix_bits) - 1) as u8;
    let first_byte = unsafe {*b & mask};
    let mut b = unsafe {b.add(1)};
    if first_byte < mask {
        return Ok((b, first_byte as u64))
    }

    let mut buf = vec!();
    loop {
        if b >= e {
            return Err("shortage of buf on deserialization.");
        }
        let byte = unsafe {*b};
        b = unsafe {b.add(1)};
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
    
    Ok((b, res))
}

fn serialize_uint(
    buf: &mut Vec<u8>,
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
        buf.push(res);
        return Ok(())
    }

    {
        let res = (first_byte_flags & flag_mask) | prefix_mask;
        buf.push(res);
        v -= prefix_mask as u64;
    }

    while v > 0x7F {
        let res = 0x80 | ((v & 0x7F) as u8);
        buf.push(res);
        v >>= 7;
    }

    {
        let res = (v & 0x7F) as u8;
        buf.push(res);
    }

    Ok(())
}

#[derive(Debug)]
struct StaticTableItem {
    name: &'static [u8],
    value: Option<&'static [u8]>,
}

const RAW_STATIC_TABLE: [StaticTableItem; 62] = [
    StaticTableItem{name: b"", value: None},

    StaticTableItem{name: b":authority", value: None},
    StaticTableItem{name: b":method", value: Some(b"GET")},
    StaticTableItem{name: b":method", value: Some(b"POST")},
    StaticTableItem{name: b":path", value: Some(b"/")},
    StaticTableItem{name: b":path", value: Some(b"/index.html")},

    StaticTableItem{name: b":scheme", value: Some(b"http")},
    StaticTableItem{name: b":scheme", value: Some(b"https")},
    StaticTableItem{name: b":status", value: Some(b"200")},
    StaticTableItem{name: b":status", value: Some(b"204")},
    StaticTableItem{name: b":status", value: Some(b"206")},
    StaticTableItem{name: b":status", value: Some(b"304")},
    StaticTableItem{name: b":status", value: Some(b"400")},
    StaticTableItem{name: b":status", value: Some(b"404")},
    StaticTableItem{name: b":status", value: Some(b"500")},

    StaticTableItem{name: b"accept-charset", value: None},
    StaticTableItem{name: b"accept-encoding", value: Some(b"gzip, deflate")},
    StaticTableItem{name: b"accept-language", value: None},
    StaticTableItem{name: b"accept-ranges", value: None},
    StaticTableItem{name: b"accept", value: None},
    StaticTableItem{name: b"access-control-allow-origin", value: None},
    StaticTableItem{name: b"age", value: None},
    StaticTableItem{name: b"allow", value: None},
    StaticTableItem{name: b"authorization", value: None},

    StaticTableItem{name: b"cache-control", value: None},
    StaticTableItem{name: b"content-disposition", value: None},
    StaticTableItem{name: b"content-encoding", value: None},
    StaticTableItem{name: b"content-language", value: None},
    StaticTableItem{name: b"content-length", value: None},
    StaticTableItem{name: b"content-location", value: None},
    StaticTableItem{name: b"content-range", value: None},
    StaticTableItem{name: b"content-type", value: None},
    StaticTableItem{name: b"cookie", value: None},

    StaticTableItem{name: b"date", value: None},
    StaticTableItem{name: b"etag", value: None},
    StaticTableItem{name: b"expect", value: None},
    StaticTableItem{name: b"expires", value: None},
    StaticTableItem{name: b"from", value: None},
    StaticTableItem{name: b"host", value: None},

    StaticTableItem{name: b"if-match", value: None},
    StaticTableItem{name: b"if-modified-since", value: None},
    StaticTableItem{name: b"if-none-match", value: None},
    StaticTableItem{name: b"if-range", value: None},
    StaticTableItem{name: b"if-unmodified-since", value: None},

    StaticTableItem{name: b"last-modified", value: None},
    StaticTableItem{name: b"link", value: None},
    StaticTableItem{name: b"location", value: None},

    StaticTableItem{name: b"max-forwards", value: None},
    StaticTableItem{name: b"proxy-authenticate", value: None},
    StaticTableItem{name: b"proxy-authorization", value: None},

    StaticTableItem{name: b"range", value: None},
    StaticTableItem{name: b"referer", value: None},
    StaticTableItem{name: b"refresh", value: None},
    StaticTableItem{name: b"retry-after", value: None},

    StaticTableItem{name: b"server", value: None},
    StaticTableItem{name: b"set-cookie", value: None},
    StaticTableItem{name: b"strict-transport-security", value: None},

    StaticTableItem{name: b"transfer-encoding", value: None},
    StaticTableItem{name: b"user-agent", value: None},
    StaticTableItem{name: b"vary", value: None},
    StaticTableItem{name: b"via", value: None},
    StaticTableItem{name: b"www-authenticate", value: None},
];

mod details {
    use super::BTreeMap;

    pub type HeaderIndexMap = BTreeMap<&'static [u8], usize>;
    pub type SizedHeaderIndexMap = BTreeMap<usize, HeaderIndexMap>;

    pub type ValueIndexMap = BTreeMap<&'static [u8], usize>;
    pub type HeaderValueIndexMap = BTreeMap<&'static [u8], ValueIndexMap>;
    pub type SizedHeaderValueIndexMap = BTreeMap<usize, HeaderValueIndexMap>;
}

struct StaticTableSeeker {
    full_headers: details::SizedHeaderValueIndexMap,
    no_value_headers: details::SizedHeaderIndexMap,
}

impl StaticTableSeeker {
    fn new() -> StaticTableSeeker {
        let mut res = StaticTableSeeker{
            full_headers: details::SizedHeaderValueIndexMap::new(),
            no_value_headers: details::SizedHeaderIndexMap::new()};
        for idx in 1..RAW_STATIC_TABLE.len() {
            let name = RAW_STATIC_TABLE[idx].name;
            let value = RAW_STATIC_TABLE[idx].value;
            match value {
                Some(value) => {
                    let r = res.full_headers
                        .entry(name.len())
                        .or_insert(details::HeaderValueIndexMap::new())
                        .entry(name)
                        .or_insert(details::ValueIndexMap::new())
                        .insert(value, idx);
                    assert!(r.is_none());
                },
                None => {
                    let r = res.no_value_headers
                        .entry(name.len())
                        .or_insert(details::HeaderIndexMap::new())
                        .insert(name, idx);
                    assert!(r.is_none());
                }
            };
        }
        res
    }

    fn seek(&self, header: &[u8], value: &[u8]) -> Option<usize> {
        let res = self.seek_in_no_value_headers(header);
        if res.is_some() {
            return res;
        }

        self.seek_in_full_headers(header, value)
    }

    fn seek_in_no_value_headers(&self, header: &[u8]) -> Option<usize> {
        let header_idx_map = self.no_value_headers.get(&header.len())?;
        let idx = header_idx_map.get(header)?;
        Some(*idx)
    }

    fn seek_in_full_headers(&self, header: &[u8], value: &[u8]) -> Option<usize> {
        let header_value_idx_map = self.full_headers.get(&header.len())?;
        let value_idx_map = header_value_idx_map.get(header)?;
        let idx = value_idx_map.get(value)?;
        Some(*idx)
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_parse_uint_0() {
        let buf = vec!(0u8);
        let b = buf.as_ptr();
        let e = unsafe {b.add(buf.len())};
        let (b, res) = parse_uint(b, e, 5).unwrap();
        assert_eq!(b, e);
        assert_eq!(res, 0);
    }

    #[test]
    fn test_parse_uint_1() {
        let buf = vec!(0x0Au8);
        let b = buf.as_ptr();
        let e = unsafe {b.add(buf.len())};
        let (b, res) = parse_uint(b, e, 5).unwrap();
        assert_eq!(b, e);
        assert_eq!(res, 10);
    }

    #[test]
    fn test_parse_uint_2() {
        let buf = vec!(31u8, 154u8, 10u8);
        let b = buf.as_ptr();
        let e = unsafe {b.add(buf.len())};
        let (b, res) = parse_uint(b, e, 5).unwrap();
        assert_eq!(b, e);
        assert_eq!(res, 1337);
    }

    #[test]
    fn test_parse_uint_err0() {
        let buf: Vec<u8> = vec!();
        let b = buf.as_ptr();
        let e = unsafe {b.add(buf.len())};
        let err = parse_uint(b, e, 5);
        assert!(err.is_err());
    }
    
    #[test]
    fn test_parse_uint_err1() {
        let buf: Vec<u8> = vec!(31u8, 154u8);
        let b = buf.as_ptr();
        let e = unsafe {b.add(buf.len())};
        let err = parse_uint(b, e, 5);
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

                let b = buf.as_ptr();
                let e = unsafe {b.add(buf.len())};
                let (b, trial_value) = parse_uint(b, e, prefix_bits)
                    .unwrap();

                assert_eq!(trial_value, oracle_value);
                assert_eq!(b, e);
            }
        }
    }

    #[test]
    fn test_static_table_seeker_exhaustive() {
        let seeker = StaticTableSeeker::new();
        let none = b"";
        
        for oracle_idx in 1..RAW_STATIC_TABLE.len() {
            let header = RAW_STATIC_TABLE[oracle_idx].name;
            let value = RAW_STATIC_TABLE[oracle_idx].value;

            let trial_idx = match value {
                Some(ref v) => seeker.seek(header, v),
                None => seeker.seek(header, none),
            };

            assert_eq!(trial_idx, Some(oracle_idx));
        }
    }

    #[test]
    fn test_static_table_seeker_nonexist_header() {
        let seeker = StaticTableSeeker::new();
        let res = seeker.seek(b"NOT_EXIST", b"WHATEVER");
        assert!(res.is_none());
    }

    #[test]
    fn test_static_table_seeker_nonexist_value () {
        let seeker = StaticTableSeeker::new();
        let res = seeker.seek(b":status", b"NOT_EXIST");
        assert!(res.is_none());
    }
}
