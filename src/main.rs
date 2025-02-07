mod cli;
mod commands;
mod cleanup;
mod logger;
mod pty;
mod signal_handler;
mod heartbeat;
mod io_handler;

use clap::Parser;
use std::fs::{remove_file, OpenOptions, File};
use std::io::{self, Write};
use std::os::fd::{AsRawFd, FromRawFd, IntoRawFd};
#[cfg(unix)]
use std::os::unix::fs::symlink;
use std::sync::{Arc, Mutex};
use nix::sys::termios::{tcgetattr, tcsetattr, LocalFlags, SetArg};

use cli::Args;
use commands::load_commands_from_file;
use cleanup::Cleanup;
use pty::{create_virtual_serial_port, get_slave_name, set_baud_rate, set_nonblocking, set_parity, speed_to_baud};
use signal_handler::setup_signal_handler;
use heartbeat::start_heartbeat;
use io_handler::{start_reader, start_writer};

fn main() -> io::Result<()> {
    // Разбор аргументов командной строки
    let args = Args::parse();
    if args.verbose {
        println!("[Info] Starting virtual port with arguments: {:?}", args);
    }

    // Загрузка команд из файла
    let commands = load_commands_from_file("commands.txt");
    if !commands.is_empty() {
        println!("[Info] Loaded {} command(s).", commands.len());
    }

    // Автоматическая очистка символической ссылки при завершении работы
    let _cleanup = Cleanup::new(args.link.clone());

    // Настройка обработчика сигналов
    let running = setup_signal_handler();

    // Создание виртуального последовательного порта
    let pty_result = create_virtual_serial_port(5);
    let (master, slave) = (pty_result.master, pty_result.slave);

    // Установка неблокирующего режима для master и stdin
    set_nonblocking(master.as_raw_fd());
    set_nonblocking(0); // stdin

    // Получение имени slave-устройства и создание символической ссылки
    let slave_name = get_slave_name(slave.as_raw_fd());
    println!("[Info] Virtual serial port created: {}", slave_name);
    let _ = remove_file(&args.link);
    symlink(&slave_name, &args.link).expect("[Error] Failed to create symbolic link");
    println!("[Info] Custom virtual port available at: {}", args.link);

    // Настройка slave-устройства (отключение эха, если требуется)
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

    // Настройка скорости и паритета
    let baud_rate = match args.baud_rate.parse::<u32>() {
        Ok(baud) => match speed_to_baud(baud) {
            Some(br) => br,
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

    // Оборачивание master-устройства в Arc для потокобезопасного доступа
    let master_fd = master.into_raw_fd();
    let master_file = unsafe { File::from_raw_fd(master_fd) };
    let mut master_file = Arc::new(master_file);

    // Отправка начального сообщения, если задано
    if let Some(msg) = &args.init_msg {
        if args.verbose {
            println!("[Info] Sending init message: {}", msg);
        }
        master_file.write_all(msg.as_bytes()).expect("[Error] Failed to write init message");
    }

    // Инициализация логгера, если задан файл для логирования
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

    // Запуск heartbeat-потока, если задан интервал
    if args.heartbeat > 0 {
        start_heartbeat(
            running.clone(),
            args.heartbeat,
            args.hb_msg.clone(),
            Arc::clone(&master_file),
            logger.clone(),
        );
    }

    // Запуск потоков для чтения и записи
    let reader_handle = start_reader(running.clone(), Arc::clone(&master_file), logger.clone(), commands.clone());
    let writer_handle = start_writer(running.clone(), Arc::clone(&master_file), logger.clone(), commands.clone());

    let _ = reader_handle.join();
    let _ = writer_handle.join();

    println!("[Info] Exiting main.");
    Ok(())
}
