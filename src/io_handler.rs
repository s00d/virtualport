use std::sync::{Arc, Mutex};
use std::fs::File;
use std::io::{self, Read, Write};
use std::thread;
use std::time::Duration;
use std::collections::HashMap;
use crate::logger::log_message;

pub fn start_reader(
    running: Arc<std::sync::atomic::AtomicBool>,
    master: Arc<File>,
    logger: Option<Arc<Mutex<File>>>,
    commands: HashMap<String, String>,
) -> thread::JoinHandle<()> {
    thread::spawn(move || {
        let mut buf = [0u8; 1024];
        let mut received_data = String::new();
        while running.load(std::sync::atomic::Ordering::SeqCst) {
            match master.as_ref().read(&mut buf) {
                Ok(0) => {
                    if cfg!(debug_assertions) {
                        eprintln!("[Reader] Master device closed.");
                    }
                    break;
                }
                Ok(n) => {
                    let text = String::from_utf8_lossy(&buf[..n]);
                    received_data.push_str(&text);
                    print!("[Received] {}", text);
                    io::stdout().flush().unwrap();
                    log_message(&logger, &format!("[Received] {}", text.trim_end()));

                    while let Some(pos) = received_data.find('\n') {
                        let command = received_data.drain(..=pos).collect::<String>().trim().to_string();
                        if let Some(response) = commands.get(&command) {
                            println!("[Command] Recognized: '{}', responding with '{}'", command, response);
                            if let Err(e) = master.as_ref().write_all(format!("{}\n", response).as_bytes()) {
                                eprintln!("[Reader] Error writing response: {}", e);
                            }
                            log_message(&logger, &format!("[Response] {}", response.trim_end()));
                        }
                    }
                    master.as_ref().flush().expect("[Reader] Failed to flush response");
                }
                Err(e) if e.kind() == io::ErrorKind::WouldBlock => {
                    thread::sleep(Duration::from_millis(10));
                    continue;
                }
                Err(e) => {
                    eprintln!("[Reader] Error reading from master: {}", e);
                    break;
                }
            }
        }
        println!("[Reader] Thread exiting.");
    })
}

pub fn start_writer(
    running: Arc<std::sync::atomic::AtomicBool>,
    master: Arc<File>,
    logger: Option<Arc<Mutex<File>>>,
    commands: HashMap<String, String>,
) -> thread::JoinHandle<()> {
    thread::spawn(move || {
        let stdin = io::stdin();
        let mut stdin_lock = stdin.lock();
        let mut input_buf = [0u8; 1024];
        let mut current_line = String::new();

        while running.load(std::sync::atomic::Ordering::SeqCst) {
            match stdin_lock.read(&mut input_buf) {
                Ok(0) => break, // EOF
                Ok(n) => {
                    let bytes = &input_buf[..n];
                    current_line.push_str(&String::from_utf8_lossy(bytes));

                    while let Some(pos) = current_line.find('\n') {
                        let line = current_line.drain(..=pos).collect::<String>();
                        let trimmed = line.trim().to_string();
                        let response = if let Some(resp) = commands.get(&trimmed) {
                            println!("[Command] Recognized: '{}', responding with '{}'", trimmed, resp);
                            format!("{}\n", resp)
                        } else {
                            line
                        };

                        if let Err(e) = master.as_ref().write_all(response.as_bytes()) {
                            eprintln!("[Writer] Error writing to master: {}", e);
                            break;
                        }
                        log_message(&logger, &format!("[Sent] {}", response.trim_end()));
                    }
                }
                Err(e) if e.kind() == io::ErrorKind::WouldBlock => {
                    thread::sleep(Duration::from_millis(10));
                    continue;
                }
                Err(e) => {
                    eprintln!("[Writer] Error reading from stdin: {}", e);
                    break;
                }
            }
        }
        println!("[Writer] Thread exiting.");
    })
}
