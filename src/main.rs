use std::fs::{File, OpenOptions, rename, remove_file};
use std::io::{self, Write, BufRead, BufReader};
use std::path::Path;
use std::thread;
use std::time::Duration;
use chrono::Local;
use clap::{Parser, Subcommand};
use fs2::FileExt;
use nix::unistd::{fork, ForkResult};
use nix::sys::wait::{waitpid, WaitStatus};
use std::process::Command;
use memmap2::{MmapMut, MmapOptions};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;

const LOG_FILE: &str = "http.log";
const MAX_LOG_FILES: u32 = 5;
const NUM_CHILDREN: usize = 4;
const CONFIG_FILE: &str = "config.dat";

#[derive(Debug, Clone, Copy)]
struct Config {
    verbosity: u32,
    max_connections: u32,
    timeout_seconds: u32,
    version: u32,  // Used to detect config changes
}

impl Config {
    fn new() -> Self {
        Self {
            verbosity: 1,
            max_connections: 100,
            timeout_seconds: 30,
            version: 0,
        }
    }

    fn to_bytes(&self) -> [u8; 16] {
        let mut bytes = [0u8; 16];
        bytes[0..4].copy_from_slice(&self.verbosity.to_ne_bytes());
        bytes[4..8].copy_from_slice(&self.max_connections.to_ne_bytes());
        bytes[8..12].copy_from_slice(&self.timeout_seconds.to_ne_bytes());
        bytes[12..16].copy_from_slice(&self.version.to_ne_bytes());
        bytes
    }

    fn from_bytes(bytes: &[u8; 16]) -> Self {
        Self {
            verbosity: u32::from_ne_bytes(bytes[0..4].try_into().unwrap()),
            max_connections: u32::from_ne_bytes(bytes[4..8].try_into().unwrap()),
            timeout_seconds: u32::from_ne_bytes(bytes[8..12].try_into().unwrap()),
            version: u32::from_ne_bytes(bytes[12..16].try_into().unwrap()),
        }
    }
}

#[derive(Parser)]
#[command(author, version, about, long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Start the logging server
    Run,
    /// Count the number of log entries
    Count,
    /// Rotate log files
    Rotate,
    /// Update server configuration
    UpdateConfig {
        /// Verbosity level (0-3)
        #[arg(short, long)]
        verbosity: Option<u32>,
        /// Maximum number of connections
        #[arg(short, long)]
        max_connections: Option<u32>,
        /// Connection timeout in seconds
        #[arg(short, long)]
        timeout: Option<u32>,
    },
}

fn rotate_logs() -> io::Result<()> {
    // Delete the oldest log file if it exists
    let oldest_log = format!("{}.{}", LOG_FILE, MAX_LOG_FILES);
    if Path::new(&oldest_log).exists() {
        remove_file(&oldest_log)?;
    }

    // Rotate existing log files
    for i in (1..MAX_LOG_FILES).rev() {
        let old_name = format!("{}.{}", LOG_FILE, i);
        let new_name = format!("{}.{}", LOG_FILE, i + 1);
        if Path::new(&old_name).exists() {
            rename(&old_name, &new_name)?;
        }
    }

    // Rename current log file to .1
    if Path::new(LOG_FILE).exists() {
        rename(LOG_FILE, format!("{}.1", LOG_FILE))?;
    }

    Ok(())
}

fn append_log(file: &mut File, message: &str) -> io::Result<()> {
    let timestamp = Local::now().format("%Y-%m-%d %H:%M:%S");
    writeln!(file, "[{}] {}", timestamp, message)?;
    file.flush()?;
    Ok(())
}

fn count_logs() -> io::Result<()> {
    if !Path::new(LOG_FILE).exists() {
        println!("Log file does not exist. No entries to count.");
        return Ok(());
    }

    let file = File::open(LOG_FILE)?;
    file.lock_shared()?;
    let reader = BufReader::new(file);
    let count = reader.lines().count();
    println!("Total log entries: {}", count);
    Ok(())
}

fn update_config(config: &mut Config, verbosity: Option<u32>, max_connections: Option<u32>, timeout: Option<u32>) {
    if let Some(v) = verbosity {
        config.verbosity = v;
    }
    if let Some(m) = max_connections {
        config.max_connections = m;
    }
    if let Some(t) = timeout {
        config.timeout_seconds = t;
    }
    config.version += 1;
}

fn spawn_child_process(child_id: usize) -> io::Result<()> {
    // Open memory-mapped config file
    let file = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .open(CONFIG_FILE)?;
    file.set_len(16)?; // Ensure file is large enough

    let mmap = unsafe { MmapOptions::new().map_mut(&file)? };

    let mut last_version = 0;
    let mut sleep_time = (child_id + 1) * 5; // 5, 10, 15, 20 seconds

    println!("Child {} started with initial sleep time: {} seconds", child_id, sleep_time);
    
    loop {
        // Read current config
        let mut config_bytes = [0u8; 16];
        config_bytes.copy_from_slice(&mmap[..16]);
        let config = Config::from_bytes(&config_bytes);

        // Check for config updates
        if config.version != last_version {
            println!("Child {} detected config update: {:?}", child_id, config);
            last_version = config.version;
            sleep_time = (child_id + 1) * 5 * (config.verbosity as usize + 1);
        }

        // Sleep for the configured duration
        thread::sleep(Duration::from_secs(sleep_time as u64));
        println!("Child {} woke up after {} seconds", child_id, sleep_time);
    }
}

fn run_server() -> io::Result<()> {
    let mut counter = 0;
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(LOG_FILE)?;

    file.lock_exclusive()?;
    println!("Server started. Press Ctrl+C to stop.");

    // Create memory-mapped config file
    let config_file = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .open(CONFIG_FILE)?;
    config_file.set_len(16)?; // Ensure file is large enough

    let mut mmap = unsafe { MmapOptions::new().map_mut(&config_file)? };

    // Initialize config
    let config = Config::new();
    mmap[..16].copy_from_slice(&config.to_bytes());

    // Fork child processes
    let mut children = Vec::new();
    for i in 0..NUM_CHILDREN {
        match unsafe { fork() } {
            Ok(ForkResult::Parent { child, .. }) => {
                println!("Forked child process with PID: {}", child);
                children.push(child);
            }
            Ok(ForkResult::Child) => {
                // Child process
                if let Err(e) = spawn_child_process(i) {
                    eprintln!("Child {} error: {}", i, e);
                    std::process::exit(1);
                }
                std::process::exit(0);
            }
            Err(e) => {
                eprintln!("Fork failed: {}", e);
                return Err(io::Error::new(io::ErrorKind::Other, "Fork failed"));
            }
        }
    }

    // Main server loop
    loop {
        // Check for completed child processes
        let mut completed_children = Vec::new();
        for &child_pid in &children {
            match waitpid(child_pid, None) {
                Ok(WaitStatus::Exited(pid, status)) => {
                    println!("Child {} exited with status {}", pid, status);
                    completed_children.push(pid);
                }
                Ok(WaitStatus::Signaled(pid, signal, _)) => {
                    println!("Child {} terminated by signal {:?}", pid, signal);
                    completed_children.push(pid);
                }
                _ => {}
            }
        }

        // Remove completed children
        children.retain(|&p| !completed_children.contains(&p));

        // Continue with logging
        counter += 1;
        append_log(&mut file, &format!("Log entry #{}", counter))?;
        thread::sleep(Duration::from_secs(1));

        // Exit if all children have finished
        if children.is_empty() {
            println!("All child processes have completed");
            break;
        }
    }

    Ok(())
}

fn update_server_config(verbosity: Option<u32>, max_connections: Option<u32>, timeout: Option<u32>) -> io::Result<()> {
    // Open memory-mapped config file
    let file = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .open(CONFIG_FILE)?;
    file.set_len(16)?; // Ensure file is large enough

    let mut mmap = unsafe { MmapOptions::new().map_mut(&file)? };

    // Read current config
    let mut config_bytes = [0u8; 16];
    config_bytes.copy_from_slice(&mmap[..16]);
    let mut config = Config::from_bytes(&config_bytes);

    // Update config
    update_config(&mut config, verbosity, max_connections, timeout);

    // Write updated config
    mmap[..16].copy_from_slice(&config.to_bytes());

    println!("Configuration updated: {:?}", config);
    Ok(())
}

fn main() -> io::Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Run => {
            run_server()?;
        }
        Commands::Count => {
            count_logs()?;
        }
        Commands::Rotate => {
            rotate_logs()?;
            println!("Log files rotated successfully");
        }
        Commands::UpdateConfig { verbosity, max_connections, timeout } => {
            update_server_config(verbosity, max_connections, timeout)?;
        }
    }

    Ok(())
} 