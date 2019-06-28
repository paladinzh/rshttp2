use std::fmt::{Debug, Formatter, Error};
use std::fmt::Write;

pub trait Sliceable {
    fn as_slice(&self) -> &[u8];
}

impl Sliceable for Vec<u8> {
    fn as_slice(&self) -> &[u8] {
        self.as_slice()
    }
}

impl Debug for Sliceable {
    fn fmt(&self, f: &mut Formatter) -> Result<(), Error> {
        fmt_slice(f, self.as_slice())
    }
}

pub struct AnySliceable(Box<dyn Sliceable + Send>);

impl AnySliceable {
    pub fn new(obj: impl Sliceable + Send + 'static) -> AnySliceable {
        AnySliceable(Box::new(obj))
    }
}

impl Sliceable for AnySliceable {
    fn as_slice(&self) -> &[u8] {
        self.0.as_slice()
    }
}

impl Debug for AnySliceable {
    fn fmt(&self, f: &mut Formatter) -> Result<(), Error> {
        (self as &Sliceable).fmt(f)
    }
}


fn fmt_slice(f: &mut Formatter, s: &[u8]) -> Result<(), Error> {
    f.write_str("b\"");
    for b in s {
        match *b {
            b if b == b'\\' => {
                f.write_char('\\');
            },
            b if b >= 32u8 && b < 128u8 => {
                f.write_char(char::from(b));
            },
            b => {
                f.write_str("\\x");
                f.write_char(hex(b >> 4));
                f.write_char(hex(b & 0x0F));
            }
        }
    }
    f.write_char('"');
    Ok(())
}

fn hex(b: u8) -> char {
    const ZERO: u8 = 48u8;
    const A: u8 = 65u8;
    assert!(b < 0x10);
    if b < 10 {
        char::from(b + ZERO)
    } else {
        char::from(b + A)
    }
}
