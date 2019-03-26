extern crate tokio;
extern crate futures;
#[macro_use] extern crate log;
extern crate flexi_logger;

use tokio::io;
use tokio::net::{TcpListener, TcpStream, Incoming};
use tokio::prelude::*;
use std::net::SocketAddr;
use flexi_logger::{Logger, with_thread};

static PREFACE: &str = "PRI * HTTP/2.0\r\n\r\nSM\r\n\r\n";

struct Connection {
    rx: io::ReadHalf<TcpStream>,
    tx: io::WriteHalf<TcpStream>
}

fn listen_on(addr: &SocketAddr) -> impl Future<Item=(), Error=io::Error>
{
    let listener = TcpListener::bind(addr).unwrap();
    
    let server = listener.incoming().for_each(|socket| {
        let buf = [0u8; 24];
        let in_preface = io::read_exact(socket, buf)
            .and_then(|(socket, buf)| {
                if buf != PREFACE.as_bytes() {
                    error!("HTTP/2 preface mismatch: expect {:?} got {:?}", PREFACE.as_bytes(), buf);
                    return Err(io::Error::new(io::ErrorKind::InvalidData, "HTTP/2 preface mismatch"));
                }
                debug!("read HTTP/2 preface");

                let ack_preface = io::write_all(socket, PREFACE.as_bytes())
                    .and_then(|(socket, _)| {
                        debug!("succeed to send back HTTP/2 preface");
                        let frame_header = [0u8; 9];
                        let settings_frame = io::read_exact(socket, frame_header)
                            .and_then(|(socket, frame_header)| {
                                let mut len = 0u32;
                                {
                                    len = frame_header[0] as u32;
                                    len <<= 8;
                                    len |= frame_header[1] as u32;
                                    len <<= 8;
                                    len |= frame_header[2] as u32;
                                }
                                let mut tp = frame_header[3];
                                let mut flags = frame_header[4];
                                let mut stream_id = 0u32;
                                {
                                    stream_id = frame_header[5] as u32;
                                    stream_id <<= 8;
                                    stream_id |= frame_header[6] as u32;
                                    stream_id <<= 8;
                                    stream_id |= frame_header[7] as u32;
                                    stream_id <<= 8;
                                    stream_id |= frame_header[8] as u32;
                                }
                                debug!("succeed to read header of SETTINGS frame: len={} type={:X} flags={:X} stream_id={}",
                                       len, tp, flags, stream_id);
                                let mut buf = Vec::<u8>::with_capacity(len as usize);
                                buf.resize(len as usize, 0);
                                let settings_body = io::read_exact(socket, buf)
                                    .and_then(|(socket, buf)| {
                                        debug!("succeed to read payload of SETTINGS frame: {:?}", buf);
                                        Ok(())
                                    })
                                    .map_err(|err| {
                                        error!("fail to read payload of SETTINGS frame: {:?}", err);
                                    });
                                tokio::spawn(settings_body);
                                Ok(())
                            })
                            .map_err(|err| {
                                error!("fail to read SETTINGS frame: {:?}", err);
                            });
                        tokio::spawn(settings_frame);
                        Ok(())
                    })
                    .map_err(|err| {
                        error!("fail to read HTTP/2 preface: {:?}", err);
                    });
                tokio::spawn(ack_preface);
                Ok(())
            })
            .map_err(|err| {
                error!("fail to read HTTP/2 preface: {:?}", err);
            });
        tokio::spawn(in_preface);
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
