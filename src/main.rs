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
use rshttp2::*;
use rshttp2::parsers::*;

static PREFACE: &str = "PRI * HTTP/2.0\r\n\r\nSM\r\n\r\n";

struct ConnectionReader {
}

impl ConnectionReader {
    fn new() -> ConnectionReader {
        ConnectionReader{}
    }
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
    let buf = [0u8; 9];
    io::read_exact(socket_in, buf)
        .and_then(|(socket_in, buf)| {
            let buf: &[u8] = &buf;
            let frame_header = parse_header(buf);
            let mut body = Vec::<u8>::with_capacity(frame_header.body_len);
            body.resize(frame_header.body_len, 0);
            io::read_exact(socket_in, body)
                .and_then(move |(socket_in, body)| {
                    debug!("succeed to read payload of a frame with {} bytes", body.len());
                    let frame = parse_frame(&frame_header, body);
                    match frame {
                        Ok(f) => Ok((socket_in, conn, f)),
                        Err(err) => Err(err),
                    }
                })
        })
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
        socket.set_nodelay(true).unwrap();
        let (input, output) = socket.split();
        reader(input);
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
