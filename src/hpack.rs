pub fn parse_uint(
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

pub fn serialize_uint(
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

struct StaticTableItem {
    name: &'static str,
    value: Option<&'static str>,
}

const RAW_STATIC_TABLE: [StaticTableItem; 62] = [
    StaticTableItem{name: "", value: None},

    StaticTableItem{name: ":authority", value: None},
    StaticTableItem{name: ":method", value: Some("GET")},
    StaticTableItem{name: ":method", value: Some("POST")},
    StaticTableItem{name: ":path", value: Some("/")},
    StaticTableItem{name: ":path", value: Some("/index.html")},
    StaticTableItem{name: ":scheme", value: Some("http")},
    StaticTableItem{name: ":scheme", value: Some("https")},
    StaticTableItem{name: ":status", value: Some("200")},
    StaticTableItem{name: ":status", value: Some("204")},
    StaticTableItem{name: ":status", value: Some("206")},
    StaticTableItem{name: ":status", value: Some("304")},
    StaticTableItem{name: ":status", value: Some("400")},
    StaticTableItem{name: ":status", value: Some("404")},
    StaticTableItem{name: ":status", value: Some("500")},

    StaticTableItem{name: "accept-charset", value: None},
    StaticTableItem{name: "accept-encoding", value: Some("gzip, deflate")},
    StaticTableItem{name: "accept-language", value: None},
    StaticTableItem{name: "accept-ranges", value: None},
    StaticTableItem{name: "accept", value: None},
    StaticTableItem{name: "access-control-allow-origin", value: None},
    StaticTableItem{name: "age", value: None},
    StaticTableItem{name: "allow", value: None},
    StaticTableItem{name: "authorization", value: None},

    StaticTableItem{name: "cache-control", value: None},
    StaticTableItem{name: "content-disposition", value: None},
    StaticTableItem{name: "content-encoding", value: None},
    StaticTableItem{name: "content-language", value: None},
    StaticTableItem{name: "content-length", value: None},
    StaticTableItem{name: "content-location", value: None},
    StaticTableItem{name: "content-range", value: None},
    StaticTableItem{name: "content-type", value: None},
    StaticTableItem{name: "cookie", value: None},

    StaticTableItem{name: "date", value: None},
    StaticTableItem{name: "etag", value: None},
    StaticTableItem{name: "expect", value: None},
    StaticTableItem{name: "expires", value: None},
    StaticTableItem{name: "from", value: None},
    StaticTableItem{name: "host", value: None},

    StaticTableItem{name: "if-match", value: None},
    StaticTableItem{name: "if-modified-since", value: None},
    StaticTableItem{name: "if-none-match", value: None},
    StaticTableItem{name: "if-range", value: None},
    StaticTableItem{name: "if-unmodified-since", value: None},

    StaticTableItem{name: "last-modified", value: None},
    StaticTableItem{name: "link", value: None},
    StaticTableItem{name: "location", value: None},

    StaticTableItem{name: "max-forwards", value: None},
    StaticTableItem{name: "proxy-authenticate", value: None},
    StaticTableItem{name: "proxy-authorization", value: None},

    StaticTableItem{name: "range", value: None},
    StaticTableItem{name: "referer", value: None},
    StaticTableItem{name: "refresh", value: None},
    StaticTableItem{name: "retry-after", value: None},

    StaticTableItem{name: "server", value: None},
    StaticTableItem{name: "set-cookie", value: None},
    StaticTableItem{name: "strict-transport-security", value: None},

    StaticTableItem{name: "transfer-encoding", value: None},
    StaticTableItem{name: "user-agent", value: None},
    StaticTableItem{name: "vary", value: None},
    StaticTableItem{name: "via", value: None},
    StaticTableItem{name: "www-authenticate", value: None},
];

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
}
