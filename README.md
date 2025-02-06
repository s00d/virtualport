# Virtual Serial Port (PTY) Emulator

A Rust-based tool to create virtual serial ports using pseudo-terminals (PTYs) with advanced features like heartbeat messages, logging, and real-time configuration.

## Features

- **PTY Creation**: Creates master/slave pseudo-terminal pairs.
- **Symbolic Link**: Exposes the slave PTY via a customizable symlink (e.g., `/tmp/my_virtual_port`).
- **Baud Rate Control**: Dynamically adjust baud rate using `/baud` commands.
- **Parity Settings**: Configure parity bits (none/even/odd) via `/parity` commands.
- **Heartbeat Messages**: Periodically send configurable messages to the port.
- **Logging**: Log all communications to a file.
- **Non-Blocking I/O**: Efficiently handle input/output without blocking threads.
- **Echo Control**: Disable/enable terminal echo on the slave device.

## Installation

### Prerequisites
- Rust and Cargo: [Install Rust](https://www.rust-lang.org/tools/install)
- Linux/macOS (Unix-like system required)

### Steps
1. Clone the repository:
   ```bash
   git clone https://github.com/s00d/virtualport.git
   cd virtualport
   ```
2. Build the project:
   ```bash
   cargo build --release
   ```

## Usage

### Basic Example
```bash
# Run with verbose output
sudo cargo run -- --verbose

# In another terminal, read from the virtual port
sudo cat /tmp/my_virtual_port

# Send data to the virtual port
echo "Hello World" > /tmp/my_virtual_port
```

### Command-Line Options
```bash
USAGE:
    virtualport [OPTIONS] --link <LINK>

OPTIONS:
    -l, --link <LINK>          Symlink path for the virtual port [default: /tmp/my_virtual_port]
    -v, --verbose              Enable verbose logging
    --enable-echo              Keep echo enabled on the slave device
    --init-msg <INIT_MSG>      Initial message to send on startup
    --log-file <LOG_FILE>      Path to log file (e.g., serial.log)
    --heartbeat <HEARTBEAT>    Heartbeat interval in seconds (0 = disabled)
    --hb-msg <HB_MSG>          Custom heartbeat message [default: HEARTBEAT\n]
```

### Advanced Examples
1. **Heartbeat and Logging**:
   ```bash
   sudo cargo run -- \
     --verbose \
     --log-file serial.log \
     --heartbeat 5 \
     --hb-msg "PING\n"
   ```

2. **Initial Message and Custom Baud Rate**:
   ```bash
   sudo cargo run -- \
     --init-msg "INIT" \
     --enable-echo

   # In the program's console:
   /baud 9600
   ```

3. **Interact Programmatically**:
   ```bash
   # Send commands from shell
   echo "/parity even" > /tmp/my_virtual_port
   ```

## Technical Details

### PTY Workflow
1. **Master/Slave Creation**: Uses `openpty` to create a PTY pair.
2. **Symlink**: Binds the slave PTY to a user-friendly path.
3. **Threads**:
    - **Master Reader**: Reads data from the master PTY and prints `[Received]` messages.
    - **Stdin Writer**: Sends user input from stdin to the master PTY.
    - **Heartbeat**: Periodically writes heartbeat messages to the master PTY.
4. **Configuration**:
    - Baud rate and parity settings are applied to the slave PTY using termios.

## Data Flow

```mermaid
graph LR
    A[User Input/Console] -->|stdin| B[Stdin Writer Thread]
    B -->|writes to| C[Master PTY]
    C -->|read by| D[Master Reader Thread]
    D -->|logs/output| E[[Console: [Received] ...]]
    F[Heartbeat Thread] -->|periodic writes| C
    C <-->|PTY Pair| G[Slave PTY]
    G -->|symlink| H[/tmp/my_virtual_port]
    H -->|read/write| I[External Tools (e.g., `cat`, `echo`)]
    I -->|writes data| H
    G -->|read by| J[Slave Reader Thread]
    J -->|writes back to| C
```

### Diagram Explanation:
1. **User Input** (via console) is sent to the `Stdin Writer Thread`, which writes it to the **Master PTY**.
2. **Master PTY**:
   - Data is read by the `Master Reader Thread` and displayed as `[Received] ...`.
   - `Heartbeat Thread` periodically writes messages (e.g., `HEARTBEAT`).
3. **Slave PTY**:
   - Linked to a symlink (`/tmp/my_virtual_port`).
   - External tools (e.g., `echo`, `cat`) interact with the slave via the symlink.
4. **Slave Reader Thread**:
   - Reads data from the **Slave PTY** and writes it back to the **Master PTY** (loopback).
   - Ensures data sent to the slave (e.g., via `echo`) is forwarded to the master and displayed in the console.

### Key Components:
- 🔄 **PTY Pair**: Master and slave are connected bidirectionally.
- 📝 **Symlink**: Provides user-friendly access to the slave PTY.
- 🧵 **Threads**: Non-blocking I/O ensures real-time communication.
- 💓 **Heartbeat**: Optional periodic messages for monitoring.

### Working with `commands.txt`

The program supports automated command-response handling through a file named `commands.txt`. This file allows you to predefine command-response pairs that the emulator will use to process incoming commands during runtime.

#### How It Works:
- **File Structure**: The file is expected to contain alternating lines: the first line of each pair is a command, and the second line is its corresponding response. For example, if the emulator receives the command matching one of these entries, it will automatically send back the associated response.

- **Location**: Place `commands.txt` in the same directory from which you launch the emulator.

- **Loading Commands**: Upon startup, the emulator reads `commands.txt` and loads all command-response pairs into memory. If the file is not found, the program will issue a warning and continue running without predefined commands.

- **Usage in Communication**:
   - **Incoming Data Handling**: In the reader thread, when a complete line (terminated by `\n`) is received from the master PTY, it is checked against the loaded commands. If a match is found, the program responds with the predefined response from the file.
   - **Interactive Input**: Similarly, if you input a command interactively via the terminal, the program checks the command against the loaded pairs and sends back the associated response if available.

#### Example `commands.txt`:
```text
AT
OK
AT+CSQ
+CSQ: 23,99
AT+CREG?
+CREG: 0,1
```

## Troubleshooting

### Common Issues
1. **Permission Denied**:
    - Use `sudo` to create PTY devices and symlinks.
    - Ensure `/tmp` has write permissions.

2. **Data Not Visible in `cat`**:
    - Enable echo with `--enable-echo` to loopback data.
    - Ensure no other process is holding the PTY open.

3. **Garbage Output**:
    - Match baud rates between the program and external devices.
    - Check parity settings.

4. **Heartbeat Not Working**:
    - Ensure `--heartbeat` is greater than 0.
    - Use `--verbose` to debug.

## License
MIT License. Contributions welcome!