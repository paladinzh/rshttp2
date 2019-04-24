extern crate tokio;
extern crate futures;
#[macro_use] extern crate log;
extern crate flexi_logger;

use tokio::io;
use tokio::net::TcpListener;
use tokio::prelude::*;
use std::net::SocketAddr;
use flexi_logger::{Logger, with_thread};
use rshttp2::*;

fn listen_on(addr: &SocketAddr) -> impl Future<Item=(), Error=io::Error> {
    let listener = TcpListener::bind(addr).unwrap();
    
    let server = listener.incoming().for_each(|conn| {
        on_connect(conn)
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
