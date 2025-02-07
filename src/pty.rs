use std::fs::File;
#[cfg(target_os = "android")]
use std::os::fd::{AsRawFd, FromRawFd};
#[cfg(not(target_os = "android"))]
use nix::pty::{openpty};
#[cfg(unix)]
use nix::pty::{OpenptyResult};
#[cfg(unix)]
use nix::sys::termios::{tcgetattr, tcsetattr, cfsetispeed, cfsetospeed, BaudRate, ControlFlags, SetArg};
#[cfg(unix)]
use std::os::unix::io::RawFd;
#[cfg(unix)]
use std::thread;
#[cfg(unix)]
use std::time::Duration;

#[cfg(unix)]
/// Создаёт виртуальный последовательный порт с указанным количеством попыток.
pub fn create_virtual_serial_port(retries: usize) -> OpenptyResult {
    #[cfg(not(target_os = "android"))]
    {
        for attempt in 0..retries {
            match openpty(None, None) {
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
    {
        for attempt in 0..retries {
            match openpty_android() {
                Ok(pty) => return pty,
                Err(e) => {
                    eprintln!("[Error] Failed to create PTY on Android (attempt {}/{}): {}", attempt + 1, retries, e);
                    thread::sleep(Duration::from_millis(500));
                }
            }
        }
        panic!("[Error] Unable to create a virtual serial port after {} attempts", retries);
    }
}

#[cfg(target_os = "android")]
fn openpty_android() -> nix::Result<OpenptyResult> {
    use nix::pty::{posix_openpt, PtyMaster};
    use nix::fcntl::OFlag;
    use nix::sys::stat::Mode;
    use nix::pty::{grantpt, unlockpt, ptsname};
    use std::path::Path;
    let master_fd: PtyMaster = posix_openpt(OFlag::O_RDWR | OFlag::O_NOCTTY)?;
    grantpt(&master_fd)?;
    unlockpt(&master_fd)?;
    let slave_name = unsafe { ptsname(&master_fd)? };
    let slave_name_str: &str = slave_name.as_ref();
    let slave_path = Path::new(slave_name_str);
    let slave_fd = nix::fcntl::open(slave_path, OFlag::O_RDWR | OFlag::O_NOCTTY, Mode::empty())?;
    Ok(OpenptyResult {
        master: unsafe { File::from_raw_fd(master_fd.as_raw_fd()).into() },
        slave: unsafe { File::from_raw_fd(slave_fd).into() },
    })
}

#[cfg(unix)]
/// Устанавливает заданную скорость (baud rate) для терминала.
pub fn set_baud_rate(file: &File, baud: BaudRate) {
    let mut termios = tcgetattr(file).expect("Failed to get terminal attributes");
    cfsetispeed(&mut termios, baud).expect("Failed to set input speed");
    cfsetospeed(&mut termios, baud).expect("Failed to set output speed");
    tcsetattr(file, SetArg::TCSANOW, &termios).expect("Failed to set terminal attributes");
}

#[cfg(unix)]
/// Устанавливает заданный паритет для терминала.
pub fn set_parity(file: &File, parity: &str) {
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

#[cfg(unix)]
/// Переводит файловый дескриптор в неблокирующий режим.
pub fn set_nonblocking(fd: RawFd) {
    use nix::fcntl::{fcntl, F_GETFL, F_SETFL, OFlag};
    let flags = fcntl(fd, F_GETFL).expect("fcntl F_GETFL failed");
    let new_flags = OFlag::from_bits_truncate(flags) | OFlag::O_NONBLOCK;
    fcntl(fd, F_SETFL(new_flags)).expect("fcntl F_SETFL O_NONBLOCK failed");
}

#[cfg(unix)]
/// Получает имя slave-устройства по файловому дескриптору.
pub fn get_slave_name(fd: RawFd) -> String {
    use std::ffi::CStr;
    use libc::ttyname;
    let ret = unsafe { ttyname(fd) };
    if ret.is_null() {
        "unknown".to_string()
    } else {
        let path = unsafe { CStr::from_ptr(ret).to_string_lossy() };
        path.to_string()
    }
}

#[cfg(unix)]
/// Преобразует числовую скорость в тип BaudRate.
pub fn speed_to_baud(speed: u32) -> Option<BaudRate> {
    use nix::sys::termios::BaudRate::*;
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
