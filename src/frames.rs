use tokio::io;
use super::parsers::*;
use super::serializers::*;
use super::error::*;
use super::settings::*;

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

#[derive(Debug)]
pub enum Frame {
    Headers(HeadersFrame), // 1
    Settings(SettingsFrame), // 4
    GoAway(GoAwayFrame), // 7
}

impl Frame {
    pub fn parse(
        header: &FrameHeader,
        body: Vec<u8>,
    ) -> Result<Frame, io::Error> {
        match header.frame_type {
            1 => {
                let f = HeadersFrame::parse(header, body)?;
                Ok(Frame::Headers(f))
            },
            4 => {
                let f = SettingsFrame::parse(header, body)?;
                Ok(Frame::Settings(f))
            },
            7 => {
                let f = GoAwayFrame::parse(header, body)?;
                Ok(Frame::GoAway(f))
            }
            _ => Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("unknown frame type: {}", header.frame_type)))
        }
    }

    pub fn serialize(&self) -> Vec<u8> {
        match self {
            Frame::Settings(f) => f.serialize(),
            _ => panic!("unknown frame type: {:?}", self)
        }
    }
}

#[derive(Debug)]
pub struct HeadersFrame {
    end_stream: bool,
    end_headers: bool,
    padded: bool,
    prioritized: bool,
    headers: Vec<u8>,
}

impl HeadersFrame {
    fn parse(
        header: &FrameHeader,
        body: Vec<u8>,
    ) -> Result<HeadersFrame, io::Error> {
        let frame = HeadersFrame{
            end_stream: false,
            end_headers: false,
            padded: false,
            prioritized: false,
            headers: vec!(),
        };

        // TODO:

        Ok(frame)
    }

}

#[derive(Debug)]
pub struct SettingsFrame {
    ack: bool,
    values: Vec<(SettingKey, u32)>,
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
    ) -> Result<SettingsFrame, io::Error> {
        assert!(header.frame_type == 4);

        if header.stream_id != 0 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "a SETTINGS frame can only be applied to the whole connection."));
        }

        if body.len() % 6 != 0 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "body length of a SETTINGS frame must be a multiple of 6 octets."));
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
            serialize_uint(&mut buf, k.to_h2_id() as u32, 4);
            serialize_uint(&mut buf, *v, 4);
        }
        
        buf
    }
}

#[derive(Debug)]
pub struct GoAwayFrame {
    last_stream_id: u32,
    error_code: ErrorCode,
    debug_info: Vec<u8>,
}

impl GoAwayFrame {
    fn parse(
        header: &FrameHeader,
        body: Vec<u8>,
    ) -> Result<GoAwayFrame, io::Error> {
        assert!(header.frame_type == 7);

        if header.stream_id != 0 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "a GOAWAY frame can only be applied to the whole connection."));
        }

        let mut frame = GoAwayFrame{
            last_stream_id: 0,
            error_code: ErrorCode::NoError,
            debug_info: vec!(),
        };

        let (buf, last_stream_id) = parse_uint::<u32>(body.as_slice(), 4);
        frame.last_stream_id = last_stream_id;
        let (buf, ec) = parse_uint::<usize>(buf, 4);
        frame.error_code = ErrorCode::from_h2_id(ec);
        frame.debug_info = buf.to_vec();

        Ok(frame)
    }
}


