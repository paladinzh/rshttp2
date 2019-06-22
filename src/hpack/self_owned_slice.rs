use std::cmp::Ordering;
use std::fmt::{Debug, Formatter, Error};
use super::{CachedStr, MaybeOwnedSlice};
use super::super::Sliceable;

pub enum SelfOwnedSlice {
    Array((u8, [u8; 15])),
    Vec(Vec<u8>),
    CachedStr(CachedStr),
}

impl SelfOwnedSlice {
    pub fn new_with_slice(v: &[u8]) -> SelfOwnedSlice {
        let a = SelfOwnedSlice::try_new_with_array(v);
        match a {
            Some(x) => x,
            None => SelfOwnedSlice::Vec(v.to_vec())
        }
    }

    pub fn new_with_vec(v: Vec<u8>) -> SelfOwnedSlice {
        let a = SelfOwnedSlice::try_new_with_array(v.as_slice());
        match a {
            Some(x) => x,
            None => SelfOwnedSlice::Vec(v)
        }
    }

    pub fn new_with_cached_str(v: &CachedStr) -> SelfOwnedSlice {
        let a = SelfOwnedSlice::try_new_with_array(v.as_slice());
        match a {
            Some(x) => x,
            None => SelfOwnedSlice::CachedStr(v.clone())
        }
    }

    pub fn new_with_maybe_owned_slice(v: MaybeOwnedSlice) -> SelfOwnedSlice {
        match v {
            MaybeOwnedSlice::Slice(v) => SelfOwnedSlice::new_with_slice(v),
            MaybeOwnedSlice::Vec(v) => SelfOwnedSlice::new_with_vec(v),
        }
    }

    fn try_new_with_array(v: &[u8]) -> Option<SelfOwnedSlice> {
        if v.len() < 16 {
            let mut dst = [0u8; 15];
            let (used, _) = dst.split_at_mut(v.len());
            used.copy_from_slice(v);
            Some(SelfOwnedSlice::Array((v.len() as u8, dst)))
        } else {
            None
        }
    }
}

impl Sliceable<u8> for SelfOwnedSlice {
    fn as_slice(&self) -> &[u8] {
        match self {
            SelfOwnedSlice::Array((len, ref arr)) => {
                let (used, _) = arr.split_at(*len as usize);
                used
            },
            SelfOwnedSlice::Vec(ref x) => x.as_slice(),
            SelfOwnedSlice::CachedStr(ref x) => x.as_slice(),
        }
    }
}

impl Debug for SelfOwnedSlice {
    fn fmt(&self, f: &mut Formatter) -> Result<(), Error> {
        let s = self.as_slice();
        s.fmt(f)
    }
}

impl PartialOrd for SelfOwnedSlice {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        let a = self.as_slice();
        let b = other.as_slice();
        Some(a.cmp(b))
    }
}

impl Ord for SelfOwnedSlice {
    fn cmp(&self, other: &Self) -> Ordering {
        let a = self.as_slice();
        let b = other.as_slice();
        a.cmp(b)
    }
}

impl PartialEq for SelfOwnedSlice {
    fn eq(&self, other: &Self) -> bool {
        self.cmp(other) == Ordering::Equal
    }
}

impl Eq for SelfOwnedSlice {}

unsafe impl Send for SelfOwnedSlice {}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn enhancedslice_short_slice() {
        let s = SelfOwnedSlice::new_with_slice(b"012");
        assert_eq!(s.as_slice(), b"012");
    }

    #[test]
    fn long_slice() {
        let s = SelfOwnedSlice::new_with_slice(b"0123456789ABCDEF");
        assert_eq!(s.as_slice(), b"0123456789ABCDEF");
    }

    #[test]
    fn short_vec() {
        let mut vec: Vec<u8> = vec!();
        vec.extend_from_slice(b"0123");
        let s = SelfOwnedSlice::new_with_vec(vec);
        assert_eq!(s.as_slice(), b"0123");
    }

    #[test]
    fn long_vec() {
        let mut vec: Vec<u8> = vec!();
        vec.extend_from_slice(b"0123456789ABCDEF");
        let s = SelfOwnedSlice::new_with_vec(vec);
        assert_eq!(s.as_slice(), b"0123456789ABCDEF");
    }

    #[test]
    fn debug_short() {
        let s = SelfOwnedSlice::new_with_slice(b"01");
        let t = format!("{:?}", s);
        let o = format!("{:?}", b"01");
        assert_eq!(t, o);
    }

    #[test]
    fn debug_slice() {
        let s = SelfOwnedSlice::new_with_slice(b"0123456789ABCDEF");
        let t = format!("{:?}", s);
        let o = format!("{:?}", b"0123456789ABCDEF");
        assert_eq!(t, o);
    }

    #[test]
    fn debug_vec() {
        let mut vec: Vec<u8> = vec!();
        vec.extend_from_slice(b"0123456789ABCDEF");
        let s = SelfOwnedSlice::new_with_vec(vec);
        let t = format!("{:?}", s);
        let o = format!("{:?}", b"0123456789ABCDEF");
        assert_eq!(t, o);
    }
}
