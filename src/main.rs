use clap::Parser;
use ctrlc;
use nix::fcntl::{fcntl, FcntlArg, OFlag, F_GETFL, F_SETFL};
use nix::pty::{openpty, OpenptyResult};
use nix::sys::termios::{cfsetispeed, cfsetospeed, tcgetattr, tcsetattr, BaudRate, ControlFlags, LocalFlags, SetArg};
use std::fs::{remove_file, OpenOptions, File};
use std::io::{self, Read, Write};
use std::os::fd::{AsRawFd, FromRawFd, IntoRawFd};
use std::os::unix::fs::symlink;
use std::path::PathBuf;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc, Mutex,
};
use std::thread;
use std::time::Duration;

/// Программа для создания виртуального последовательного порта (PTY) с расширенным функционалом.
#[derive(Parser, Debug)]
#[command(author, version, about)]
struct Args {
    /// Путь для символической ссылки на виртуальный порт (например, /dev/ttys019 или /tmp/my_virtual_port)
    #[arg(short, long, default_value = "/tmp/my_virtual_port")]
    link: String,

    /// Включить подробное логирование в stdout
    #[arg(short, long, default_value_t = false)]
    verbose: bool,

    /// Не отключать эхо (по умолчанию эхо отключается)
    #[arg(long, default_value_t = false)]
    enable_echo: bool,

    /// Начальное сообщение, которое будет отправлено в виртуальный порт после старта
    #[arg(short, long)]
    init_msg: Option<String>,

    /// Путь к файлу для логирования (если не указан, логирование в файл не производится)
    #[arg(long)]
    log_file: Option<String>,

    /// Интервал heartbeat (в секундах). Если не задано или равно 0, heartbeat не отправляется.
    #[arg(long, default_value_t = 0)]
    heartbeat: u64,

    /// Текст heartbeat-сообщения (по умолчанию: "HEARTBEAT\n")
    #[arg(long, default_value = "HEARTBEAT\n")]
    hb_msg: String,
}

/// Структура для автоматической очистки созданного ресурса (символической ссылки).
struct Cleanup {
    link_path: String,
}

impl Cleanup {
    fn new(link_path: String) -> Self {
        Cleanup { link_path }
    }
}

impl Drop for Cleanup {
    fn drop(&mut self) {
        let _ = remove_file(&self.link_path);
        println!("\n[Cleanup] Removed symbolic link: {}", self.link_path);
    }
}

/// Изменяет скорость передачи данных (baud rate)
fn set_baud_rate(file: &File, baud: BaudRate) {
    let mut termios = tcgetattr(file).expect("Failed to get terminal attributes");
    cfsetispeed(&mut termios, baud).expect("Failed to set input speed");
    cfsetospeed(&mut termios, baud).expect("Failed to set output speed");
    tcsetattr(file, SetArg::TCSANOW, &termios).expect("Failed to set terminal attributes");
}

/// Изменяет бит чётности
fn set_parity(file: &File, parity: &str) {
    let mut termios = tcgetattr(file).expect("Failed to get terminal attributes");
    match parity {
        "none" => termios.control_flags &= !ControlFlags::PARENB,
        "even" => {
            termios.control_flags |= ControlFlags::PARENB;
            termios.control_flags &= !ControlFlags::PARODD;
        }
        "odd" => termios.control_flags |= ControlFlags::PARENB | ControlFlags::PARODD,
        _ => {
            println!("[Error] Invalid parity setting: {}", parity);
            return;
        }
    }
    tcsetattr(file, SetArg::TCSANOW, &termios).expect("Failed to set terminal attributes");
    println!("[Info] Parity set to {}", parity);
}

/// Функция для логирования сообщения (если логгер активен).
fn log_message(logger: &Option<Arc<Mutex<File>>>, msg: &str) {
    if let Some(logger) = logger {
        let mut file = logger.lock().unwrap();
        let _ = writeln!(file, "{}", msg);
    }
}

fn set_nonblocking(fd: i32) {
    let flags = fcntl(fd, F_GETFL).expect("fcntl F_GETFL failed");
    fcntl(fd, F_SETFL(OFlag::from_bits_truncate(flags) | OFlag::O_NONBLOCK))
        .expect("fcntl F_SETFL O_NONBLOCK failed");
}

fn speed_to_baud(speed: u32) -> Option<BaudRate> {
    use BaudRate::*;
    Some(match speed {
        50 => B50,
        75 => B75,
        110 => B110,
        134 => B134,
        150 => B150,
        200 => B200,
        300 => B300,
        600 => B600,
        1200 => B1200,
        1800 => B1800,
        2400 => B2400,
        4800 => B4800,
        9600 => B9600,
        19200 => B19200,
        38400 => B38400,
        57600 => B57600,
        115200 => B115200,
        230400 => B230400,
        _ => return None,
    })
}

fn main() -> io::Result<()> {
    let args = Args::parse();

    if args.verbose {
        println!("[Info] Starting virtual port with arguments: {:?}", args);
    }

    let _cleanup = Cleanup::new(args.link.clone());

    let running = Arc::new(AtomicBool::new(true));
    {
        let running = running.clone();
        ctrlc::set_handler(move || {
            println!("\n[Signal] Ctrl+C received, shutting down...");
            running.store(false, Ordering::SeqCst);
        })
            .expect("Error setting Ctrl-C handler");
    }

    let OpenptyResult { master, slave } = openpty(None, None).expect("[Error] Failed to open PTY");

    // Устанавливаем неблокирующий режим для master и stdin
    set_nonblocking(master.as_raw_fd());
    set_nonblocking(0); // stdin

    let slave_name = get_slave_name(slave.as_raw_fd());
    println!("[Info] Virtual serial port created: {}", slave_name);

    let _ = remove_file(&args.link);
    symlink(&slave_name, &args.link).expect("[Error] Failed to create symbolic link");
    println!("[Info] Custom virtual port available at: {}", args.link);

    let slave_fd = slave.into_raw_fd();
    let slave_file = unsafe { File::from_raw_fd(slave_fd) };

    {
        let mut termios = tcgetattr(&slave_file).expect("[Error] tcgetattr failed");
        if !args.enable_echo {
            termios.local_flags.remove(LocalFlags::ECHO);
            if args.verbose {
                println!("[Info] Disabling local echo on slave device.");
            }
        } else if args.verbose {
            println!("[Info] Echo enabled on slave device.");
        }
        tcsetattr(&slave_file, SetArg::TCSANOW, &termios).expect("[Error] tcsetattr failed");
    }

    let master_fd = master.into_raw_fd();
    let master_file = unsafe { File::from_raw_fd(master_fd) };
    let master_file = Arc::new(master_file);

    if let Some(msg) = &args.init_msg {
        if args.verbose {
            println!("[Info] Sending init message: {}", msg);
        }
        master_file
            .as_ref()
            .write_all(msg.as_bytes())
            .expect("[Error] Failed to write init message");
    }

    let logger: Option<Arc<Mutex<File>>> = args.log_file.as_ref().and_then(|path| {
        OpenOptions::new()
            .append(true)
            .create(true)
            .open(path)
            .map(|file| {
                println!("[Info] Logging communications to file: {}", path);
                Arc::new(Mutex::new(file))
            })
            .map_err(|e| eprintln!("[Error] Cannot open log file {}: {}", path, e))
            .ok()
    });

    if args.heartbeat > 0 {
        let master_for_hb = Arc::clone(&master_file);
        let running_hb = Arc::clone(&running);
        let hb_msg = args.hb_msg.clone();
        let logger_hb = logger.clone();
        thread::spawn(move || {
            while running_hb.load(Ordering::SeqCst) {
                thread::sleep(Duration::from_secs(args.heartbeat));
                if let Err(e) = master_for_hb.as_ref().write_all(hb_msg.as_bytes()) {
                    eprintln!("[Heartbeat] Error sending heartbeat: {}", e);
                    break;
                }
                println!("[Heartbeat] Sent: {}", hb_msg.trim_end());
                log_message(&logger_hb, &format!("[Heartbeat] Sent: {}", hb_msg.trim_end()));
            }
            println!("[Heartbeat] Thread exiting.");
        });
    }

    let master_reader = Arc::clone(&master_file);
    let running_reader = Arc::clone(&running);
    let logger_reader = logger.clone();
    let reader_handle = thread::spawn(move || {
        let mut buf = [0u8; 1024];
        while running_reader.load(Ordering::SeqCst) {
            match master_reader.as_ref().read(&mut buf) {
                Ok(0) => {
                    if cfg!(debug_assertions) {
                        eprintln!("[Reader] Master device closed.");
                    }
                    break;
                }
                Ok(n) => {
                    let text = String::from_utf8_lossy(&buf[..n]);
                    print!("[Received] {}", text);
                    io::stdout().flush().unwrap(); // Принудительный сброс буфера
                    log_message(&logger_reader, &format!("[Received] {}", text.trim_end()));
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
    });

    let master_writer = Arc::clone(&master_file);
    let running_writer = Arc::clone(&running);
    let logger_writer = logger.clone();
    let writer_handle = thread::spawn(move || {
        let stdin = io::stdin();
        let mut stdin = stdin.lock();
        let mut input_buf = [0u8; 1024];
        let mut current_line = String::new();

        while running_writer.load(Ordering::SeqCst) {
            match stdin.read(&mut input_buf) {
                Ok(0) => break, // EOF
                Ok(n) => {
                    let bytes = &input_buf[..n];
                    current_line.push_str(&String::from_utf8_lossy(bytes));

                    // Обработка строк по мере поступления '\n'
                    while let Some(pos) = current_line.find('\n') {
                        let line = current_line.drain(..=pos).collect::<String>();
                        let trimmed = line.trim();

                        if trimmed.starts_with('/') {
                            handle_command(trimmed, &slave_file, &running_writer);
                        } else {
                            if let Err(e) = master_writer.as_ref().write_all(line.as_bytes()) {
                                eprintln!("[Writer] Error writing to master: {}", e);
                                break;
                            }
                            log_message(&logger_writer, &format!("[Sent] {}", trimmed));
                        }
                    }
                }
                Err(e) if e.kind() == io::ErrorKind::WouldBlock => {
                    // Нет данных для чтения, продолжаем цикл
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
    });


    let _ = reader_handle.join();
    let _ = writer_handle.join();

    println!("[Info] Exiting main.");
    Ok(())
}

fn handle_command(cmd: &str, slave_file: &File, running: &AtomicBool) {
    let parts: Vec<&str> = cmd.split_whitespace().collect();
    match parts.as_slice() {
        ["/baud", speed_str] => {
            if let Ok(speed) = speed_str.parse::<u32>() {
                if let Some(baud) = speed_to_baud(speed) {
                    set_baud_rate(slave_file, baud);
                    println!("[Info] Baud rate changed to {}", speed);
                } else {
                    println!("[Error] Unsupported baud rate");
                }
            }
        }
        ["/parity", parity] => set_parity(slave_file, parity),
        ["/quit"] => {
            println!("[Command] Quit command received. Shutting down.");
            running.store(false, Ordering::SeqCst);
        }
        _ => println!("[Command] Unknown command: {}", cmd),
    }
}

fn get_slave_name(fd: i32) -> String {
    let mut path_buf = PathBuf::new();
    let res = fcntl(fd, FcntlArg::F_GETPATH(&mut path_buf));
    match res {
        Ok(_) => path_buf.to_string_lossy().to_string(),
        Err(_) => "unknown".to_string(),
    }
}