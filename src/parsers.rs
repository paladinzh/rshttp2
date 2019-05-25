use std::convert::From;
use std::slice;
use std::ops::*;

pub fn parse_uint<'a, T>(buf: &'a [u8], n: usize) -> (&'a [u8], T)
where T: From<u8> + ShlAssign<usize> + BitOrAssign<T> {
    assert!(buf.len() >= n, "buf.len()={} n={}", buf.len(), n);
    if n == 1 {
        let (b, buf) = buf.split_first().unwrap();
        (buf, T::from(*b))
    } else {
        let mut b = buf.as_ptr();
        let mut res: T = T::from(0u8);
        unsafe {
            for _ in 0..n {
                res <<= 8;
                res |= T::from(*b);
                b = b.add(1);
            }
            (slice::from_raw_parts(b, buf.len() - n), res)
        }
    }
}


#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_parse_uint_0() {
        let buf: [u8; 5] = [1, 2, 3, 4, 5];
        let (remain_buf, res) = parse_uint::<u64>(&buf, 0);
        assert_eq!(res, 0);
        assert_eq!(remain_buf.len(), 5);
        assert_eq!(remain_buf[0], 1);
    }

    #[test]
    fn test_parse_uint_1() {
        let buf: [u8; 5] = [1, 2, 3, 4, 5];
        let (remain_buf, res) = parse_uint::<u64>(&buf, 4);
        assert_eq!(res, 0x01020304);
        assert_eq!(remain_buf.len(), 1);
        assert_eq!(remain_buf[0], 5);
    }

    #[test]
    fn test_parse_uint_2() {
        let buf: [u8; 5] = [1, 2, 3, 4, 5];
        let (remain_buf, res) = parse_uint::<u8>(&buf, 1);
        assert_eq!(res, 0x1u8);
        assert_eq!(remain_buf.len(), 4);
        assert_eq!(remain_buf[0], 0x2);
    }
}
