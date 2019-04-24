extern crate tokio;
extern crate futures;
#[macro_use] extern crate log;
extern crate flexi_logger;

//use std::sync::Arc;
use tokio::io;
use tokio::net::TcpListener;
use tokio::prelude::*;
use std::net::SocketAddr;
use flexi_logger::{Logger, with_thread};
use rshttp2::parsers::*;

static PREFACE: &str = "PRI * HTTP/2.0\r\n\r\nSM\r\n\r\n";

struct ConnectionReader {
}

impl ConnectionReader {
    fn new() -> ConnectionReader {
        ConnectionReader{}
    }
}

enum SettingKey {
    HeaderTableSize,
    EnablePush,
    MaxConcurrentStreams,
    InitialWindowSize,
    MaxFrameSize,
    MaxHeaderListSize,
}

#[derive(Debug)]
struct SettingsImpl {
    ack: bool,
    values: [u32; 7],
}

impl SettingsImpl {
    fn new() -> SettingsImpl {
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

    fn get(&self, key: SettingKey) -> u32 {
        match key {
            SettingKey::HeaderTableSize => self.values[1],
            SettingKey::EnablePush => self.values[2],
            SettingKey::MaxConcurrentStreams => self.values[3],
            SettingKey::InitialWindowSize => self.values[4],
            SettingKey::MaxFrameSize => self.values[5],
            SettingKey::MaxHeaderListSize => self.values[6],
        }
    }

    fn set(&mut self, key: SettingKey, value: u32) {
        match key {
            SettingKey::HeaderTableSize => self.values[1] = value,
            SettingKey::EnablePush => self.values[2] = value,
            SettingKey::MaxConcurrentStreams => self.values[3] = value,
            SettingKey::InitialWindowSize => self.values[4] = value,
            SettingKey::MaxFrameSize => self.values[5] = value,
            SettingKey::MaxHeaderListSize => self.values[6] = value,
        }
    }
}

#[derive(Debug)]
struct RawFrame {
    frame_type: u8,
    flags: u8,
    stream_id: u32,
    body: Vec<u8>,
}

#[derive(Debug)]
enum Frame {
    Settings(SettingsImpl),
}

fn read_preface<R: 'static + Send + AsyncRead>(
    socket_in: R,
    conn: ConnectionReader,
) -> impl Future<Item = (R, ConnectionReader), Error = io::Error> {
    let buf = [0u8; 24];
    io::read_exact(socket_in, buf)
        .then(move |result| {
            match result {
                Err(err) => {
                    error!("fail to read HTTP/2 preface: {:?}", err);
                    return Err(io::Error::new(io::ErrorKind::InvalidData, "fail to read HTTP/2 preface"));
                },
                Ok((socket_in, buf)) => {
                    if buf != PREFACE.as_bytes() {
                        error!("HTTP/2 preface mismatch: expect {:?} got {:?}", PREFACE.as_bytes(), buf);
                        return Err(io::Error::new(io::ErrorKind::InvalidData, "HTTP/2 preface mismatch"));
                    } else {
                        debug!("read HTTP/2 preface");
                        return Ok((socket_in, conn));
                    }
                }
            }
        })
}

fn read_settings<R: 'static + Send + AsyncRead>(
    socket_in: R,
    conn: ConnectionReader,
) -> impl Future<Item = (R, ConnectionReader), Error = io::Error> {
    read_frame(socket_in, conn)
        .and_then(|(socket_in, conn, frame)| {
            debug!("got a frame: {:?}", frame);
            Ok((socket_in, conn))
        })
}

fn read_frame<R: 'static + Send + AsyncRead>(
    socket_in: R,
    conn: ConnectionReader,
) -> impl Future<Item = (R, ConnectionReader, Frame), Error = io::Error> {
    let frame_header = [0u8; 9];
    io::read_exact(socket_in, frame_header)
        .and_then(|(socket_in, frame_header)| {
            let buf: &[u8] = &frame_header;
            let (buf, len) = parse_uint::<usize>(buf, 3);
            let (tp, buf) = buf.split_first().unwrap();
            let (flags, buf) = buf.split_first().unwrap();
            let (buf, stream_id) = parse_uint::<u32>(buf, 4);
            let mut raw_frame = RawFrame{
                frame_type: *tp,
                flags: *flags,
                stream_id,
                body: vec!{}};
            let mut buf = Vec::<u8>::with_capacity(len);
            buf.resize(len, 0);
            io::read_exact(socket_in, buf)
                .and_then(move |(socket_in, buf)| {
                    raw_frame.body = buf;
                    debug!("succeed to read payload of a frame: {:?}", raw_frame);
                    let frame = parse_frame(raw_frame);
                    match frame {
                        Ok(f) => Ok((socket_in, conn, f)),
                        Err(err) => Err(err),
                    }
                })
        })
}

fn parse_frame(raw: RawFrame) -> Result<Frame, io::Error> {
    match raw.frame_type {
        4 => {
            let f = parse_settings_frame(raw)?;
            Ok(Frame::Settings(f))
        },
        _ => Err(io::Error::new(io::ErrorKind::InvalidData, "unknown frame type."))
    }
}

fn parse_settings_frame(raw: RawFrame) -> Result<SettingsImpl, io::Error> {
    assert!(raw.frame_type == 4);

    if raw.stream_id != 0 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "a SETTINGS frame can only be applied to the whole connection."));
    }

    if raw.body.len() % 6 != 0 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "body length of a SETTINGS frame must be a multiple of 6 octets."));
    }
    
    let mut settings = SettingsImpl::new();

    if raw.flags & 0x1 > 0 {
        settings.ack = true;
    }

    let mut body: &[u8] = raw.body.as_slice();
    while body.len() > 0 {
        let (buf, identifier) = parse_uint::<u16>(body, 2);
        let (buf, value) = parse_uint::<u32>(buf, 4);

        if identifier >= 1 && identifier <= 6 {
            settings.values[identifier as usize] = value;
        }

        body = buf;
    }

    Ok(settings)
}

fn reader<R: 'static + Send + AsyncRead>(socket_in: R) -> () {
    debug!("start to handshake an incoming connection");
    let conn = ConnectionReader::new();
    let task = read_preface(socket_in, conn)
        .and_then(|(socket_in, conn)| {
            read_settings(socket_in, conn)
        })
        .and_then(|_| {
            Ok(())
        })
        .map_err(|err| {
            error!("Read error: {:?}", err);
        });
    tokio::spawn(task);
}

fn listen_on(addr: &SocketAddr) -> impl Future<Item=(), Error=io::Error> {
    let listener = TcpListener::bind(addr).unwrap();
    
    let server = listener.incoming().for_each(|socket| {
        let (input, output) = socket.split();
        reader(input);
        // let buf = [0u8; 24];
        // let in_preface = io::read_exact(socket, buf)
        //     .and_then(|(socket, buf)| {
        //         if buf != PREFACE.as_bytes() {
        //             error!("HTTP/2 preface mismatch: expect {:?} got {:?}", PREFACE.as_bytes(), buf);
        //             return Err(io::Error::new(io::ErrorKind::InvalidData, "HTTP/2 preface mismatch"));
        //         }
        //         debug!("read HTTP/2 preface");

        //         Ok(())
        //     })
        //     .map_err(|err| {
        //         error!("fail to read HTTP/2 preface: {:?}", err);
        //     });
        // tokio::spawn(in_preface);
        Ok(())
    });
    server
}

fn main() {
    Logger::with_env()
        .format(with_thread)
        .start()
        .unwrap();
    let addr = "127.0.0.1:2333".parse().unwrap();
    let server = listen_on(&addr)
        .map_err(|err| {
            error!("accept error = {:?}", err);
        });
    
    debug!("server running on localhost:2333");
    
    tokio::run(server);
}
