use std::collections::HashMap;
use std::ffi::CStr;
use clap::Parser;
use ctrlc;
use nix::fcntl::{fcntl, OFlag, F_GETFL, F_SETFL};
use nix::pty::{OpenptyResult};
use nix::sys::termios::{cfsetispeed, cfsetospeed, tcgetattr, tcsetattr, BaudRate, ControlFlags, LocalFlags, SetArg};
use std::fs::{remove_file, OpenOptions, File};
use std::io::{self, BufRead, Read, Write};
use std::os::fd::{AsRawFd, FromRawFd, IntoRawFd};
use std::os::unix::fs::symlink;
use std::path::Path;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc, Mutex,
};
use std::thread;
use std::time::Duration;
use libc::ttyname;

#[derive(Parser, Debug)]
#[command(
    author = "s00d <Virus191288@gmail.com>",
    version = env!("CARGO_PKG_VERSION"),
    about = "A program to create a virtual serial port (PTY) with extended functionality.",
    long_about = "This program creates a virtual serial port using pseudoterminal (PTY) with configurable baud rate, parity, and logging options. It also supports sending heartbeat messages and managing serial communication through the command-line interface."
)]
struct Args {
    /// Path for the symbolic link to the virtual port
    #[arg(short = 'l', long, default_value = "/tmp/my_virtual_port", help = "Specify the symbolic link path for the virtual serial port.")]
    link: String,

    /// Enable verbose logging to stdout
    #[arg(short = 'v', long, default_value_t = false, help = "Enable verbose logging for additional information in the terminal.")]
    verbose: bool,

    /// Do not disable echo (echo is disabled by default)
    #[arg(short = 'e', long, default_value_t = false, help = "If enabled, the echo feature will remain active on the slave device.")]
    enable_echo: bool,

    /// Initial message that will be sent to the virtual port upon startup
    #[arg(short = 'i', long, help = "Provide an initial message to be sent to the virtual port after startup.")]
    init_msg: Option<String>,

    /// Path to a file for logging communication
    #[arg(short = 'f', long, help = "Specify a path for logging communication to a file.")]
    log_file: Option<String>,

    /// Heartbeat interval in seconds
    #[arg(short = 'b', long, default_value_t = 0, help = "Set the interval (in seconds) for sending heartbeat messages.")]
    heartbeat: u64,

    /// Text for the heartbeat message
    #[arg(short = 'm', long, default_value = "HEARTBEAT\n", help = "Set the text for the heartbeat message.")]
    hb_msg: String,

    /// Set the baud rate for the virtual serial port
    #[arg(short = 'r', long, default_value = "9600", help = "Set the baud rate for the virtual serial port.")]
    baud_rate: String,

    /// Set the parity for the serial connection (none, even, odd)
    #[arg(short = 'p', long, default_value = "none", value_parser = ["none", "even", "odd"], help = "Set the parity for the virtual serial port")]
    parity: String,
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

/// Загружает команды из файла, где каждая команда и её ответ задаются в соседних строках.
fn load_commands_from_file(filename: &str) -> HashMap<String, String> {
    let mut commands = HashMap::new();
    let path = Path::new(filename);

    if !path.exists() {
        eprintln!("[Warning] Command file '{}' not found. Running without predefined commands.", filename);
        return commands;
    }

    let file = File::open(filename).expect("Failed to open commands file");
    let reader = io::BufReader::new(file);
    let mut lines = reader.lines();

    while let Some(Ok(command)) = lines.next() {
        if let Some(Ok(response)) = lines.next() {
            commands.insert(command, response);
        }
    }

    commands
}

/// --- Реализация создания виртуального порта --- ///

#[cfg(not(target_os = "android"))]
fn create_virtual_serial_port(retries: usize) -> nix::pty::OpenptyResult {
    // Для платформ, отличных от Android, используем стандартную реализацию.
    for attempt in 0..retries {
        match nix::pty::openpty(None, None) {
            Ok(pty) => return pty,
            Err(e) => {
                eprintln!("[Error] Failed to create PTY (attempt {}/{}): {}", attempt + 1, retries, e);
                thread::sleep(Duration::from_millis(500));
            }
        }
    }
    panic!("[Error] Unable to create a virtual serial port after {} attempts", retries);
}

#[cfg(target_os = "android")]
mod android_pty {
    use nix::pty::{OpenptyResult, posix_openpt, PtyMaster, grantpt, unlockpt, ptsname};
    use nix::fcntl::OFlag;
    use nix::sys::stat::Mode;
    use std::fs::File;
    use std::os::unix::io::FromRawFd;
    use std::path::Path;
    use std::os::fd::AsRawFd;

    pub fn openpty_android() -> nix::Result<OpenptyResult> {
        // Открываем мастер-устройство PTY с нужными флагами.
        let master_fd: PtyMaster = posix_openpt(OFlag::O_RDWR | OFlag::O_NOCTTY)?;
        // Вызываем grantpt/unlockpt с заимствованием мастера.
        grantpt(&master_fd)?;
        unlockpt(&master_fd)?;
        // Получаем имя slave-устройства.
        let slave_name = unsafe { ptsname(&master_fd)? };

        // Если ptsname возвращает Cow<str>, можно явно указать тип:
        let slave_name_str: &str = slave_name.as_ref();
        // Если требуется использовать Path:
        let slave_path = Path::new(slave_name_str);
        let slave_fd = nix::fcntl::open(
            slave_path,
            OFlag::O_RDWR | OFlag::O_NOCTTY,
            Mode::empty()
        )?;
        Ok(OpenptyResult {
            master: unsafe { File::from_raw_fd(master_fd.as_raw_fd()).into() },
            slave: unsafe { File::from_raw_fd(slave_fd).into() },
        })
    }
}


#[cfg(target_os = "android")]
fn create_virtual_serial_port(retries: usize) -> nix::pty::OpenptyResult {
    // Для Android используем собственную реализацию.
    for attempt in 0..retries {
        match android_pty::openpty_android() {
            Ok(pty) => return pty,
            Err(e) => {
                eprintln!("[Error] Failed to create PTY on Android (attempt {}/{}): {}", attempt + 1, retries, e);
                thread::sleep(Duration::from_millis(500));
            }
        }
    }
    panic!("[Error] Unable to create a virtual serial port after {} attempts", retries);
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

/// Устанавливает неблокирующий режим для файлового дескриптора.
fn set_nonblocking(fd: i32) {
    let flags = fcntl(fd, F_GETFL).expect("fcntl F_GETFL failed");
    fcntl(fd, F_SETFL(OFlag::from_bits_truncate(flags) | OFlag::O_NONBLOCK))
        .expect("fcntl F_SETFL O_NONBLOCK failed");
}

/// Преобразует числовую скорость в BaudRate.
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

    let commands = load_commands_from_file("commands.txt");

    if !commands.is_empty() {
        println!("[Info] Loaded {} command(s).", commands.len());
    }

    // Автоматически удаляем символическую ссылку при завершении программы.
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

    // Создаём виртуальный последовательный порт.
    let OpenptyResult { master, slave } = create_virtual_serial_port(5);

    // Устанавливаем неблокирующий режим для master и stdin.
    set_nonblocking(master.as_raw_fd());
    set_nonblocking(0); // stdin

    // Получаем имя slave-устройства.
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

    let baud_rate = match args.baud_rate.parse::<u32>() {
        Ok(baud) => match speed_to_baud(baud) {
            Some(baud_rate) => baud_rate,
            None => {
                eprintln!("[Error] Unsupported baud rate: {}", args.baud_rate);
                return Ok(());
            }
        },
        Err(_) => {
            eprintln!("[Error] Invalid baud rate: {}", args.baud_rate);
            return Ok(());
        }
    };

    set_baud_rate(&slave_file, baud_rate);
    set_parity(&slave_file, &args.parity);

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
    let commands_reader = commands.clone();
    let reader_handle = thread::spawn(move || {
        let mut buf = [0u8; 1024];
        let mut received_data = String::new();
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
                    received_data.push_str(&text);
                    print!("[Received] {}", text);
                    io::stdout().flush().unwrap();
                    log_message(&logger_reader, &format!("[Received] {}", text.trim_end()));

                    // Проверяем, есть ли в буфере полная команда (до '\n')
                    while let Some(pos) = received_data.find('\n') {
                        let command = received_data.drain(..=pos).collect::<String>().trim().to_string();
                        if let Some(response) = commands_reader.get(&command) {
                            println!("[Command] Recognized: '{}', responding with '{}'", command, response);
                            master_reader
                                .as_ref()
                                .write_all(format!("{}\n", response).as_bytes())
                                .expect("[Error] Failed to write response");
                            log_message(&logger_reader, &format!("[Response] {}", response.trim_end()));
                        }
                    }
                    master_reader
                        .as_ref()
                        .flush()
                        .expect("[Error] Failed to flush response");
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
    let commands_writer = commands.clone();
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
                        let trimmed = line.trim().to_string();

                        let response = if let Some(resp) = commands_writer.get(&trimmed) {
                            println!("[Command] Recognized: '{}', responding with '{}'", trimmed, resp);
                            resp.clone() + "\n"
                        } else {
                            line
                        };

                        if let Err(e) = master_writer.as_ref().write_all(response.as_bytes()) {
                            eprintln!("[Writer] Error writing to master: {}", e);
                            break;
                        }
                        log_message(&logger_writer, &format!("[Sent] {}", response.trim_end()));
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
    });

    let _ = reader_handle.join();
    let _ = writer_handle.join();

    println!("[Info] Exiting main.");
    Ok(())
}

/// Возвращает имя slave-устройства для заданного файлового дескриптора.
fn get_slave_name(fd: i32) -> String {
    let ret = unsafe { ttyname(fd) };

    if ret.is_null() {
        "unknown".to_string()
    } else {
        let path = unsafe { CStr::from_ptr(ret).to_string_lossy() };
        path.to_string()
    }
}
