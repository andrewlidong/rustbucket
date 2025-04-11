# Rust Logging Utility

A simple command-line utility for logging messages and counting log entries.

## Usage

The program provides two commands:

1. `run` - Append a log entry to the log file
2. `count` - Count the number of log entries

### Examples

To add a log entry:
```bash
cargo run -- run "This is a test message"
```

To count log entries:
```bash
cargo run -- count
```

## Log Format

Log entries are formatted as:
```
[YYYY-MM-DD HH:MM:SS] message
```

The log file is stored as `server.log` in the current directory. 