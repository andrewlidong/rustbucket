use std::fs::{File, OpenOptions};
use std::io::{self, Write, BufRead, BufReader};
use std::path::Path;
use chrono::Local;
use clap::{Parser, Subcommand};

const LOG_FILE: &str = "server.log";

#[derive(Parser)]
#[command(author, version, about, long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Append a log entry to the log file
    Run {
        /// The message to log
        message: String,
    },
    /// Count the number of log entries
    Count,
}

fn append_log(message: &str) -> io::Result<()> {
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(LOG_FILE)?;

    let timestamp = Local::now().format("%Y-%m-%d %H:%M:%S");
    writeln!(file, "[{}] {}", timestamp, message)?;
    Ok(())
}

fn count_logs() -> io::Result<()> {
    if !Path::new(LOG_FILE).exists() {
        println!("Log file does not exist. No entries to count.");
        return Ok(());
    }

    let file = File::open(LOG_FILE)?;
    let reader = BufReader::new(file);
    let count = reader.lines().count();
    println!("Total log entries: {}", count);
    Ok(())
}

fn main() -> io::Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Run { message } => {
            append_log(&message)?;
            println!("Log entry added successfully");
        }
        Commands::Count => {
            count_logs()?;
        }
    }

    Ok(())
} 