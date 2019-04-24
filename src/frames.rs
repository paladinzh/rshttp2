extern crate tokio;

use tokio::io;
use super::parsers::*;

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
    Settings(SettingsImpl),
}

#[derive(Clone)]
pub enum SettingKey {
    HeaderTableSize,
    EnablePush,
    MaxConcurrentStreams,
    InitialWindowSize,
    MaxFrameSize,
    MaxHeaderListSize,
}

pub const ALL_SETTING_KEYS: [SettingKey; 6] = [
    SettingKey::HeaderTableSize,
    SettingKey::EnablePush,
    SettingKey::MaxConcurrentStreams,
    SettingKey::InitialWindowSize,
    SettingKey::MaxFrameSize,
    SettingKey::MaxHeaderListSize,
];

impl SettingKey {
    pub fn from_h2_id(id: usize) -> SettingKey {
        assert!(id >= 1 && id <= 6, "id={}", id);
        ALL_SETTING_KEYS[id - 1].clone()
    }
}

#[derive(Debug)]
pub struct SettingsImpl {
    ack: bool,
    values: [u32; 7],
}

impl SettingsImpl {
    pub fn new() -> SettingsImpl {
        SettingsImpl{
            ack: false,
            values: [
                0, // placeholder,
                4096, // SETTINGS_HEADER_TABLE_SIZE
                1, // SETTINGS_ENABLE_PUSH
                100, // SETTINGS_MAX_CONCURRENT_STREAMS. RFC-7540 does not specify a default value. nghttp2 engages 100 as default.
                65535, // SETTINGS_INITIAL_WINDOW_SIZE
                16384, // SETTINGS_MAX_FRAME_SIZE
                u32::max_value(), // SETTINGS_MAX_HEADER_LIST_SIZE. By RFC-7540, it should be unlimited.
            ]
        }
    }

    pub fn get(&self, key: SettingKey) -> u32 {
        match key {
            SettingKey::HeaderTableSize => self.values[1],
            SettingKey::EnablePush => self.values[2],
            SettingKey::MaxConcurrentStreams => self.values[3],
            SettingKey::InitialWindowSize => self.values[4],
            SettingKey::MaxFrameSize => self.values[5],
            SettingKey::MaxHeaderListSize => self.values[6],
        }
    }

    pub fn set(&mut self, key: SettingKey, value: u32) {
        match key {
            SettingKey::HeaderTableSize => self.values[1] = value,
            SettingKey::EnablePush => self.values[2] = value,
            SettingKey::MaxConcurrentStreams => self.values[3] = value,
            SettingKey::InitialWindowSize => self.values[4] = value,
            SettingKey::MaxFrameSize => self.values[5] = value,
            SettingKey::MaxHeaderListSize => self.values[6] = value,
        }
    }

    pub fn set_ack(&mut self) {
        self.ack = true;
    }

    pub fn clear_ack(&mut self) {
        self.ack = false;
    }
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
        _ => Err(io::Error::new(io::ErrorKind::InvalidData, "unknown frame type."))
    }
}

pub fn parse_settings_frame(
    header: &FrameHeader,
    body: Vec<u8>,
) -> Result<SettingsImpl, io::Error> {
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
    
    let mut settings = SettingsImpl::new();

    if header.flags & 0x1 > 0 {
        settings.set_ack();
    }

    let mut body: &[u8] = body.as_slice();
    while body.len() > 0 {
        let (buf, identifier) = parse_uint::<u16>(body, 2);
        let (buf, value) = parse_uint::<u32>(buf, 4);

        if identifier >= 1 && identifier <= 6 {
            settings.set(SettingKey::from_h2_id(identifier as usize), value);
        }

        body = buf;
    }

    Ok(settings)
}

