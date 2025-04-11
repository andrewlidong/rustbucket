# Rust Logging Utility

A command-line utility for logging messages, counting log entries, and managing log rotation.

## Features

- TCP server with thread-based concurrency
- Continuous logging with timestamps
- Log file rotation
- File locking for concurrent access
- Log entry counting
- Live configuration updates using memory-mapped files

## Usage

The program provides four commands:

1. `run` - Start the web server (listens for TCP connections)
2. `count` - Count the number of log entries
3. `rotate` - Rotate log files (renames http.log to http.1.log, etc.)
4. `update-config` - Update server configuration while running

### Examples

To start the web server:
```bash
# Start server on default port (8080) with default threads (4)
cargo run -- run

# Start server on specific port with custom number of threads
cargo run -- run --port 3000 --threads 8
```

To count log entries:
```bash
cargo run -- count
```

To rotate log files:
```bash
cargo run -- rotate
```

To update server configuration:
```bash
# Update verbosity level
cargo run -- update-config --verbosity 2

# Update maximum connections
cargo run -- update-config --max-connections 200

# Update timeout
cargo run -- update-config --timeout 60
```

## TCP Protocol

The server implements a simple text-based protocol:

1. Connect to the server using netcat:
```bash
# On macOS/Linux
nc localhost 8080

# On Windows (if you have netcat installed)
nc.exe localhost 8080
```

2. Send commands:
- `hello server` - Server responds with `hello client`
- Any other command - Server responds with `unknown command`

## Log Format

Log entries are formatted as:
```
[YYYY-MM-DD HH:MM:SS] message
```

## Log Rotation

The program maintains up to 5 log files:
- `http.log` (current log)
- `http.1.log` (most recent rotated log)
- `http.2.log`
- `http.3.log`
- `http.4.log`
- `http.5.log` (oldest rotated log, will be deleted on rotation)

When rotating logs:
1. `http.5.log` is deleted (if it exists)
2. Each log file is renamed to the next number (e.g., `http.1.log` → `http.2.log`)
3. `http.log` is renamed to `http.1.log`

## Thread Management

When running the server:
- Creates a fixed-size thread pool at startup
- Each incoming connection is handled by a worker thread from the pool
- Threads share configuration through atomic reference counting
- Default thread pool size is 4, but can be configured at startup

## Configuration Management

The server uses memory-mapped files to share configuration between threads. Configuration parameters include:

- `verbosity`: Log verbosity level (0-3)
- `max_connections`: Maximum number of concurrent connections
- `timeout_seconds`: Connection timeout in seconds

Configuration changes are detected by worker threads in real-time, and they adjust their behavior accordingly. The configuration is stored in `config.dat` and is shared between all threads.

### Default Configuration

- Verbosity: 1
- Maximum Connections: 100
- Timeout: 30 seconds 