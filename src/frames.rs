use super::*;
use super::parsers::*;
use super::serializers::*;

#[derive(Debug)]
pub struct FrameHeader {
    pub body_len: usize,
    pub frame_type: u8,
    pub flags: u8,
    pub stream_id: u32,
}

impl FrameHeader {
    pub fn parse(buf: &[u8]) -> FrameHeader {
        assert!(buf.len() == 9, "buf.len() = {}", buf.len());
        let (buf, body_len) = parse_uint::<usize>(buf, 3);
        let (frame_type, buf) = buf.split_first().unwrap();
        let (flags, buf) = buf.split_first().unwrap();
        let (_, stream_id) = parse_uint::<u32>(buf, 4);
        FrameHeader{
            body_len,
            frame_type: *frame_type,
            flags: *flags,
            stream_id}
    }

    pub fn serialize(&self, out: &mut Vec<u8>) {
        serialize_uint(out, self.body_len as u32, 3);
        out.push(self.frame_type);
        out.push(self.flags);
        serialize_uint(out, self.stream_id, 4);
    }
}

#[derive(Debug, Eq, PartialEq)]
pub enum Frame {
    Headers(HeadersFrame), // 1
    Priority(PriorityFrame), // 2
    Settings(SettingsFrame), // 4
    GoAway(GoAwayFrame), // 7
}

impl Frame {
    pub fn parse(
        header: &FrameHeader,
        body: Vec<u8>,
    ) -> Result<Frame, Error> {
        match header.frame_type {
            1 => {
                let f = HeadersFrame::parse(header, body)?;
                Ok(Frame::Headers(f))
            },
            2 => {
                let f = PriorityFrame::parse(header, body)?;
                Ok(Frame::Priority(f))
            }
            4 => {
                let f = SettingsFrame::parse(header, body)?;
                Ok(Frame::Settings(f))
            },
            7 => {
                let f = GoAwayFrame::parse(header, body)?;
                Ok(Frame::GoAway(f))
            }
            _ => Err(Error::new(
                error::Level::ConnectionLevel,
                error::Code::ProtocolError,
                format!("unknown frame type: {}", header.frame_type)))
        }
    }

    pub fn serialize(&self) -> Vec<u8> {
        match self {
            Frame::Settings(f) => f.serialize(),
            Frame::GoAway(f) => f.serialize(),
            _ => panic!("unknown frame type: {:?}", self)
        }
    }
}

#[derive(Debug, Eq, PartialEq)]
pub struct HeadersFrame {
    end_stream: bool,
    end_headers: bool,
    header_block: Vec<u8>,
    padding: Option<Vec<u8>>,
    priority: Option<PriorityInHeadersFrame>,
}

#[derive(Debug, Eq, PartialEq)]
pub struct PriorityInHeadersFrame {
    weight: u8,
    dependency_stream: u32,
}

impl HeadersFrame {
    fn parse(
        header: &FrameHeader,
        body: Vec<u8>,
    ) -> Result<HeadersFrame, Error> {
        if header.stream_id == 0 {
            return Err(Error::new(
                error::Level::ConnectionLevel,
                error::Code::ProtocolError,
                "HeadersFrame associates with stream 0.".to_string()));
        }

        let mut frame = HeadersFrame{
            end_stream: false,
            end_headers: false,
            header_block: vec!(),
            padding: None,
            priority: None,
        };

        if (header.flags & 0x1) > 0 {
            frame.end_stream = true;
        }
        if (header.flags & 0x4) > 0 {
            frame.end_headers = true;
        }
        let mut padded = false;
        if (header.flags & 0x8) > 0 {
            padded = true;
        }
        let mut prioritized = false;
        if (header.flags & 0x20) > 0 {
            prioritized = true;
        }

        let mut body: &[u8] = body.as_slice();

        let mut pad_len = 0usize;
        if padded {
            let (buf, len) = parse_uint::<u8>(body, 1);
            body = buf;
            pad_len = len as usize;
        }

        if prioritized {
            let (buf, sid) = parse_uint::<u32>(body, 4);
            let (buf, weight) = parse_uint::<u8>(buf, 1);
            body = buf;
            frame.priority = Some(PriorityInHeadersFrame{
                weight,
                dependency_stream: sid});
        }

        if pad_len > body.len() {
            return Err(Error::new(
                error::Level::ConnectionLevel,
                error::Code::ProtocolError,
                "Too long padding.".to_string()));
        }

        {
            let (head, tail) = body.split_at(body.len() - pad_len);
            frame.header_block = head.to_vec();
            if padded {
                frame.padding = Some(tail.to_vec());
            }
        }
        
        Ok(frame)
    }

}

#[derive(Debug, Eq, PartialEq)]
pub struct PriorityFrame {
    my_stream_id: u32,
    dep_stream_id: u32,
    weight: i64,
}

impl PriorityFrame {
    pub fn new(
        my_stream_id: u32,
        dep_stream_id: u32,
        weight: i64
    ) -> PriorityFrame {
        PriorityFrame{
            my_stream_id,
            dep_stream_id,
            weight,
        }
    }

    fn parse(
        header: &FrameHeader,
        body: Vec<u8>,
    ) -> Result<PriorityFrame, Error> {
        if header.stream_id == 0 {
            return Err(Error::new(
                error::Level::ConnectionLevel,
                error::Code::ProtocolError,
                "PriorityFrame must be associated with a stream".to_string()));
        }

        if body.len() != 5 {
            return Err(Error::new(
                error::Level::StreamLevel,
                error::Code::FrameSizeError,
                "PriorityFrame must has a body of length 5.".to_string()));
        }

        let body: &[u8] = body.as_slice();
        let (body, dep_stream_id) = parse_uint::<u32>(body, 4);
        let (_, weight) = parse_uint::<u8>(body, 1);

        Ok(PriorityFrame::new(
            header.stream_id,
            dep_stream_id,
            weight as i64))
    }
}

#[derive(Debug, Eq, PartialEq)]
pub struct SettingsFrame {
    pub ack: bool,
    pub values: Vec<(SettingKey, u32)>,
}

impl SettingsFrame {
    pub fn new(
        ack: bool,
        values: Vec<(SettingKey, u32)>,
    ) -> SettingsFrame {
        SettingsFrame{ack, values}
    }
    
    fn parse(
        header: &FrameHeader,
        body: Vec<u8>,
    ) -> Result<SettingsFrame, Error> {
        assert!(header.frame_type == 4);

        if header.stream_id != 0 {
            return Err(Error::new(
                error::Level::ConnectionLevel,
                error::Code::ProtocolError,
                "a SETTINGS frame can only be applied to the whole connection.".to_string()));
        }

        if body.len() % 6 != 0 {
            return Err(Error::new(
                error::Level::ConnectionLevel,
                error::Code::ProtocolError,
                "body length of a SETTINGS frame must be a multiple of 6 octets.".to_string()));
        }
        
        let mut settings = SettingsFrame{
            ack: false,
            values: vec!(),
        };

        if header.flags & 0x1 > 0 {
            settings.ack = true;
        }

        let mut body: &[u8] = body.as_slice();
        while body.len() > 0 {
            let (buf, identifier) = parse_uint::<u16>(body, 2);
            let (buf, value) = parse_uint::<u32>(buf, 4);

            if identifier >= 1 && identifier <= 6 {
                settings.values.push((SettingKey::from_h2_id(identifier as usize), value));
            }

            body = buf;
        }

        Ok(settings)
    }

    fn serialize(&self) -> Vec<u8> {
        let mut buf = vec!();

        {
            let h = FrameHeader{
                body_len: 6 * self.values.len(),
                frame_type: 4u8,
                flags: if self.ack {1u8} else {0u8},
                stream_id: 0u32};
            h.serialize(&mut buf);
        }
        for (k, v) in &self.values {
            serialize_uint(&mut buf, k.to_h2_id() as u32, 2);
            serialize_uint(&mut buf, *v, 4);
        }
        
        buf
    }
}

#[derive(Debug, Eq, PartialEq)]
pub struct GoAwayFrame {
    pub last_stream_id: u32,
    pub error_code: error::Code,
    pub debug_info: Vec<u8>,
}

impl GoAwayFrame {
    fn new() -> GoAwayFrame {
        GoAwayFrame{
            last_stream_id: 0,
            error_code: error::Code::NoError,
            debug_info: vec!()}
    }

    fn parse(
        header: &FrameHeader,
        body: Vec<u8>,
    ) -> Result<GoAwayFrame, Error> {
        assert!(header.frame_type == 7);

        if header.stream_id != 0 {
            return Err(Error::new(
                error::Level::ConnectionLevel,
                error::Code::ProtocolError,
                "a GOAWAY frame can only be applied to the whole connection.".to_string()));
        }

        let mut frame = GoAwayFrame::new();
        {
            let (buf, last_stream_id) = parse_uint::<u32>(body.as_slice(), 4);
            frame.last_stream_id = last_stream_id;
            let (buf, ec) = parse_uint::<usize>(buf, 4);
            frame.error_code = error::Code::from_h2_id(ec);
            frame.debug_info = buf.to_vec();
        }
        Ok(frame)
    }

    fn serialize(&self) -> Vec<u8> {
        let mut buf = vec!();

        {
            let h = FrameHeader{
                body_len: 0,
                frame_type: 7,
                flags: 0,
                stream_id: 0};
            h.serialize(&mut buf);
        }
        serialize_uint(&mut buf, self.last_stream_id, 4);
        serialize_uint(&mut buf, self.error_code.to_h2_id() as u32, 4);
        buf.extend(self.debug_info.iter());
        
        buf
    }
}

#[cfg(test)]
mod test {
    use random::Source;
    use super::*;

    #[test]
    fn settingsframe_serde() {
        let mut rng = random::default();
        for _ in 0..1000 {
            let ack = if (rng.read_u64() & 1) > 0 {true} else {false};
            let mut values = vec!();
            loop {
                let rnd = (rng.read_u64() as usize) % (ALL_SETTING_KEYS.len() + 1);
                if rnd == 0 {
                    break;
                }
                values.push((SettingKey::from_h2_id(rnd), 0x12345678u32));
            }

            let f_oracle = Frame::Settings(SettingsFrame::new(ack, values));
            let mut buf = f_oracle.serialize();
            let header = FrameHeader::parse(&buf[0..9]);
            let buf = buf.split_off(9);
            let f_trial = Frame::parse(&header, buf);
            match f_trial {
                Ok(f_trial) => assert_eq!(f_trial, f_oracle),
                Err(err) => assert!(false, "{:?}", err),
            }
        }
    }

    fn randomized_vec<T: Eq + Clone>(alphabet: &[T], terminator: T) -> Vec<T> {
        let mut rng = random::default();
        let len = alphabet.len();
        let mut out = vec!();
        loop {
            let x = alphabet[(rng.read_u64() as usize) % len].clone();
            if x == terminator {
                break;
            }
            out.push(x);
        }
        out
    }

    #[test]
    fn goawayframe_serde() {
        let mut rng = random::default();
        for _ in 0..1000 {
            let mut f = GoAwayFrame::new();
            f.last_stream_id = rng.read_u64() as u32;
            f.error_code = error::Code::from_h2_id((rng.read_u64() as usize) % ALL_ERRORS.len());
            f.debug_info = randomized_vec("abcdefghijklmn.".as_bytes(), '.' as u8);

            let f_oracle = Frame::GoAway(f);
            let mut buf = f_oracle.serialize();
            let header = FrameHeader::parse(&buf[0..9]);
            let buf = buf.split_off(9);
            let f_trial = Frame::parse(&header, buf);
            match f_trial {
                Ok(f_trial) => assert_eq!(f_trial, f_oracle),
                Err(err) => assert!(false, "{:?}", err),
            }
        }
    }

}
