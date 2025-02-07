use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use ctrlc;

pub fn setup_signal_handler() -> Arc<AtomicBool> {
    let running = Arc::new(AtomicBool::new(true));
    let running_clone = Arc::clone(&running);
    ctrlc::set_handler(move || {
        println!("\n[Signal] Ctrl+C received, shutting down...");
        running_clone.store(false, Ordering::SeqCst);
    }).expect("Error setting Ctrl-C handler");
    running
}
