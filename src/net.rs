use std::sync::{Arc, Mutex};
use std::sync::atomic::{Ordering, AtomicBool, AtomicU32};
use std::time::{Duration, Instant};
use tokio::prelude::*;
use tokio::io;
use tokio::net::TcpStream;
use tokio::sync::mpsc::{channel, Receiver, Sender};
use random::Source;
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
    info!("start to handshake an incoming connection {}", base62::encode(conn.id));
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

pub struct Connection {
    id: u64,
    on_frame: FnBox,
    sender: Sender<Frame>,
    my_h2_settings: Mutex<Settings>,
    remote_h2_settings: Mutex<Settings>,
    to_close: AtomicBool,
    last_received_stream_id: AtomicU32,
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
    fn new<F>(on_frame: F, sender: Sender<Frame>) -> Arc<Connection>
    where F: 'static + Sync + Send + Fn(Arc<Connection>, Frame) -> () {
        Arc::new(Connection{
            id: random::default().read_u64(),
            on_frame: FnBox::new(on_frame),
            sender,
            my_h2_settings: Mutex::new(Settings::new()),
            remote_h2_settings: Mutex::new(Settings::new()),
            to_close: AtomicBool::new(false),
            last_received_stream_id: AtomicU32::new(0)})
    }

    pub fn update_sender_h2_settings(
        &mut self,
        new_values: Vec<(SettingKey, u32)>,
    ) -> () {
        {
            let whole: &mut Settings = &mut self.my_h2_settings.lock().unwrap();
            for (key, val) in &new_values {
                whole.set(key.clone(), *val);
            }
        }
        let f = Frame::Settings(SettingsFrame::new(false, new_values));
        send_frame(self.sender.clone(), f);
    }

    pub fn disconnect(&mut self) {
        
    }
}

fn send_frame(mut q: Sender<Frame>, f: Frame) {
    let res = q.try_send(f);
    match res {
        Ok(_) => (),
        Err(err) => {
            let f = err.into_inner();
            let mut rng = random::default();
            let delay = Duration::from_millis(rng.read_u64() % 30);
            let wakeup = Instant::now() + delay;
            let task = tokio::timer::Delay::new(wakeup)
                .map_err(|e| panic!("timer failed; err={:?}", e))
                .and_then(move |_| {
                    send_frame(q, f);
                    Ok(())
                });
            tokio::spawn(task);
        }
    }
}

const PREFACE: &str = "PRI * HTTP/2.0\r\n\r\nSM\r\n\r\n";

fn start_receive_coroutine<R>(
    socket_in: R,
    conn: Arc<Connection>,
) -> ()
where R: 'static + Send + AsyncRead {
    let task = read_preface(socket_in, conn)
        .and_then(|(socket_in, conn)| {
            read_settings(socket_in, conn)
        })
        .and_then(|(socket_in, conn)| {
            receive_coroutine_continuation(socket_in, conn);
            Ok(())
        })
        .map_err(|err| {
            error!("Read error: {:?}", err);
        });
    tokio::spawn(task);
}

fn receive_coroutine_continuation<R>(
    socket_in: R,
    conn: Arc<Connection>,
) -> ()
where R: 'static + Send + AsyncRead {
    if conn.to_close.load(Ordering::Acquire) {
        return;
    }
    let conn1 = conn.clone();
    let task = read_frame(socket_in, conn)
        .and_then(|(socket_in, conn, frame)| {
            match frame {
                Frame::Settings(ref f) => {
                    if !f.ack {
                        debug!("ack a SETTINGS_FRAME");
                        let whole: &mut Settings = &mut conn.remote_h2_settings.lock().unwrap();
                        for (key, val) in &f.values {
                            whole.set(key.clone(), *val);
                        }
                        send_frame(conn.sender.clone(), Frame::Settings(SettingsFrame::new(true, vec!())));
                    }
                },
                Frame::GoAway(ref f) => {
                    info!(
                        "Close connection {} because of receiving GoAway frame: {:?}",
                        base62::encode(conn.id),
                        f);
                    let f = GoAwayFrame{
                        last_stream_id: conn.last_received_stream_id.load(Ordering::Acquire),
                        error_code: ErrorCode::NoError,
                        debug_info: vec!()};
                    send_frame(conn.sender.clone(), Frame::GoAway(f));
                },
                _ => (),
            }
            {
                let f = &conn.on_frame.0;
                f(conn.clone(), frame);
            }
            receive_coroutine_continuation(socket_in, conn);
            Ok(())
        })
        .map_err(move |err| {
            error!(
                "Close connection {} because of reading error: {:?}",
                base62::encode(conn1.id),
                err);
            let f = GoAwayFrame{
                last_stream_id: conn1.last_received_stream_id.load(Ordering::Acquire),
                error_code: ErrorCode::ConnectError,
                debug_info: vec!()};
            send_frame(conn1.sender.clone(), Frame::GoAway(f));
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
                    let whole: &mut Settings = &mut conn.remote_h2_settings.lock().unwrap();
                    for (key, val) in &f.values {
                        whole.set(key.clone(), *val);
                    }
                    send_frame(conn.sender.clone(), Frame::Settings(SettingsFrame::new(true, vec!())));
                },
                _ => {unreachable!()},
            }
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
) -> impl Future<Item = (R, Arc<Connection>, Frame), Error = Error> {
    let buf = [0u8; 9];
    let conn1 = conn.clone();
    io::read_exact(socket_in, buf)
        .map_err(move |err| {
            info!("fail to read connection {} because {:?}",
                  base62::encode(conn1.id),
                  err);
            Error::new(
                error::ErrorLevel::ConnectionLevel,
                error::ErrorCode::ConnectError,
                format!("fail to read on connection {}", base62::encode(conn1.id)))
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
                          base62::encode(conn1.id),
                          err);
                    Error::new(
                        error::ErrorLevel::ConnectionLevel,
                        error::ErrorCode::ConnectError,
                        format!("fail to read on connection {}", conn1.id))
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
    if conn.to_close.load(Ordering::Acquire) {
        return;
    }
    let conn1 = conn.clone();
    let task = rx.into_future()
        .and_then(move |(frame, rx)| {
            if conn.to_close.load(Ordering::Acquire) {
                return Ok(());
            }
            match frame {
                None => (),
                Some(frame) => {
                    match frame {
                        Frame::GoAway(_) => {
                            conn.to_close.store(true, Ordering::Release);
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
                                base62::encode(conn2.id),
                                err);
                            conn2.to_close.store(true, Ordering::Release);
                        });
                    tokio::spawn(task);
                }
            };
            Ok(())
        })
        .map_err(move |err| {
            info!(
                "Close connection {} because of writing error: {:?}",
                base62::encode(conn1.id),
                err);
            conn1.to_close.store(true, Ordering::Release);
        });
    tokio::spawn(task);
}

