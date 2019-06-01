pub fn parse_uint(
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

pub fn serialize_uint(
    out: &mut Vec<u8>,
    v: u64,
    prefix_bits: usize,
    first_byte_flags: u8,
) -> () {
    let prefix_mask = ((1 << prefix_bits) - 1) as u8;
    let flag_mask = !prefix_mask;
    let mut v = v;

    if v < prefix_mask as u64 {
        let mut res = (v & prefix_mask as u64) as u8;
        res |= first_byte_flags & flag_mask;
        out.push(res);
    } else {
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
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_parse_0() {
        let buf = vec!(0u8);
        let (b, res) = parse_uint(buf.as_slice(), 5).unwrap();
        assert!(b.is_empty(), "{:?}", b);
        assert_eq!(res, 0);
    }

    #[test]
    fn test_parse_1() {
        let buf = vec!(0x0Au8);
        let (b, res) = parse_uint(buf.as_slice(), 5).unwrap();
        assert!(b.is_empty(), "{:?}", b);
        assert_eq!(res, 10);
    }

    #[test]
    fn test_parse_2() {
        let buf = vec!(31u8, 154u8, 10u8);
        let (b, res) = parse_uint(buf.as_slice(), 5).unwrap();
        assert!(b.is_empty(), "{:?}", b);
        assert_eq!(res, 1337);
    }

    #[test]
    fn test_parse_err0() {
        let buf: Vec<u8> = vec!();
        let err = parse_uint(buf.as_slice(), 5);
        assert!(err.is_err());
    }
    
    #[test]
    fn test_parse_err1() {
        let buf: Vec<u8> = vec!(31u8, 154u8);
        let err = parse_uint(buf.as_slice(), 5);
        assert!(err.is_err());
    }

    #[test]
    fn test_serialize_0() {
        let mut buf: Vec<u8> = vec!();
        serialize_uint(&mut buf, 0, 5, 0);
        assert_eq!(buf, [0]);
    }

    #[test]
    fn test_serialize_1() {
        let mut buf: Vec<u8> = vec!();
        serialize_uint(&mut buf, 10, 5, 0xA0);
        assert_eq!(buf, [0xAA]);
    }

    #[test]
    fn test_serialize_2() {
        let mut buf: Vec<u8> = vec!();
        serialize_uint(&mut buf, 1337, 5, 0);
        assert_eq!(buf, [31u8, 154u8, 10u8]);
    }

    #[test]
    fn test_serialize_3() {
        let mut buf: Vec<u8> = vec!();
        serialize_uint(&mut buf, 31, 5, 0);
        assert_eq!(buf, [31, 0]);
    }

    #[test]
    fn test_serialize_4() {
        let mut buf: Vec<u8> = vec!();
        serialize_uint(&mut buf, u64::max_value(), 1, 0);
        let (b, trial_value) = parse_uint(buf.as_slice(), 1).unwrap();

        assert_eq!(trial_value, u64::max_value());
        assert!(b.is_empty());
    }

    #[test]
    fn test_serialize_parse_exhaustive() {
        for prefix_bits in 1usize..9usize {
            for oracle_value in 0u64..2000u64 {
                let mut buf = vec!();
                serialize_uint(&mut buf, oracle_value, prefix_bits, 0);

                let (b, trial_value) = parse_uint(buf.as_slice(), prefix_bits)
                    .unwrap();

                assert_eq!(trial_value, oracle_value);
                assert!(b.is_empty());
            }
        }
    }
}
