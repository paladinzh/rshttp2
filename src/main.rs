extern crate tokio;
extern crate futures;
#[macro_use] extern crate log;
extern crate flexi_logger;

use tokio::net::TcpListener;
use tokio::prelude::*;
use std::net::SocketAddr;
use flexi_logger::{Logger, with_thread};
use rshttp2::*;

fn listen_on(addr: &SocketAddr) -> impl Future<Item=(), Error=()> {
    let listener = TcpListener::bind(addr).unwrap();
    listener.incoming()
        .for_each(|conn| {
            let cfg = Config{
                sender_queue_size: 100,
                my_h2_settings: vec!((SettingKey::MaxConcurrentStreams, 123)),
            };
            let _ = handshake(cfg, conn, |conn, frame| {
                info!("got a frame: {:?}", frame);
                match frame {
                    Frame::Headers(ref f) if f.end_stream && f.end_headers => {
                        info!("responding");
                        let builder = SendHeadersFrameBuilder::new()
                            .set_stream_id(1)
                            .append_header_field(EncoderField::ToCache((
                                AnySliceable::new(b":status".to_vec()),
                                AnySliceable::new(b"200".to_vec()),
                            )))
                            .set_end_headers()
                            .set_end_stream();
                        // let builder = SendHeadersFrameBuilder::new()
                        //     .set_stream_id(2)
                        //     .append_header_field(EncoderField::ToCache((
                        //         AnySliceable::new(b":methd".to_vec()),
                        //         AnySliceable::new(b"GET".to_vec()),
                        //     )))
                        //     .append_header_field(EncoderField::ToCache((
                        //         AnySliceable::new(b":path".to_vec()),
                        //         AnySliceable::new(b"/".to_vec()),
                        //     )))
                        //     .set_end_headers()
                        //     .set_end_stream();
                        conn.send_frame(SendFrame::Headers(SendHeadersFrame::new(builder)));
                    },
                    _ => (),
                };
            });
            Ok(())
        })
        .map_err(|err| {
            error!("accept error = {:?}", err);
        })
}

fn main() {
    Logger::with_env()
        .format(with_thread)
        .start()
        .unwrap();
    let addr = "127.0.0.1:2333".parse().unwrap();
    let server = listen_on(&addr);
    debug!("server running on localhost:2333");
    tokio::run(server);
}
