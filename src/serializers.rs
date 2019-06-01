use std::convert::From;

pub fn serialize_uint<T>(buf: &mut Vec<u8>, v: T, n: usize) -> ()
where u64: From<T> {
    let vv = u64::from(v);
    let xs = vv.to_be_bytes();
    assert!(n <= xs.len());

    for x in &xs[(xs.len() - n) .. xs.len()] {
        buf.push(*x);
    }
}

#[cfg(test)]
mod test {
    use super::*;
    
    #[test]
    fn serialize_uint_0() {
        let mut buf = vec!();
        serialize_uint(&mut buf, 0x1234u16, 2);
        assert_eq!(buf, [0x12, 0x34]);
    }

    #[test]
    fn serialize_uint_1() {
        let mut buf = vec!();
        serialize_uint(&mut buf, 0x1234u16, 3);
        assert_eq!(buf, [0, 0x12, 0x34]);
    }

    #[test]
    fn serialize_uint_2() {
        let mut buf = vec!();
        serialize_uint(&mut buf, 0x1234u16, 1);
        assert_eq!(buf, [0x34]);
    }
}
