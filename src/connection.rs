use std::sync::{Arc, Mutex};
use std::sync::atomic::{Ordering, AtomicBool, AtomicU32};
use std::time::{Duration, Instant};
use tokio::prelude::*;
use tokio::sync::mpsc::Sender;
use random::Source;
use super::*;

pub struct Connection {
    id: u64,
    on_frame: FnBox,
    sender: Sender<Frame>,
    my_h2_settings: Mutex<Settings>,
    remote_h2_settings: Mutex<Settings>,
    to_close: AtomicBool,
    last_received_stream_id: AtomicU32,
    pub header_decoder: Mutex<hpack::Decoder>,
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
    pub fn new<F>(on_frame: F, sender: Sender<Frame>) -> Arc<Connection>
    where F: 'static + Sync + Send + Fn(Arc<Connection>, Frame) -> () {
        Arc::new(Connection{
            id: random::default().read_u64(),
            on_frame: FnBox::new(on_frame),
            sender,
            my_h2_settings: Mutex::new(Settings::new()),
            remote_h2_settings: Mutex::new(Settings::new()),
            to_close: AtomicBool::new(false),
            last_received_stream_id: AtomicU32::new(0),
            header_decoder: Mutex::new(
                hpack::Decoder::with_capacity(
                    Settings::new().get(SettingKey::HeaderTableSize) as usize)),
        })
    }

    pub fn encoded_id(&self) -> String {
        base62::encode(self.id)
    }

    pub fn update_sender_h2_settings(
        &self,
        new_values: Vec<(SettingKey, u32)>,
    ) -> () {
        {
            let whole: &mut Settings = &mut self.my_h2_settings.lock().unwrap();
            for (key, val) in &new_values {
                whole.set(key.clone(), *val);
            }
        }
        let f = Frame::Settings(SettingsFrame::new(false, new_values));
        self.send_frame(f);
    }

    pub fn update_remote_h2_settings(
        &self,
        new_values: &Vec<(SettingKey, u32)>,
    ) -> () {
        let whole: &mut Settings = &mut self.remote_h2_settings.lock().unwrap();
        for (key, val) in new_values {
            whole.set(key.clone(), *val);
        }
        // send back ack frame
        self.send_frame(Frame::Settings(SettingsFrame::new(true, vec!())));
    }

    pub fn trigger_user_callback(conn: &Arc<Connection>, frame: Frame) -> () {
        let f = &conn.on_frame.0;
        f(conn.clone(), frame);
    }
    
    pub fn send_frame(&self, f: Frame) {
        let q = self.sender.clone();
        inner_send_frame(q, f);
    }

    pub fn get_last_received_stream_id(&self) -> u32 {
        self.last_received_stream_id.load(Ordering::Acquire)
    }

    pub fn is_closing(&self) -> bool {
        self.to_close.load(Ordering::Acquire)
    }

    pub fn async_disconnect(&self) {
        self.to_close.store(true, Ordering::Release);
    }
}

fn inner_send_frame(mut q: Sender<Frame>, f: Frame) {
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
                    inner_send_frame(q, f);
                    Ok(())
                });
            tokio::spawn(task);
        }
    }
}

