use std::sync::{Arc};
use tokio::sync::lock::{Lock, LockGuard};
use tokio::io;
use tokio::net::TcpStream;
use tokio::prelude::*;
use super::frames::*;

pub fn on_connect<F>(
    tcp: TcpStream,
    on_frame: F,
) -> Result<(), io::Error>
where F: 'static + Sync + Send + Fn(Arc<Connection>, Frame) -> () {
    tcp.set_nodelay(true).unwrap();
    let conn = Connection::new(on_frame);
    let (input, output) = tcp.split();
    reader(input, conn.clone());
    Ok(())
}

const PREFACE: &str = "PRI * HTTP/2.0\r\n\r\nSM\r\n\r\n";

pub struct Connection {
    on_frame: FnBox,
}

struct FnBox(Box<dyn Fn(Arc<Connection>, Frame) -> ()>);

unsafe impl Send for FnBox {}
unsafe impl Sync for FnBox {}

impl FnBox {
    fn new<F>(f: F) -> FnBox
    where F: 'static + Sync + Send + Fn(Arc<Connection>, Frame) -> () {
        FnBox(Box::new(f))
    }
}


impl Connection {
    fn new<F>(on_frame: F) -> Arc<Connection>
    where F: 'static + Sync + Send + Fn(Arc<Connection>, Frame) -> () {
        Arc::new(Connection{
            on_frame: FnBox::new(on_frame)})
    }
}

fn reader<R>(
    socket_in: R,
    conn: Arc<Connection>,
) -> ()
where R: 'static + Send + AsyncRead {
    debug!("start to handshake an incoming connection");
    let task = read_preface(socket_in, conn)
        .and_then(|(socket_in, conn)| {
            read_settings(socket_in, conn)
        })
        .and_then(|(socket_in, conn)| {
            reader_continuation(socket_in, conn);
            Ok(())
        })
        .map_err(|err| {
            error!("Read error: {:?}", err);
        });
    tokio::spawn(task);
}

fn reader_continuation<R>(
    socket_in: R,
    conn: Arc<Connection>,
) -> ()
where R: 'static + Send + AsyncRead {
    let task = read_frame(socket_in, conn)
        .and_then(|(socket_in, conn, frame)| {
            {
                let f = &conn.on_frame.0;
                f(conn.clone(), frame);
            }
            reader_continuation(socket_in, conn);
            Ok(())
        })
        .map_err(|err| {
            error!("Read error: {:?}", err);
        });
    tokio::spawn(task);
 }

fn read_preface<R: 'static + Send + AsyncRead>(
    socket_in: R,
    conn: Arc<Connection>,
) -> impl Future<Item = (R, Arc<Connection>), Error = io::Error> {
    let buf = [0u8; 24];
    io::read_exact(socket_in, buf)
        .then(move |result| {
            match result {
                Err(err) => {
                    error!("fail to read HTTP/2 preface: {:?}", err);
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidData,
                        "fail to read HTTP/2 preface"));
                },
                Ok((socket_in, buf)) => {
                    if buf != PREFACE.as_bytes() {
                        error!("HTTP/2 preface mismatch: expect {:?} got {:?}",
                               PREFACE.as_bytes(),
                               buf);
                        return Err(io::Error::new(
                            io::ErrorKind::InvalidData,
                            "HTTP/2 preface mismatch"));
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
    conn: Arc<Connection>,
) -> impl Future<Item = (R, Arc<Connection>), Error = io::Error> {
    read_frame(socket_in, conn)
        .and_then(|(socket_in, conn, frame)| {
            {
                let f = &conn.on_frame.0;
                f(conn.clone(), frame);
            }
            Ok((socket_in, conn))
        })
}

fn read_frame<R: 'static + Send + AsyncRead>(
    socket_in: R,
    conn: Arc<Connection>,
) -> impl Future<Item = (R, Arc<Connection>, Frame), Error = io::Error> {
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

