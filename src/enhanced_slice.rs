use std::fmt::{Debug, Formatter, Error};
use std::cmp::Ordering;

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

impl<'a> Debug for EnhancedSlice<'a> {
    fn fmt(&self, f: &mut Formatter) -> Result<(), Error> {
        match self {
            EnhancedSlice::Array((len, arr)) => {
                let (used, _) = arr.split_at(*len as usize);
                used.fmt(f)
            },
            EnhancedSlice::Slice(slice) => {
                slice.fmt(f)
            },
            EnhancedSlice::Vec(ref vec) => {
                vec.as_slice().fmt(f)
            }
        }
    }
}

impl<'a> PartialOrd for EnhancedSlice<'a> {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        let a = self.as_slice();
        let b = other.as_slice();
        Some(a.cmp(b))
    }
}

impl<'a> Ord for EnhancedSlice<'a> {
    fn cmp(&self, other: &Self) -> Ordering {
        let a = self.as_slice();
        let b = other.as_slice();
        a.cmp(b)
    }
}

impl<'a> PartialEq for EnhancedSlice<'a> {
    fn eq(&self, other: &Self) -> bool {
        self.cmp(other) == Ordering::Equal
    }
}

impl<'a> Eq for EnhancedSlice<'a> {}

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

    #[test]
    fn test_debug_short() {
        let s = EnhancedSlice::new_with_slice(b"01");
        let t = format!("{:?}", s);
        let o = format!("{:?}", b"01");
        assert_eq!(t, o);
    }

    #[test]
    fn test_debug_slice() {
        let s = EnhancedSlice::new_with_slice(b"0123456789ABCDEF");
        let t = format!("{:?}", s);
        let o = format!("{:?}", b"0123456789ABCDEF");
        assert_eq!(t, o);
    }

    #[test]
    fn test_debug_vec() {
        let mut vec: Vec<u8> = vec!();
        vec.extend_from_slice(b"0123456789ABCDEF");
        let s = EnhancedSlice::new_with_vec(vec);
        let t = format!("{:?}", s);
        let o = format!("{:?}", b"0123456789ABCDEF");
        assert_eq!(t, o);
    }
}
