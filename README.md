# Rust Logging Utility

A command-line utility for logging messages, counting log entries, and managing log rotation.

## Features

- Continuous logging with timestamps
- Log file rotation
- File locking for concurrent access
- Log entry counting
- Process forking and child process management
- Live configuration updates using memory-mapped files

## Usage

The program provides four commands:

1. `run` - Start the logging server (writes a log entry every second and manages child processes)
2. `count` - Count the number of log entries
3. `rotate` - Rotate log files (renames http.log to http.1.log, etc.)
4. `update-config` - Update server configuration while running

### Examples

To start the logging server:
```bash
cargo run -- run
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

## Process Management

When running the server:
- Forks 4 child processes
- Each child process runs for a different duration (5, 10, 15, and 20 seconds)
- Parent process continues logging while monitoring child processes
- Server exits automatically when all child processes complete
- Child process status is reported as they complete

## Configuration Management

The server uses memory-mapped files to share configuration between processes. Configuration parameters include:

- `verbosity`: Log verbosity level (0-3)
- `max_connections`: Maximum number of concurrent connections
- `timeout_seconds`: Connection timeout in seconds

Configuration changes are detected by child processes in real-time, and they adjust their behavior accordingly. The configuration is stored in `config.dat` and is shared between all processes.

### Default Configuration

- Verbosity: 1
- Maximum Connections: 100
- Timeout: 30 seconds 