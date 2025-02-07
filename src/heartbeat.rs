use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
    Mutex,
};
use std::fs::File;
use std::io::Write;
use std::thread;
use std::time::Duration;
use crate::logger::log_message;

pub fn start_heartbeat(
    running: Arc<AtomicBool>,
    heartbeat_interval: u64,
    hb_msg: String,
    mut master: Arc<File>,
    logger: Option<Arc<Mutex<File>>>,
) {
    thread::spawn(move || {
        while running.load(Ordering::SeqCst) {
            thread::sleep(Duration::from_secs(heartbeat_interval));
            if let Err(e) = master.write_all(hb_msg.as_bytes()) {
                eprintln!("[Heartbeat] Error sending heartbeat: {}", e);
                break;
            }
            println!("[Heartbeat] Sent: {}", hb_msg.trim_end());
            log_message(&logger, &format!("[Heartbeat] Sent: {}", hb_msg.trim_end()));
        }
        println!("[Heartbeat] Thread exiting.");
    });
}
