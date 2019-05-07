use std::sync::Arc;
use tokio::prelude::*;
use tokio::io;
use tokio::net::TcpStream;
use tokio::sync::mpsc::{channel, Receiver};
use super::*;

pub fn handshake<F>(
    cfg: Config,
    tcp: TcpStream,
    on_frame: F,
) -> Result<Arc<Connection>, super::error::Error>
where F: 'static + Sync + Send + Fn(Arc<Connection>, Frame) -> () {
    tcp.set_nodelay(true).unwrap();
    let (tx, rx) = channel::<Frame>(cfg.sender_queue_size);
    let mut conn = Connection::new(on_frame, tx);
    info!("start to handshake an incoming connection {}", conn.encoded_id());
    Arc::get_mut(&mut conn).unwrap()
        .update_sender_h2_settings(cfg.my_h2_settings);
    let (input, output) = tcp.split();
    start_receive_coroutine(input, conn.clone());
    start_send_coroutine(rx, output, conn.clone());
    Ok(conn)
}

#[derive(Debug)]
pub struct Config {
    pub sender_queue_size: usize,
    pub my_h2_settings: Vec<(SettingKey, u32)>,
}

const PREFACE: &str = "PRI * HTTP/2.0\r\n\r\nSM\r\n\r\n";

fn start_receive_coroutine<R>(
    socket_in: R,
    conn: Arc<Connection>,
) -> ()
where R: 'static + Send + AsyncRead {
    let conn1 = conn.clone();
    let task = read_preface(socket_in, conn)
        .and_then(|(socket_in, conn)| {
            read_settings(socket_in, conn)
        })
        .and_then(|(socket_in, conn)| {
            receive_coroutine_continuation(socket_in, conn);
            Ok(())
        })
        .map_err(move |err| {
            error!(
                "Close connection {} during handshaking because of reading error: {:?}",
                conn1.encoded_id(),
                err);
            let f = GoAwayFrame{
                last_stream_id: conn1.get_last_received_stream_id(),
                error_code: ErrorCode::ConnectError,
                debug_info: vec!()};
            conn1.send_frame(Frame::GoAway(f));
        });
    tokio::spawn(task);
}

fn receive_coroutine_continuation<R>(
    socket_in: R,
    conn: Arc<Connection>,
) -> ()
where R: 'static + Send + AsyncRead {
    if conn.is_closing() {
        return;
    }
    let conn1 = conn.clone();
    let task = read_frame(socket_in, conn)
        .and_then(|(socket_in, conn, frame)| {
            match frame {
                Frame::Settings(ref f) => {
                    if !f.ack {
                        debug!("ack a SETTINGS_FRAME");
                        conn.update_remote_h2_settings(&f.values);
                    }
                },
                Frame::GoAway(ref f) => {
                    info!(
                        "Close connection {} because of receiving GoAway frame: {:?}",
                        conn.encoded_id(),
                        f);
                    let f = GoAwayFrame{
                        last_stream_id: conn.get_last_received_stream_id(),
                        error_code: ErrorCode::NoError,
                        debug_info: vec!()};
                    conn.send_frame(Frame::GoAway(f));
                },
                _ => (),
            }
            Connection::trigger_user_callback(&conn, frame);
            receive_coroutine_continuation(socket_in, conn);
            Ok(())
        })
        .map_err(move |err| {
            error!(
                "Close connection {} because of reading error: {:?}",
                conn1.encoded_id(),
                err);
            let f = GoAwayFrame{
                last_stream_id: conn1.get_last_received_stream_id(),
                error_code: ErrorCode::ConnectError,
                debug_info: vec!()};
            conn1.send_frame(Frame::GoAway(f));
        });
    tokio::spawn(task);
 }

fn read_preface<R: 'static + Send + AsyncRead>(
    socket_in: R,
    conn: Arc<Connection>,
) -> impl Future<Item = (R, Arc<Connection>), Error = Error> {
    let buf = [0u8; 24];
    io::read_exact(socket_in, buf)
        .then(move |result| {
            match result {
                Err(err) => {
                    error!("fail to read HTTP/2 preface: {:?}", err);
                    return Err(Error::new(
                        ErrorLevel::ConnectionLevel,
                        ErrorCode::ProtocolError,
                        "fail to read HTTP/2 preface".to_string()));
                },
                Ok((socket_in, buf)) => {
                    if buf != PREFACE.as_bytes() {
                        error!("HTTP/2 preface mismatch: expect {:?} got {:?}",
                               PREFACE.as_bytes(),
                               buf);
                        return Err(Error::new(
                            ErrorLevel::ConnectionLevel,
                            ErrorCode::ProtocolError,
                            "HTTP/2 preface mismatch".to_string()));
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
) -> impl Future<Item = (R, Arc<Connection>), Error = Error> {
    read_frame(socket_in, conn)
        .and_then(|(socket_in, conn, frame)| {
            match frame {
                Frame::Settings(ref f) => {
                    debug!("ack a SETTINGS_FRAME");
                    conn.update_remote_h2_settings(&f.values);
                },
                _ => {unreachable!()},
            }
            Connection::trigger_user_callback(&conn, frame);
            Ok((socket_in, conn))
        })
}

fn read_frame<R: 'static + Send + AsyncRead>(
    socket_in: R,
    conn: Arc<Connection>,
) -> impl Future<Item = (R, Arc<Connection>, Frame), Error = Error> {
    let buf = [0u8; 9];
    let conn1 = conn.clone();
    io::read_exact(socket_in, buf)
        .map_err(move |err| {
            info!("fail to read connection {} because {:?}",
                  conn1.encoded_id(),
                  err);
            Error::new(
                error::ErrorLevel::ConnectionLevel,
                error::ErrorCode::ConnectError,
                format!("fail to read on connection {}", conn1.encoded_id()))
        })
        .and_then(|(socket_in, buf)| {
            let buf: &[u8] = &buf;
            let frame_header = FrameHeader::parse(buf);
            let mut body = Vec::<u8>::with_capacity(frame_header.body_len);
            body.resize(frame_header.body_len, 0);
            let conn1 = conn.clone();
            io::read_exact(socket_in, body)
                .map_err(move |err| {
                    info!("fail to read connection {} because {:?}",
                          conn1.encoded_id(),
                          err);
                    Error::new(
                        error::ErrorLevel::ConnectionLevel,
                        error::ErrorCode::ConnectError,
                        format!("fail to read on connection {}", conn1.encoded_id()))
                })
                .and_then(move |(socket_in, body)| {
                    debug!("succeed to read payload of a frame with {} bytes", body.len());
                    let frame = Frame::parse(&frame_header, body);
                    match frame {
                        Ok(f) => Ok((socket_in, conn, f)),
                        Err(err) => Err(err),
                    }
                })
        })
}

fn start_send_coroutine<W>(
    rx: Receiver<Frame>,
    socket_out: W,
    conn: Arc<Connection>,
) -> ()
where W: 'static + Send + AsyncWrite {
    if conn.is_closing() {
        return;
    }
    let conn1 = conn.clone();
    let task = rx.into_future()
        .and_then(move |(frame, rx)| {
            if conn.is_closing() {
                return Ok(());
            }
            match frame {
                None => (),
                Some(frame) => {
                    match frame {
                        Frame::GoAway(_) => {
                            conn.async_disconnect();
                        },
                        _ => (),
                    };
                    debug!("dump a frame {:?}", frame);
                    let buf = frame.serialize();
                    let conn2 = conn.clone();
                    let task = io::write_all(socket_out, buf)
                        .and_then(|(socket_out, _buf)| {
                            start_send_coroutine(rx, socket_out, conn);
                            Ok(())
                        })
                        .map_err(move |err| {
                            info!(
                                "Close connection {} because of writing error: {:?}",
                                conn2.encoded_id(),
                                err);
                            conn2.async_disconnect();
                        });
                    tokio::spawn(task);
                }
            };
            Ok(())
        })
        .map_err(move |err| {
            info!(
                "Close connection {} because of writing error: {:?}",
                conn1.encoded_id(),
                err);
            conn1.async_disconnect();
        });
    tokio::spawn(task);
}

