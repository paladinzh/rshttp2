use super::huffman;
use super::int::*;
use super::super::Sliceable;

pub fn serialize_string(out: &mut Vec<u8>, input: &[u8]) -> () {
    if input.len() < 16 {
        serialize_raw_string(out, input)
    } else {
        huffman::encode(out, input)
    }
}

fn serialize_raw_string(out: &mut Vec<u8>, input: &[u8]) -> () {
    serialize_uint(out, input.len() as u64, 7, 0);
    out.extend_from_slice(input);
}

pub fn parse_string(input: &[u8]) -> Result<(&[u8], MaybeOwnedSlice), &'static str> {
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
        let res = MaybeOwnedSlice::new_with_slice(buf);
        Ok((rem, res))
    } else {
        let buf = huffman::decode(buf)?;
        let res = MaybeOwnedSlice::new_with_vec(buf);
        Ok((rem, res))
    }
}

pub enum MaybeOwnedSlice<'a> {
    Slice(&'a [u8]),
    Vec(Vec<u8>),
}

impl<'a> MaybeOwnedSlice<'a> {
    fn new_with_slice(v: &[u8]) -> MaybeOwnedSlice {
        MaybeOwnedSlice::Slice(v)
    }

    fn new_with_vec(v: Vec<u8>) -> MaybeOwnedSlice<'static> {
        MaybeOwnedSlice::Vec(v)
    }
}

impl<'a> Sliceable for MaybeOwnedSlice<'a> {
    fn as_slice(&self) -> &[u8] {
        match self {
            MaybeOwnedSlice::Slice(x) => x,
            MaybeOwnedSlice::Vec(ref x) => x.as_slice(),
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn serialize_raw_string_0() {
        let mut buf: Vec<u8> = vec!();
        serialize_raw_string(&mut buf, b"");
        assert_eq!(buf, [0]);
    }

    #[test]
    fn serialize_raw_string_1() {
        let mut buf: Vec<u8> = vec!();
        serialize_raw_string(&mut buf, b"custom-key");
        assert_eq!(
            buf,
            [0x0A, 0x63, 0x75, 0x73, 0x74, 0x6F, 0x6D, 0x2D, 0x6B, 0x65, 0x79]);
    }

    #[test]
    fn parse_raw_string_0() {
        let buf = vec!(0u8);
        let (b, res) = parse_string(buf.as_slice()).unwrap();
        assert!(b.is_empty(), "{:?}", b);
        assert_eq!(res.as_slice(), []);
    }

    #[test]
    fn parse_raw_string_1() {
        let buf = vec![0x0A, 0x63, 0x75, 0x73, 0x74, 0x6F, 0x6D, 0x2D, 0x6B, 0x65, 0x79];
        let (b, res) = parse_string(buf.as_slice()).unwrap();
        assert!(b.is_empty(), "{:?}", b);
        assert_eq!(res.as_slice(), b"custom-key");
    }

    #[test]
    fn parse_huffman_string_0() {
        let buf = vec!(0x80u8);
        let (b, res) = parse_string(buf.as_slice()).unwrap();
        assert!(b.is_empty(), "{:?}", b);
        assert_eq!(res.as_slice(), []);
    }

    #[test]
    fn parse_huffman_string_1() {
        let buf = vec![
            0x8C, 0xF1, 0xE3, 0xC2, 0xE5,
            0xF2, 0x3A, 0x6B, 0xA0, 0xAB,
            0x90, 0xF4, 0xFF];
        let (b, res) = parse_string(buf.as_slice()).unwrap();
        assert!(b.is_empty(), "{:?}", b);
        assert_eq!(res.as_slice(), b"www.example.com");
    }

}
