use clap::Parser;

#[derive(Parser, Debug)]
#[command(
    author = "s00d <Virus191288@gmail.com>",
    version = env!("CARGO_PKG_VERSION"),
    about = "A program to create a virtual serial port (PTY) with extended functionality.",
    long_about = "This program creates a virtual serial port using pseudoterminal (PTY) with configurable baud rate, parity, and logging options. It also supports sending heartbeat messages and managing serial communication through the command-line interface."
)]
pub struct Args {
    /// Path for the symbolic link to the virtual port (Unix only)
    #[arg(short = 'l', long, default_value = "/tmp/my_virtual_port", help = "Specify the symbolic link path for the virtual serial port.")]
    pub link: String,

    /// Enable verbose logging to stdout
    #[arg(short = 'v', long, default_value_t = false, help = "Enable verbose logging for additional information in the terminal.")]
    pub verbose: bool,

    /// Do not disable echo (echo is disabled by default)
    #[arg(short = 'e', long, default_value_t = false, help = "If enabled, the echo feature will remain active on the slave device.")]
    pub enable_echo: bool,

    /// Initial message that will be sent to the virtual port upon startup
    #[arg(short = 'i', long, help = "Provide an initial message to be sent to the virtual port after startup.")]
    pub init_msg: Option<String>,

    /// Path to a file for logging communication
    #[arg(short = 'f', long, help = "Specify a path for logging communication to a file.")]
    pub log_file: Option<String>,

    /// Heartbeat interval in seconds
    #[arg(short = 'b', long, default_value_t = 0, help = "Set the interval (in seconds) for sending heartbeat messages.")]
    pub heartbeat: u64,

    /// Text for the heartbeat message
    #[arg(short = 'm', long, default_value = "HEARTBEAT\n", help = "Set the text for the heartbeat message.")]
    pub hb_msg: String,

    /// Set the baud rate for the virtual serial port
    #[arg(short = 'r', long, default_value = "9600", help = "Set the baud rate for the virtual serial port.")]
    pub baud_rate: String,

    /// Set the parity for the serial connection (none, even, odd)
    #[arg(short = 'p', long, default_value = "none", value_parser = ["none", "even", "odd"], help = "Set the parity for the virtual serial port")]
    pub parity: String,
}
