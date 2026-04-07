use std::time::Duration;

use anyhow::Result;
use clap::{Parser, Subcommand};
use time::OffsetDateTime;

use crate::storage::Storage;
use crate::timer::TimerManager;

mod export;
use export::ExportCommands;

/// WorkTimer CLI - Automatic time tracking
#[derive(Parser)]
#[command(name = "work-tuimer")]
#[command(about = "Automatic time tracking with CLI commands and TUI", long_about = None)]
#[command(version)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
}

/// Available CLI commands
#[derive(Subcommand)]
pub enum Commands {
    /// Manage timer sessions (start/stop/pause/resume/status)
    Session {
        #[command(subcommand)]
        command: SessionCommands,
    },

    /// Show storage and migration diagnostics
    Doctor,

    /// Export work records to CSV
    Export {
        #[command(subcommand)]
        command: ExportCommands,
    },
}

/// Session management commands
#[derive(Subcommand)]
pub enum SessionCommands {
    /// Start a new timer session
    Start {
        /// Task name
        task: String,

        /// Optional task description
        #[arg(short, long)]
        description: Option<String>,

        /// Optional project name
        #[arg(long)]
        project: Option<String>,

        /// Optional customer name
        #[arg(long)]
        customer: Option<String>,
    },

    /// Stop the running timer session
    Stop,

    /// Pause the running timer session
    Pause,

    /// Resume the paused timer session
    Resume,

    /// Show status of running timer session
    Status,
}

/// Handle CLI command execution
pub fn handle_command(cmd: Commands, storage: Storage) -> Result<()> {
    match cmd {
        Commands::Session { command } => match command {
            SessionCommands::Start {
                task,
                description,
                project,
                customer,
            } => handle_start(task, description, project, customer, storage),
            SessionCommands::Stop => handle_stop(storage),
            SessionCommands::Pause => handle_pause(storage),
            SessionCommands::Resume => handle_resume(storage),
            SessionCommands::Status => handle_status(storage),
        },
        Commands::Doctor => handle_doctor(storage),
        Commands::Export { command } => export::handle_export(command, storage),
    }
}

fn handle_doctor(storage: Storage) -> Result<()> {
    let diagnostics = storage.diagnostics()?;

    println!("WorkTimer Doctor");
    println!("  Database: {}", diagnostics.database_path.display());
    println!(
        "  Migration marker: {}",
        diagnostics
            .migration_marker
            .as_deref()
            .unwrap_or("<not-set>")
    );
    println!("  Days stored: {}", diagnostics.days_count);
    println!("  Work records: {}", diagnostics.work_records_count);
    println!(
        "  Active timer: {}",
        if diagnostics.active_timer_present {
            "present"
        } else {
            "none"
        }
    );
    println!(
        "  Legacy JSON backups: {} day files, {} timer files",
        diagnostics.legacy_day_json_files, diagnostics.legacy_timer_json_files
    );

    if diagnostics.migration_marker.is_some() {
        println!("  Status: OK (SQLite migration completed)");
    } else {
        println!(
            "  Status: WARN (unexpected missing migration marker; possible failed migration or DB issue)"
        );
    }

    Ok(())
}

/// Start a new session
fn handle_start(
    task: String,
    description: Option<String>,
    project: Option<String>,
    customer: Option<String>,
    storage: Storage,
) -> Result<()> {
    let timer_manager = TimerManager::new(storage);

    // Trim task name
    let task = task.trim().to_string();
    if task.is_empty() {
        return Err(anyhow::anyhow!("Task name cannot be empty"));
    }

    let project = project
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty());
    let customer = customer
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty());

    let timer = timer_manager.start(task, description, project, customer, None, None)?;

    let start_time = format_time(timer.start_time);
    println!("✓ Session started");
    println!("  Task: {}", timer.task_name);
    if let Some(project) = &timer.project {
        println!("  Project: {}", project);
    }
    if let Some(customer) = &timer.customer {
        println!("  Customer: {}", customer);
    }
    if let Some(desc) = &timer.description {
        println!("  Description: {}", desc);
    }
    println!("  Started at: {}", start_time);

    Ok(())
}

/// Stop the running session
fn handle_stop(storage: Storage) -> Result<()> {
    let timer_manager = TimerManager::new(storage);

    // Load and validate timer exists
    let timer = timer_manager
        .status()?
        .ok_or_else(|| anyhow::anyhow!("No session is running"))?;

    let elapsed = timer_manager.get_elapsed_duration(&timer);
    let formatted_duration = format_duration(elapsed);

    let start_time = format_time(timer.start_time);

    // Stop the timer and get the work record
    let record = timer_manager.stop()?;

    // Format end time from the work record (HH:MM format)
    let end_time = format!("{:02}:{:02}:{:02}", record.end.hour, record.end.minute, 0);

    println!("✓ Session stopped");
    println!("  Task: {}", timer.task_name);
    if let Some(project) = &timer.project {
        println!("  Project: {}", project);
    }
    if let Some(customer) = &timer.customer {
        println!("  Customer: {}", customer);
    }
    println!("  Duration: {}", formatted_duration);
    println!("  Started at: {}", start_time);
    println!("  Ended at: {}", end_time);

    Ok(())
}

/// Pause the running session
fn handle_pause(storage: Storage) -> Result<()> {
    let timer_manager = TimerManager::new(storage);

    let timer = timer_manager
        .status()?
        .ok_or_else(|| anyhow::anyhow!("No session is running"))?;

    let _paused_timer = timer_manager.pause()?;
    let elapsed = timer_manager.get_elapsed_duration(&timer);
    let formatted_duration = format_duration(elapsed);

    println!("⏸ Session paused");
    println!("  Task: {}", timer.task_name);
    println!("  Elapsed: {}", formatted_duration);

    Ok(())
}

/// Resume the paused session
fn handle_resume(storage: Storage) -> Result<()> {
    let timer_manager = TimerManager::new(storage);

    let timer = timer_manager
        .status()?
        .ok_or_else(|| anyhow::anyhow!("No session is running"))?;

    let _resumed_timer = timer_manager.resume()?;
    let elapsed = timer_manager.get_elapsed_duration(&timer);
    let formatted_duration = format_duration(elapsed);

    println!("▶ Session resumed");
    println!("  Task: {}", timer.task_name);
    println!("  Total elapsed (before pause): {}", formatted_duration);

    Ok(())
}

/// Show status of running session
fn handle_status(storage: Storage) -> Result<()> {
    let timer_manager = TimerManager::new(storage);

    match timer_manager.status()? {
        Some(timer) => {
            let elapsed = timer_manager.get_elapsed_duration(&timer);
            let formatted_duration = format_duration(elapsed);
            let start_time = format_time(timer.start_time);

            println!("⏱ Session Status");
            println!("  Task: {}", timer.task_name);
            println!(
                "  Status: {}",
                match timer.status {
                    crate::timer::TimerStatus::Running => "Running",
                    crate::timer::TimerStatus::Paused => "Paused",
                    crate::timer::TimerStatus::Stopped => "Stopped",
                }
            );
            println!("  Elapsed: {}", formatted_duration);
            println!("  Started at: {}", start_time);
            if let Some(project) = &timer.project {
                println!("  Project: {}", project);
            }
            if let Some(customer) = &timer.customer {
                println!("  Customer: {}", customer);
            }
            if let Some(desc) = &timer.description {
                println!("  Description: {}", desc);
            }
        }
        None => {
            println!("No session is currently running");
        }
    }

    Ok(())
}

/// Format OffsetDateTime for display (HH:MM:SS)
fn format_time(dt: OffsetDateTime) -> String {
    format!("{:02}:{:02}:{:02}", dt.hour(), dt.minute(), dt.second())
}

/// Format Duration for display (h:mm:ss or mm:ss)
fn format_duration(duration: Duration) -> String {
    let total_secs = duration.as_secs();
    let hours = total_secs / 3600;
    let minutes = (total_secs % 3600) / 60;
    let seconds = total_secs % 60;

    if hours > 0 {
        format!("{}h {:02}m {:02}s", hours, minutes, seconds)
    } else {
        format!("{}m {:02}s", minutes, seconds)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_duration_hours_minutes_seconds() {
        let duration = Duration::from_secs(3661); // 1h 1m 1s
        assert_eq!(format_duration(duration), "1h 01m 01s");
    }

    #[test]
    fn test_format_duration_minutes_seconds() {
        let duration = Duration::from_secs(125); // 2m 5s
        assert_eq!(format_duration(duration), "2m 05s");
    }

    #[test]
    fn test_format_duration_seconds_only() {
        let duration = Duration::from_secs(45);
        assert_eq!(format_duration(duration), "0m 45s");
    }

    #[test]
    fn test_format_duration_zero() {
        let duration = Duration::from_secs(0);
        assert_eq!(format_duration(duration), "0m 00s");
    }

    #[test]
    fn test_format_time() {
        use time::macros::datetime;
        let dt = datetime!(2025-01-15 14:30:45 UTC);
        assert_eq!(format_time(dt), "14:30:45");
    }

    #[test]
    fn test_cli_has_version() {
        use clap::CommandFactory;
        let cmd = Cli::command();
        let version = cmd.get_version();
        assert!(version.is_some(), "CLI should have version configured");
        // Version comes from Cargo.toml
        assert_eq!(version.unwrap(), env!("CARGO_PKG_VERSION"));
    }

    #[test]
    fn test_cli_doctor_command_parse() {
        let cli = Cli::try_parse_from(["work-tuimer", "doctor"]).unwrap();
        assert!(matches!(cli.command, Commands::Doctor));
    }

    #[test]
    fn test_cli_session_start_parses_project_and_customer() {
        let cli = Cli::try_parse_from([
            "work-tuimer",
            "session",
            "start",
            "My Task",
            "--project",
            "Internal Platform",
            "--customer",
            "ACME",
        ])
        .unwrap();

        let Commands::Session { command } = cli.command else {
            panic!("Expected session command");
        };

        match command {
            SessionCommands::Start {
                task,
                project,
                customer,
                ..
            } => {
                assert_eq!(task, "My Task");
                assert_eq!(project.as_deref(), Some("Internal Platform"));
                assert_eq!(customer.as_deref(), Some("ACME"));
            }
            _ => panic!("Expected session start command"),
        }
    }
}
