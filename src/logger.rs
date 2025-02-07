use std::fs::File;
use std::io::Write;
use std::sync::{Arc, Mutex};

/// Функция для логирования сообщения (если логгер активен).
pub fn log_message(logger: &Option<Arc<Mutex<File>>>, msg: &str) {
    if let Some(logger) = logger {
        if let Ok(mut file) = logger.lock() {
            let _ = writeln!(file, "{}", msg);
        }
    }
}
