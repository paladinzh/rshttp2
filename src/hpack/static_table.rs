use std::collections::BTreeMap;

#[derive(Debug)]
pub struct Item {
    pub name: &'static [u8],
    pub value: Option<&'static [u8]>,
}

pub const RAW_TABLE: [Item; 62] = [
    Item{name: b"", value: None},

    Item{name: b":authority", value: None},
    Item{name: b":method", value: Some(b"GET")},
    Item{name: b":method", value: Some(b"POST")},
    Item{name: b":path", value: Some(b"/")},
    Item{name: b":path", value: Some(b"/index.html")},

    Item{name: b":scheme", value: Some(b"http")},
    Item{name: b":scheme", value: Some(b"https")},
    Item{name: b":status", value: Some(b"200")},
    Item{name: b":status", value: Some(b"204")},
    Item{name: b":status", value: Some(b"206")},
    Item{name: b":status", value: Some(b"304")},
    Item{name: b":status", value: Some(b"400")},
    Item{name: b":status", value: Some(b"404")},
    Item{name: b":status", value: Some(b"500")},

    Item{name: b"accept-charset", value: None},
    Item{name: b"accept-encoding", value: Some(b"gzip, deflate")},
    Item{name: b"accept-language", value: None},
    Item{name: b"accept-ranges", value: None},
    Item{name: b"accept", value: None},
    Item{name: b"access-control-allow-origin", value: None},
    Item{name: b"age", value: None},
    Item{name: b"allow", value: None},
    Item{name: b"authorization", value: None},

    Item{name: b"cache-control", value: None},
    Item{name: b"content-disposition", value: None},
    Item{name: b"content-encoding", value: None},
    Item{name: b"content-language", value: None},
    Item{name: b"content-length", value: None},
    Item{name: b"content-location", value: None},
    Item{name: b"content-range", value: None},
    Item{name: b"content-type", value: None},
    Item{name: b"cookie", value: None},

    Item{name: b"date", value: None},
    Item{name: b"etag", value: None},
    Item{name: b"expect", value: None},
    Item{name: b"expires", value: None},
    Item{name: b"from", value: None},
    Item{name: b"host", value: None},

    Item{name: b"if-match", value: None},
    Item{name: b"if-modified-since", value: None},
    Item{name: b"if-none-match", value: None},
    Item{name: b"if-range", value: None},
    Item{name: b"if-unmodified-since", value: None},

    Item{name: b"last-modified", value: None},
    Item{name: b"link", value: None},
    Item{name: b"location", value: None},

    Item{name: b"max-forwards", value: None},
    Item{name: b"proxy-authenticate", value: None},
    Item{name: b"proxy-authorization", value: None},

    Item{name: b"range", value: None},
    Item{name: b"referer", value: None},
    Item{name: b"refresh", value: None},
    Item{name: b"retry-after", value: None},

    Item{name: b"server", value: None},
    Item{name: b"set-cookie", value: None},
    Item{name: b"strict-transport-security", value: None},

    Item{name: b"transfer-encoding", value: None},
    Item{name: b"user-agent", value: None},
    Item{name: b"vary", value: None},
    Item{name: b"via", value: None},
    Item {name: b"www-authenticate", value: None},
];

type HeaderIndexMap = BTreeMap<&'static [u8], usize>;
type SizedHeaderIndexMap = BTreeMap<usize, HeaderIndexMap>;

type ValueIndexMap = BTreeMap<&'static [u8], usize>;
type HeaderValueIndexMap = BTreeMap<&'static [u8], ValueIndexMap>;
type SizedHeaderValueIndexMap = BTreeMap<usize, HeaderValueIndexMap>;

pub struct Seeker {
    full_headers: SizedHeaderValueIndexMap,
    no_value_headers: SizedHeaderIndexMap,
}

impl Seeker {
    pub fn new() -> Seeker {
        let mut res = Seeker{
            full_headers: SizedHeaderValueIndexMap::new(),
            no_value_headers: SizedHeaderIndexMap::new()};
        for idx in 1..RAW_TABLE.len() {
            let name = RAW_TABLE[idx].name;
            let value = RAW_TABLE[idx].value;
            match value {
                Some(value) => {
                    let r = res.full_headers
                        .entry(name.len())
                        .or_insert(HeaderValueIndexMap::new())
                        .entry(name)
                        .or_insert(ValueIndexMap::new())
                        .insert(value, idx);
                    assert!(r.is_none());
                    let _ = res.no_value_headers
                        .entry(name.len())
                        .or_insert(HeaderIndexMap::new())
                        .insert(name, idx);
                },
                None => {
                    let r = res.no_value_headers
                        .entry(name.len())
                        .or_insert(HeaderIndexMap::new())
                        .insert(name, idx);
                    assert!(r.is_none());
                }
            };
        }
        res
    }

    pub fn seek_with_name(&self, name: &[u8]) -> Option<usize> {
        let header_idx_map = self.no_value_headers.get(&name.len())?;
        let idx = header_idx_map.get(name)?;
        Some(*idx)
    }

    pub fn seek_with_name_value(&self, name: &[u8], value: &[u8]) -> Option<usize> {
        let header_value_idx_map = self.full_headers.get(&name.len())?;
        let value_idx_map = header_value_idx_map.get(name)?;
        let idx = value_idx_map.get(value)?;
        Some(*idx)
    }
}


#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn seeker_exhaustive() {
        let seeker = Seeker::new();
        
        for oracle_idx in 1..RAW_TABLE.len() {
            let header = RAW_TABLE[oracle_idx].name;
            let value = RAW_TABLE[oracle_idx].value;

            let trial_idx = match value {
                Some(ref v) => seeker.seek_with_name_value(header, v),
                None => seeker.seek_with_name(header),
            };

            assert_eq!(trial_idx, Some(oracle_idx));
        }
    }

    #[test]
    fn seeker_nonexist_header() {
        let seeker = Seeker::new();
        let res = seeker.seek_with_name(b"NOT_EXIST");
        assert!(res.is_none());
        let res = seeker.seek_with_name_value(b"NOT_EXIST", b"WHATEVER");
        assert!(res.is_none());
    }

    #[test]
    fn seeker_nonexist_value () {
        let seeker = Seeker::new();
        let res = seeker.seek_with_name(b":status");
        assert!(res.is_some());
        let res = seeker.seek_with_name_value(b":status", b"NOT_EXIST");
        assert!(res.is_none());
    }
}
