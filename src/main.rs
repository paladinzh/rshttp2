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
            let _ = handshake(cfg, conn, |_conn, frame| {
                info!("got a frame: {:?}", frame);
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
