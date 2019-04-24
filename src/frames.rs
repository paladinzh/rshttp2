extern crate tokio;

use tokio::io;
use super::parsers::*;
use super::settings::*;

#[derive(Debug)]
pub struct FrameHeader {
    pub body_len: usize,
    pub frame_type: u8,
    pub flags: u8,
    pub stream_id: u32,
}

pub fn parse_header(buf: &[u8]) -> FrameHeader {
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

#[derive(Debug)]
pub enum Frame {
    Settings(SettingsFrame),
}

#[derive(Debug)]
pub struct SettingsFrame {
    ack: bool,
    values: Vec<(SettingKey, u32)>,
}

pub fn parse_frame(
    header: &FrameHeader,
    body: Vec<u8>,
) -> Result<Frame, io::Error> {
    match header.frame_type {
        4 => {
            let f = parse_settings_frame(header, body)?;
            Ok(Frame::Settings(f))
        },
        _ => Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("unknown frame type: {}", header.frame_type)))
    }
}

pub fn parse_settings_frame(
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

