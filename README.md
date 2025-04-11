# Rust Logging Utility

A command-line utility for logging messages, counting log entries, and managing log rotation.

## Features

- Continuous logging with timestamps
- Log file rotation
- File locking for concurrent access
- Log entry counting

## Usage

The program provides three commands:

1. `run` - Start the logging server (writes a log entry every second)
2. `count` - Count the number of log entries
3. `rotate` - Rotate log files (renames http.log to http.1.log, etc.)

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