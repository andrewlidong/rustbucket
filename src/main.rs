use std::fs::{File, OpenOptions, rename, remove_file};
use std::io::{self, Write, BufRead, BufReader};
use std::path::Path;
use std::thread;
use std::time::Duration;
use std::sync::{Arc, Mutex};
use chrono::Local;
use clap::{Parser, Subcommand};
use fs2::FileExt;

const LOG_FILE: &str = "http.log";
const MAX_LOG_FILES: u32 = 5;

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

fn run_server() -> io::Result<()> {
    let mut counter = 0;
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(LOG_FILE)?;

    file.lock_exclusive()?;
    println!("Server started. Press Ctrl+C to stop.");

    loop {
        counter += 1;
        append_log(&mut file, &format!("Log entry #{}", counter))?;
        thread::sleep(Duration::from_secs(1));
    }
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
    }

    Ok(())
} 