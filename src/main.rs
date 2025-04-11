//! A TCP server with thread-based concurrency, signal handling, and graceful shutdown capabilities.
//! 
//! This server implements a simple TCP protocol where clients can connect and send commands.
//! The server uses a thread pool to handle multiple connections concurrently and implements
//! graceful shutdown on receiving SIGINT/SIGTERM signals.

use std::fs::{File, OpenOptions, rename, remove_file};
use std::io::{self, Write, BufRead, BufReader, Read};
use std::path::Path;
use std::thread;
use std::time::Duration;
use chrono::Local;
use clap::{Parser, Subcommand};
use fs2::FileExt;
use memmap2::{MmapMut, MmapOptions};
use std::sync::atomic::{AtomicU32, Ordering, AtomicBool};
use std::sync::Arc;
use std::net::{TcpListener, TcpStream};
use std::str;
use threadpool::ThreadPool;
use ctrlc;

const LOG_FILE: &str = "http.log";
const MAX_LOG_FILES: u32 = 5;
const CONFIG_FILE: &str = "config.dat";
const DEFAULT_PORT: u16 = 8080;
const NUM_THREADS: usize = 4;

/// Server configuration structure
#[derive(Debug, Clone, Copy)]
struct Config {
    verbosity: u32,
    max_connections: u32,
    timeout_seconds: u32,
    version: u32,  // Used to detect config changes
    port: u16,
}

impl Config {
    /// Creates a new Config with default values
    fn new() -> Self {
        Self {
            verbosity: 1,
            max_connections: 100,
            timeout_seconds: 30,
            version: 0,
            port: DEFAULT_PORT,
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
            port: DEFAULT_PORT,
        }
    }
}

/// Command-line interface arguments
#[derive(Parser)]
#[command(author, version, about, long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

/// Available subcommands for the CLI
#[derive(Subcommand)]
enum Commands {
    /// Start the web server
    Run {
        /// Port to listen on
        #[arg(short, long, default_value_t = DEFAULT_PORT)]
        port: u16,
        /// Number of worker threads
        #[arg(short, long, default_value_t = NUM_THREADS)]
        threads: usize,
    },
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

/// Appends a message to the log file with timestamp
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

/// Server state shared across threads
#[derive(Debug)]
struct ServerState {
    /// Flag indicating if a shutdown has been requested
    shutdown_requested: AtomicBool,
    /// Flag for forcing immediate shutdown
    force_shutdown: AtomicBool,
}

impl ServerState {
    /// Creates a new ServerState with default values
    fn new() -> Self {
        Self {
            shutdown_requested: AtomicBool::new(false),
            force_shutdown: AtomicBool::new(false),
        }
    }
}

/// Sets up signal handlers for graceful shutdown
fn setup_signal_handlers(server_state: Arc<ServerState>) -> io::Result<()> {
    let server_state_clone = Arc::clone(&server_state);
    
    // Handle SIGINT (Ctrl+C) and SIGTERM
    ctrlc::set_handler(move || {
        if server_state_clone.shutdown_requested.load(Ordering::SeqCst) {
            println!("Second SIGTERM received, forcing shutdown...");
            server_state_clone.force_shutdown.store(true, Ordering::SeqCst);
        } else {
            println!("SIGTERM received, initiating graceful shutdown...");
            server_state_clone.shutdown_requested.store(true, Ordering::SeqCst);
        }
    }).map_err(|e| io::Error::new(io::ErrorKind::Other, e))?;
    
    Ok(())
}

/// Runs the TCP server with the specified configuration
fn run_server(port: u16, num_threads: usize) -> io::Result<()> {
    // Initialize server state
    let server_state = Arc::new(ServerState::new());
    
    // Set up signal handlers
    setup_signal_handlers(Arc::clone(&server_state))?;

    // Create memory-mapped config file
    let config_file = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .open(CONFIG_FILE)?;
    config_file.set_len(16)?;

    let mut mmap = unsafe { MmapOptions::new().map_mut(&config_file)? };

    // Initialize config
    let config = Arc::new(Config::new());
    mmap[..16].copy_from_slice(&config.to_bytes());

    // Create thread pool
    let pool = ThreadPool::new(num_threads);
    println!("Created thread pool with {} workers", num_threads);

    // Main server loop
    let listener = TcpListener::bind(format!("127.0.0.1:{}", port))?;
    println!("Server listening on port {} with {} worker threads", port, num_threads);

    for stream in listener.incoming() {
        // Check for shutdown request
        if server_state.shutdown_requested.load(Ordering::SeqCst) {
            println!("Shutdown requested, stopping new connections...");
            break;
        }

        match stream {
            Ok(stream) => {
                // Read current config for this connection
                let mut config_bytes = [0u8; 16];
                config_bytes.copy_from_slice(&mmap[..16]);
                let current_config = Config::from_bytes(&config_bytes);
                let config = Arc::new(current_config);

                // Clone the Arc for the thread
                let config_clone = Arc::clone(&config);
                let server_state_clone = Arc::clone(&server_state);
                
                // Spawn a new thread to handle the connection
                pool.execute(move || {
                    if let Err(e) = handle_connection(stream, config_clone, server_state_clone) {
                        eprintln!("Error handling connection: {}", e);
                    }
                });
            }
            Err(e) => {
                eprintln!("Failed to accept connection: {}", e);
            }
        }
    }

    // Wait for all active connections to complete
    println!("Waiting for active connections to complete...");
    pool.join();

    println!("Server shutdown complete");
    Ok(())
}

/// Handles a single client connection
fn handle_connection(mut stream: TcpStream, config: Arc<Config>, server_state: Arc<ServerState>) -> io::Result<()> {
    let mut buffer = [0; 1024];
    
    // Set read timeout to prevent hanging on inactive connections
    stream.set_read_timeout(Some(Duration::from_secs(30)))?;
    
    while !server_state.force_shutdown.load(Ordering::SeqCst) {
        match stream.read(&mut buffer) {
            Ok(0) => break, // Connection closed by client
            Ok(n) => {
                let message = String::from_utf8_lossy(&buffer[..n]);
                println!("Received: {}", message.trim());
                
                // Simple echo server response
                stream.write_all(b"Echo: ")?;
                stream.write_all(&buffer[..n])?;
            }
            Err(e) if e.kind() == io::ErrorKind::WouldBlock => {
                // Check for shutdown request during timeout
                if server_state.shutdown_requested.load(Ordering::SeqCst) {
                    break;
                }
                continue;
            }
            Err(e) => return Err(e),
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

/// Main entry point
fn main() -> io::Result<()> {
    let args = Cli::parse();
    
    match args.command {
        Commands::Run { port, threads } => {
            run_server(port, threads)?;
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