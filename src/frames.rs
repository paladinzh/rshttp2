use std::sync::Arc;
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
    Headers(ReceivedHeadersFrame), // 1
    Priority(PriorityFrame), // 2
    Settings(SettingsFrame), // 4
    GoAway(GoAwayFrame), // 7
}

impl Frame {
    pub fn parse(
        conn: &Arc<Connection>,
        header: &FrameHeader,
        body: Vec<u8>,
    ) -> Result<Frame, Error> {
        match header.frame_type {
            1 => {
                let mut decoder = conn.as_ref().header_decoder.lock().unwrap();
                let f = ReceivedHeadersFrame::parse(&mut decoder, header, body)?;
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

#[derive(Debug)]
pub enum SendFrame {
    Settings(SettingsFrame), // 4
    GoAway(GoAwayFrame), // 7
}

impl SendFrame {
    pub fn serialize(&self, conn: &Arc<Connection>) -> Vec<u8> {
        match self {
            SendFrame::Settings(f) => f.serialize(),
            SendFrame::GoAway(f) => f.serialize(),
            _ => panic!("unknown frame type: {:?}", self)
        }
    }
}

#[derive(Debug, Eq, PartialEq)]
pub struct ReceivedHeadersFrame {
    pub stream_id: u32,
    pub end_stream: bool,
    pub end_headers: bool,
    pub header_block: Vec<DecoderField>,
    pub padding: Option<Vec<u8>>,
    pub priority: Option<PriorityInHeadersFrame>,
}

#[derive(Debug, Eq, PartialEq)]
pub struct PriorityInHeadersFrame {
    weight: u8,
    dependency_stream: u32,
}

impl ReceivedHeadersFrame {
    fn parse(
        decoder: &mut hpack::Decoder,
        header: &FrameHeader,
        body: Vec<u8>,
    ) -> Result<ReceivedHeadersFrame, Error> {
        if header.stream_id == 0 {
            return Err(Error::new(
                error::Level::ConnectionLevel,
                error::Code::ProtocolError,
                "ReceivedHeadersFrame associates with stream 0.".to_string()));
        }

        let mut frame = ReceivedHeadersFrame{
            stream_id: header.stream_id,
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
            {
                let mut input: &[u8] = head;
                while !input.is_empty() {
                    match decoder.parse_header_field(input) {
                        Ok((remain, result)) => {
                            frame.header_block.push(result);
                            input = remain;
                        },
                        Err(err) => {
                            return Err(Error::new(
                                error::Level::ConnectionLevel,
                                error::Code::CompressionError,
                                err.to_string(),
                            ));
                        }
                    }
                }
            }
            if padded {
                frame.padding = Some(tail.to_vec());
            }
        }
        
        Ok(frame)
    }

}

#[derive(Debug)]
pub struct SendHeadersFrame {
    stream_id: u32,
    end_stream: bool,
    end_headers: bool,
    headers: Vec<EncoderField>,
    padding: Option<Vec<u8>>,
    priority: Option<PriorityInHeadersFrame>,
}

impl SendHeadersFrame {
    pub fn new(builder: SendHeadersFrameBuilder) -> SendHeadersFrame {
        assert!(builder.stream_id.is_some(), 
            "stream id is required for constructing a SendHeadersFrame");
        assert!(!builder.headers.is_empty(),
            "headers is required for constructing a SendHeadersFrame");
        SendHeadersFrame{
            stream_id: builder.stream_id.unwrap(),
            end_stream: builder.end_stream,
            end_headers: builder.end_headers,
            headers: builder.headers,
            padding: builder.padding,
            priority: builder.priority,
        }
    }

    fn serialize(&self, encoder: &mut hpack::Encoder) -> Vec<u8> {
        let mut header_buf = vec!();
        for field in &self.headers {
            encoder.encode_header_field(&mut header_buf, field);
        }

        let mut main_buf = vec!();
        let mut header = FrameHeader{
            body_len: header_buf.len(),
            frame_type: 1,
            flags: 0,
            stream_id: self.stream_id,
        };

        if self.end_stream {
            header.flags |= 0x1;
        }
        if self.end_headers {
            header.flags |= 0x4;
        }
        if self.padding.is_some() {
            header.flags |= 0x8;
            header.body_len += self.padding.as_ref().unwrap().len() + 1;
        }
        if self.priority.is_some() {
            header.flags |= 0x20;
            header.body_len += 5;
        }
        header.serialize(&mut main_buf);
        if self.padding.is_some() {
            let p = self.padding.as_ref().unwrap();
            serialize_uint(&mut main_buf, p.len() as u64, 1);
        }
        if self.priority.is_some() {
            let p = self.priority.as_ref().unwrap();
            serialize_uint(&mut main_buf, p.dependency_stream, 4);
            serialize_uint(&mut main_buf, p.weight, 1);
        }
        main_buf.append(&mut header_buf);
        if self.padding.is_some() {
            let p = self.padding.as_ref().unwrap();
            main_buf.extend_from_slice(p.as_slice());
        }

        main_buf
    }
}

#[derive(Debug)]
pub struct SendHeadersFrameBuilder {
    stream_id: Option<u32>,
    end_stream: bool,
    end_headers: bool,
    headers: Vec<EncoderField>,
    padding: Option<Vec<u8>>,
    priority: Option<PriorityInHeadersFrame>,
}

impl SendHeadersFrameBuilder {
    pub fn new() -> SendHeadersFrameBuilder {
        SendHeadersFrameBuilder{
            stream_id: None,
            end_stream: false,
            end_headers: false,
            headers: vec!(),
            padding: None,
            priority: None,
        }
    }

    pub fn set_stream_id(&mut self, stream_id: u32) -> &mut Self {
        self.stream_id = Some(stream_id);
        self
    }

    pub fn set_end_stream(&mut self) -> &mut Self {
        self.end_stream = true;
        self
    }

    pub fn set_end_headers(&mut self) -> &mut Self {
        self.end_headers = true;
        self
    }

    pub fn append_header_field(&mut self, field: EncoderField) -> &mut Self {
        self.headers.push(field);
        self
    }

    pub fn set_padding(&mut self, padding: Vec<u8>) -> &mut Self {
        self.padding = Some(padding);
        self
    }

    pub fn set_priority(&mut self, priority: PriorityInHeadersFrame) -> &mut Self {
        self.priority = Some(priority);
        self
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

            let f_oracle = SettingsFrame::new(ack, values);
            let mut buf = f_oracle.serialize();
            let header = FrameHeader::parse(&buf[0..9]);
            let buf = buf.split_off(9);
            let f_trial = SettingsFrame::parse(&header, buf);
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
            let mut f_oracle = GoAwayFrame::new();
            f_oracle.last_stream_id = rng.read_u64() as u32;
            f_oracle.error_code = error::Code::from_h2_id((rng.read_u64() as usize) % ALL_ERRORS.len());
            f_oracle.debug_info = randomized_vec(b"abcdefghijklmn.", b'.');

            let mut buf = f_oracle.serialize();
            let header = FrameHeader::parse(&buf[0..9]);
            let buf = buf.split_off(9);
            let f_trial = GoAwayFrame::parse(&header, buf);
            match f_trial {
                Ok(f_trial) => assert_eq!(f_trial, f_oracle),
                Err(err) => assert!(false, "{:?}", err),
            }
        }
    }

    #[test]
    fn headersframe_serde() {
        let mut rng = random::default();
        let mut encoder = hpack::Encoder::with_capacity(100);
        let mut decoder = hpack::Decoder::with_capacity(100);
        for i in 0..10000 {
            let f_oracle = SendHeadersFrame::new({
                let mut builder = SendHeadersFrameBuilder::new();
                builder.set_stream_id(rng.read_u64() as u32);
                if rng.read_u64() % 2 == 1 {
                    builder.set_end_headers();
                }
                if rng.read_u64() % 2 == 1 {
                    builder.set_end_stream();
                }
                for _ in 0..(rng.read_u64() % 10 + 1) {
                    let t = rng.read_u64() % 3;
                    let name = randomized_vec(b"abcdefghijklmn.", b'.');
                    let value = randomized_vec(b"abcdefghijklmn.", b'.');
                    let field = match t {
                        0 => EncoderField::ToCache((
                            AnySliceable::new(name),
                            AnySliceable::new(value),
                        )),
                        1 => EncoderField::NotCache((
                            AnySliceable::new(name),
                            AnySliceable::new(value),
                        )),
                        2 => EncoderField::NeverCache((
                            AnySliceable::new(name),
                            AnySliceable::new(value),
                        )),
                        _ => unreachable!(),
                    };
                    builder.append_header_field(field);
                };
                let padding = randomized_vec(b"abcde.", b'.');
                if !padding.is_empty() {
                    builder.set_padding(padding);
                }
                if rng.read_u64() % 2 == 1 {
                    builder.set_priority(PriorityInHeadersFrame{
                        weight: rng.read_u64() as u8,
                        dependency_stream: rng.read_u64() as u32,
                    });
                }
                builder
            });
            println!("{} {:?}", i, f_oracle);
            let mut buf = f_oracle.serialize(&mut encoder);

            let header = FrameHeader::parse(&buf[0..9]);
            let buf = buf.split_off(9);
            let f_trial = ReceivedHeadersFrame::parse(&mut decoder, &header, buf);
            match f_trial {
                Ok(f_trial) => {
                    assert_eq!(f_oracle.stream_id, f_trial.stream_id,
                        "{:?} {:?}", f_oracle, f_trial);
                    assert_eq!(f_oracle.end_stream, f_trial.end_stream,
                        "{:?} {:?}", f_oracle, f_trial);
                    assert_eq!(f_oracle.end_headers, f_trial.end_headers,
                        "{:?} {:?}", f_oracle, f_trial);
                    assert_eq!(f_oracle.padding, f_trial.padding,
                        "{:?} {:?}", f_oracle, f_trial);
                    assert_eq!(f_oracle.priority, f_trial.priority,
                        "{:?} {:?}", f_oracle, f_trial);
                    assert_eq!(f_oracle.headers.len(), f_trial.header_block.len(),
                        "{:?} {:?}", f_oracle, f_trial);
                    for i in 0..f_oracle.headers.len() {
                        let field_oracle = &f_oracle.headers[i];
                        let field_trial = &f_trial.header_block[i];
                        match field_oracle {
                            EncoderField::ToCache((o_name, o_value)) => {
                                match field_trial {
                                    DecoderField::Normal((t_name, t_value)) => {
                                        assert_eq!(o_name.as_slice(), t_name.as_slice());
                                        assert_eq!(o_value.as_slice(), t_value.as_slice());
                                    },
                                    _ => panic!(),
                                }
                            },
                            EncoderField::NotCache((o_name, o_value)) => {
                                match field_trial {
                                    DecoderField::Normal((t_name, t_value)) => {
                                        assert_eq!(o_name.as_slice(), t_name.as_slice());
                                        assert_eq!(o_value.as_slice(), t_value.as_slice());
                                    },
                                    _ => panic!(),
                                }
                            },
                            EncoderField::NeverCache((o_name, o_value)) => {
                                match field_trial {
                                    DecoderField::NeverIndex((t_name, t_value, _)) => {
                                        assert_eq!(o_name.as_slice(), t_name.as_slice());
                                        assert_eq!(o_value.as_slice(), t_value.as_slice());
                                    },
                                    _ => panic!(),
                                }
                            },
                            _ => unreachable!(),
                        }
                    }
                },
                Err(err) => assert!(false, "{:?}", err),
            }
        }
    }
}
