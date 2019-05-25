pub enum EnhancedSlice<'a> {
    Array((u8, [u8; 15])),
    Slice(&'a [u8]),
    Vec(Vec<u8>),
}

impl<'a> EnhancedSlice<'a> {
    pub fn new_with_slice(v: &[u8]) -> EnhancedSlice {
        if v.len() < 16 {
            let mut dst = [0u8; 15];
            let (used, _) = dst.split_at_mut(v.len());
            used.copy_from_slice(v);
            EnhancedSlice::Array((v.len() as u8, dst))
        } else {
            EnhancedSlice::Slice(v)
        }
    }

    pub fn new_with_vec(v: Vec<u8>) -> EnhancedSlice<'static> {
        if v.len() < 16 {
            let mut dst = [0u8; 15];
            let (used, _) = dst.split_at_mut(v.len());
            used.copy_from_slice(v.as_slice());
            EnhancedSlice::Array((v.len() as u8, dst))
        } else {
            EnhancedSlice::Vec(v)
        }
    }

    pub fn as_slice(&self) -> &[u8] {
        match self {
            EnhancedSlice::Array((len, ref arr)) => {
                let (used, _) = arr.split_at(*len as usize);
                used
            },
            EnhancedSlice::Slice(x) => x,
            EnhancedSlice::Vec(ref x) => x.as_slice(),
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_enhancedslice_short_slice() {
        let s = EnhancedSlice::new_with_slice(b"012");
        assert_eq!(s.as_slice(), b"012");
    }

    #[test]
    fn test_enhancedslice_long_slice() {
        let s = EnhancedSlice::new_with_slice(b"0123456789ABCDEF");
        assert_eq!(s.as_slice(), b"0123456789ABCDEF");
    }

    #[test]
    fn test_enhancedslice_short_vec() {
        let mut vec: Vec<u8> = vec!();
        vec.extend_from_slice(b"0123");
        let s = EnhancedSlice::new_with_vec(vec);
        assert_eq!(s.as_slice(), b"0123");
    }

    #[test]
    fn test_enhancedslice_long_vec() {
        let mut vec: Vec<u8> = vec!();
        vec.extend_from_slice(b"0123456789ABCDEF");
        let s = EnhancedSlice::new_with_vec(vec);
        assert_eq!(s.as_slice(), b"0123456789ABCDEF");
    }
}
