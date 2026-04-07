use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use clap::{Args, Subcommand};
use time::Date;
use time::macros::format_description;

use crate::storage::Storage;

const EXPORT_CSV_HEADER: &str = "date,task name,start time,end time,description,project,total time";

/// Export commands
#[derive(Subcommand)]
pub enum ExportCommands {
    /// Export records for exactly one date
    Date(ExportDateArgs),

    /// Export records for a date range or all dates
    Range(ExportRangeArgs),

    /// List dates that currently have records
    ListDates,
}

#[derive(Args)]
pub struct ExportDateArgs {
    /// Date in YYYY-MM-DD format
    date: String,

    #[command(flatten)]
    output: ExportOutputArgs,
}

#[derive(Args)]
pub struct ExportRangeArgs {
    /// Either "all" or two dates: YYYY-MM-DD YYYY-MM-DD
    #[arg(value_name = "DATE|all", required = true, num_args = 1..=2)]
    range: Vec<String>,

    /// Export one CSV file per date
    #[arg(long)]
    individual: bool,

    #[command(flatten)]
    output: ExportOutputArgs,
}

#[derive(Args, Default)]
pub struct ExportOutputArgs {
    /// Print CSV to stdout instead of writing file(s)
    #[arg(long)]
    stdout: bool,

    /// Directory where export files should be written
    #[arg(long, value_name = "DIR")]
    out_dir: Option<PathBuf>,
}

pub(super) fn handle_export(command: ExportCommands, storage: Storage) -> Result<()> {
    match command {
        ExportCommands::ListDates => handle_export_list_dates(storage),
        ExportCommands::Date(args) => handle_export_date(args, storage),
        ExportCommands::Range(args) => handle_export_range(args, storage),
    }
}

fn handle_export_list_dates(storage: Storage) -> Result<()> {
    let dates = storage.list_dates_with_records()?;
    for date in dates {
        println!("{}", format_date(date));
    }
    Ok(())
}

fn handle_export_date(args: ExportDateArgs, storage: Storage) -> Result<()> {
    validate_output_args(&args.output, false)?;

    let date = parse_cli_date(&args.date)?;
    let available_dates = storage.list_dates_with_records()?;
    if !available_dates.contains(&date) {
        anyhow::bail!("No records found for {}", format_date(date));
    }

    let csv = build_csv_for_dates(&storage, &[date])?;
    export_csv(
        &storage,
        csv,
        &args.output,
        &format!("work-records-{}.csv", format_date(date)),
    )
}

fn handle_export_range(args: ExportRangeArgs, storage: Storage) -> Result<()> {
    validate_output_args(&args.output, args.individual)?;

    let range_spec = parse_range_spec(&args.range)?;
    let available_dates = storage.list_dates_with_records()?;
    let selected_dates = match range_spec {
        RangeSpec::All => available_dates,
        RangeSpec::Bounded { start, end } => available_dates
            .into_iter()
            .filter(|date| *date >= start && *date <= end)
            .collect(),
    };

    if selected_dates.is_empty() {
        anyhow::bail!("No records found for selected range");
    }

    if args.individual {
        let out_dir = resolve_output_dir(&storage, args.output.out_dir.as_deref())?;
        let mut saved_files = Vec::with_capacity(selected_dates.len());

        for date in selected_dates {
            let csv = build_csv_for_dates(&storage, &[date])?;
            let file_name = format!("work-records-{}.csv", format_date(date));
            let file_path = out_dir.join(file_name);
            write_csv_file(&file_path, &csv)?;
            saved_files.push(file_path);
        }

        println!("Export saved to: {}", out_dir.display());
        for path in saved_files {
            let file_name = path
                .file_name()
                .and_then(|value| value.to_str())
                .unwrap_or("<invalid-utf8-file-name>");
            println!("  {}", file_name);
        }
        return Ok(());
    }

    let csv = build_csv_for_dates(&storage, &selected_dates)?;
    let file_name = match range_spec {
        RangeSpec::All => "work-records-all.csv".to_string(),
        RangeSpec::Bounded { start, end } => format!(
            "work-records-{}_to_{}.csv",
            format_date(start),
            format_date(end)
        ),
    };
    export_csv(&storage, csv, &args.output, &file_name)
}

fn validate_output_args(output: &ExportOutputArgs, individual: bool) -> Result<()> {
    if output.stdout && output.out_dir.is_some() {
        anyhow::bail!("--stdout cannot be used with --out-dir");
    }
    if output.stdout && individual {
        anyhow::bail!("--stdout cannot be used with --individual");
    }
    Ok(())
}

#[derive(Clone, Copy, Debug)]
enum RangeSpec {
    All,
    Bounded { start: Date, end: Date },
}

fn parse_range_spec(args: &[String]) -> Result<RangeSpec> {
    match args {
        [value] if value == "all" => Ok(RangeSpec::All),
        [start_raw, end_raw] => {
            let start = parse_cli_date(start_raw)?;
            let end = parse_cli_date(end_raw)?;
            if end < start {
                anyhow::bail!(
                    "Invalid date range: {} is before {}",
                    format_date(end),
                    format_date(start)
                );
            }
            Ok(RangeSpec::Bounded { start, end })
        }
        [single] => anyhow::bail!(
            "Invalid range argument '{}'. Use 'all' or provide start and end dates",
            single
        ),
        _ => anyhow::bail!("Range expects either 'all' or two dates"),
    }
}

fn parse_cli_date(value: &str) -> Result<Date> {
    Date::parse(value, format_description!("[year]-[month]-[day]"))
        .context(format!("Invalid date: '{}'. Expected YYYY-MM-DD", value))
}

fn format_date(date: Date) -> String {
    format!(
        "{:04}-{:02}-{:02}",
        date.year(),
        date.month() as u8,
        date.day()
    )
}

fn build_csv_for_dates(storage: &Storage, dates: &[Date]) -> Result<String> {
    let mut csv = String::new();
    csv.push_str(EXPORT_CSV_HEADER);
    csv.push('\n');

    for date in dates {
        let day_data = storage.load(date)?;
        let records = day_data.get_sorted_records();
        for record in records {
            csv.push_str(&csv_escape(&format_date(*date)));
            csv.push(',');
            csv.push_str(&csv_escape(&record.name));
            csv.push(',');
            csv.push_str(&csv_escape(&record.start.to_string()));
            csv.push(',');
            csv.push_str(&csv_escape(&record.end.to_string()));
            csv.push(',');
            csv.push_str(&csv_escape(&record.description));
            csv.push(',');
            csv.push_str(&csv_escape(&record.project));
            csv.push(',');
            csv.push_str(&csv_escape(&format_total_time(record.total_minutes)));
            csv.push('\n');
        }
    }

    Ok(csv)
}

fn format_total_time(total_minutes: u32) -> String {
    let hours = total_minutes / 60;
    let minutes = total_minutes % 60;
    format!("{:02}:{:02}", hours, minutes)
}

fn csv_escape(value: &str) -> String {
    let must_quote =
        value.contains(',') || value.contains('"') || value.contains('\n') || value.contains('\r');
    if must_quote {
        format!("\"{}\"", value.replace('"', "\"\""))
    } else {
        value.to_string()
    }
}

fn resolve_output_dir(storage: &Storage, out_dir: Option<&Path>) -> Result<PathBuf> {
    let dir = if let Some(path) = out_dir {
        path.to_path_buf()
    } else {
        let diagnostics = storage.diagnostics()?;
        diagnostics
            .database_path
            .parent()
            .context("Could not determine default export directory")?
            .to_path_buf()
    };

    if dir.exists() {
        if !dir.is_dir() {
            anyhow::bail!("Export output path is not a directory: {}", dir.display());
        }
        return Ok(dir);
    }

    fs::create_dir_all(&dir).context(format!(
        "Failed to create export directory {}",
        dir.display()
    ))?;
    Ok(dir)
}

fn write_csv_file(path: &Path, csv: &str) -> Result<()> {
    fs::write(path, csv).context(format!("Failed to write export file {}", path.display()))?;
    Ok(())
}

fn export_csv(
    storage: &Storage,
    csv: String,
    output: &ExportOutputArgs,
    file_name: &str,
) -> Result<()> {
    if output.stdout {
        print!("{csv}");
        return Ok(());
    }

    let out_dir = resolve_output_dir(storage, output.out_dir.as_deref())?;
    let file_path = out_dir.join(file_name);
    write_csv_file(&file_path, &csv)?;
    println!("Export saved to: {}", file_path.display());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cli::{Cli, Commands};
    use crate::models::{DayData, TimePoint, WorkRecord};
    use clap::Parser;
    use tempfile::TempDir;
    use time::Month;

    fn create_record(id: u32, name: &str, start_hour: u8, end_hour: u8) -> WorkRecord {
        let start = TimePoint::new(start_hour, 0).unwrap();
        let end = TimePoint::new(end_hour, 0).unwrap();
        WorkRecord::new(id, name.to_string(), start, end)
    }

    fn store_day(storage: &Storage, date: Date, record_name: &str) {
        let mut day = DayData::new(date);
        let mut record = create_record(1, record_name, 9, 11);
        record.project = "ProjectA".to_string();
        record.description = "Focused work".to_string();
        day.add_record(record);
        storage.save(&day).unwrap();
    }

    #[test]
    fn test_cli_export_date_parse() {
        let cli = Cli::try_parse_from(["work-tuimer", "export", "date", "2025-11-06"]).unwrap();

        let Commands::Export { command } = cli.command else {
            panic!("Expected export command");
        };

        let ExportCommands::Date(args) = command else {
            panic!("Expected export date command");
        };

        assert_eq!(args.date, "2025-11-06");
        assert!(!args.output.stdout);
        assert!(args.output.out_dir.is_none());
    }

    #[test]
    fn test_cli_export_range_all_parse() {
        let cli = Cli::try_parse_from([
            "work-tuimer",
            "export",
            "range",
            "all",
            "--individual",
            "--out-dir",
            "/tmp/exports",
        ])
        .unwrap();

        let Commands::Export { command } = cli.command else {
            panic!("Expected export command");
        };

        let ExportCommands::Range(args) = command else {
            panic!("Expected export range command");
        };

        assert_eq!(args.range, vec!["all"]);
        assert!(args.individual);
        assert_eq!(
            args.output.out_dir.as_deref(),
            Some(std::path::Path::new("/tmp/exports"))
        );
    }

    #[test]
    fn test_cli_export_range_dates_parse() {
        let cli = Cli::try_parse_from([
            "work-tuimer",
            "export",
            "range",
            "2025-11-01",
            "2025-11-10",
            "--stdout",
        ])
        .unwrap();

        let Commands::Export { command } = cli.command else {
            panic!("Expected export command");
        };

        let ExportCommands::Range(args) = command else {
            panic!("Expected export range command");
        };

        assert_eq!(args.range, vec!["2025-11-01", "2025-11-10"]);
        assert!(args.output.stdout);
        assert!(!args.individual);
    }

    #[test]
    fn test_parse_range_spec_all() {
        let spec = parse_range_spec(&["all".to_string()]).unwrap();
        assert!(matches!(spec, RangeSpec::All));
    }

    #[test]
    fn test_parse_range_spec_bounded() {
        let spec = parse_range_spec(&["2025-11-01".to_string(), "2025-11-02".to_string()]).unwrap();
        let RangeSpec::Bounded { start, end } = spec else {
            panic!("Expected bounded range");
        };

        assert_eq!(format_date(start), "2025-11-01");
        assert_eq!(format_date(end), "2025-11-02");
    }

    #[test]
    fn test_parse_range_spec_rejects_invalid_single_value() {
        let err = parse_range_spec(&["2025-11-01".to_string()]).unwrap_err();
        assert!(format!("{err:#}").contains("Invalid range argument"));
    }

    #[test]
    fn test_validate_output_args_rejects_stdout_with_individual() {
        let output = ExportOutputArgs {
            stdout: true,
            out_dir: None,
        };

        let err = validate_output_args(&output, true).unwrap_err();
        assert!(format!("{err:#}").contains("--stdout cannot be used with --individual"));
    }

    #[test]
    fn test_validate_output_args_rejects_stdout_with_out_dir() {
        let output = ExportOutputArgs {
            stdout: true,
            out_dir: Some(PathBuf::from("/tmp")),
        };

        let err = validate_output_args(&output, false).unwrap_err();
        assert!(format!("{err:#}").contains("--stdout cannot be used with --out-dir"));
    }

    #[test]
    fn test_format_total_time_as_hh_mm() {
        assert_eq!(format_total_time(0), "00:00");
        assert_eq!(format_total_time(5), "00:05");
        assert_eq!(format_total_time(75), "01:15");
    }

    #[test]
    fn test_csv_escape_quotes_and_commas() {
        assert_eq!(csv_escape("simple"), "simple");
        assert_eq!(csv_escape("hello,world"), "\"hello,world\"");
        assert_eq!(csv_escape("say \"hello\""), "\"say \"\"hello\"\"\"");
    }

    #[test]
    fn test_handle_export_range_writes_only_dates_inside_bounds() {
        let temp_dir = TempDir::new().unwrap();
        let storage = Storage::new_with_dir(temp_dir.path().to_path_buf()).unwrap();
        let out_dir = temp_dir.path().join("exports");

        let day_1 = Date::from_calendar_date(2025, Month::November, 1).unwrap();
        let day_3 = Date::from_calendar_date(2025, Month::November, 3).unwrap();
        let day_5 = Date::from_calendar_date(2025, Month::November, 5).unwrap();
        store_day(&storage, day_1, "Out of range early");
        store_day(&storage, day_3, "In range");
        store_day(&storage, day_5, "Out of range late");

        handle_export(
            ExportCommands::Range(ExportRangeArgs {
                range: vec!["2025-11-02".to_string(), "2025-11-04".to_string()],
                individual: false,
                output: ExportOutputArgs {
                    stdout: false,
                    out_dir: Some(out_dir.clone()),
                },
            }),
            storage.clone(),
        )
        .unwrap();

        let export_file = out_dir.join("work-records-2025-11-02_to_2025-11-04.csv");
        let csv = fs::read_to_string(export_file).unwrap();
        let lines: Vec<&str> = csv.lines().collect();

        assert_eq!(lines[0], EXPORT_CSV_HEADER);
        assert_eq!(lines.len(), 2);
        assert!(csv.contains("2025-11-03"));
        assert!(!csv.contains("2025-11-01"));
        assert!(!csv.contains("2025-11-05"));
    }

    #[test]
    fn test_handle_export_range_all_individual_creates_one_file_per_date_with_records() {
        let temp_dir = TempDir::new().unwrap();
        let storage = Storage::new_with_dir(temp_dir.path().to_path_buf()).unwrap();
        let out_dir = temp_dir.path().join("exports");

        let day_1 = Date::from_calendar_date(2025, Month::November, 1).unwrap();
        let day_3 = Date::from_calendar_date(2025, Month::November, 3).unwrap();
        store_day(&storage, day_1, "Day 1 task");
        store_day(&storage, day_3, "Day 3 task");
        storage
            .save(&DayData::new(
                Date::from_calendar_date(2025, Month::November, 2).unwrap(),
            ))
            .unwrap();

        handle_export(
            ExportCommands::Range(ExportRangeArgs {
                range: vec!["all".to_string()],
                individual: true,
                output: ExportOutputArgs {
                    stdout: false,
                    out_dir: Some(out_dir.clone()),
                },
            }),
            storage,
        )
        .unwrap();

        let day_1_file = out_dir.join("work-records-2025-11-01.csv");
        let day_3_file = out_dir.join("work-records-2025-11-03.csv");
        let day_2_file = out_dir.join("work-records-2025-11-02.csv");

        assert!(day_1_file.exists());
        assert!(day_3_file.exists());
        assert!(!day_2_file.exists());
    }

    #[test]
    fn test_handle_export_range_returns_error_when_no_dates_match() {
        let temp_dir = TempDir::new().unwrap();
        let storage = Storage::new_with_dir(temp_dir.path().to_path_buf()).unwrap();

        let day_1 = Date::from_calendar_date(2025, Month::November, 1).unwrap();
        store_day(&storage, day_1, "Only day");

        let result = handle_export(
            ExportCommands::Range(ExportRangeArgs {
                range: vec!["2025-11-10".to_string(), "2025-11-11".to_string()],
                individual: false,
                output: ExportOutputArgs {
                    stdout: false,
                    out_dir: Some(temp_dir.path().join("exports")),
                },
            }),
            storage,
        );

        assert!(result.is_err());
        assert!(
            format!("{:#}", result.err().unwrap()).contains("No records found for selected range")
        );
    }
}
