use crate::models::{DayData, TimePoint, WorkRecord};
use crate::timer::{TimerState, TimerStatus};
use anyhow::{Context, Result, anyhow};
use rusqlite::{Connection, OptionalExtension, Transaction, params};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration as StdDuration, SystemTime};
use time::format_description::well_known::Rfc3339;
use time::macros::format_description;
use time::{Date, OffsetDateTime};

const DATABASE_FILE_NAME: &str = "work-tuimer.db";
const LEGACY_RUNNING_TIMER_FILE_NAME: &str = "running_timer.json";
const LEGACY_ACTIVE_TIMER_FILE_NAME: &str = "active_timer.json";
const JSON_MIGRATION_META_KEY: &str = "migration.json_to_sqlite.v1";

#[derive(Clone)]
pub struct Storage {
    repository: SqliteRepository,
}

#[derive(Clone)]
struct SqliteRepository {
    db_path: PathBuf,
}

/// High-level storage manager that provides transactional operations
/// and automatic external modification tracking.
pub struct StorageManager {
    storage: Storage,
    file_modified_times: HashMap<Date, Option<SystemTime>>,
}

pub struct StorageDiagnostics {
    pub database_path: PathBuf,
    pub migration_marker: Option<String>,
    pub days_count: u64,
    pub work_records_count: u64,
    pub active_timer_present: bool,
    pub legacy_day_json_files: u64,
    pub legacy_timer_json_files: u64,
}

impl StorageManager {
    /// Create a new StorageManager
    pub fn new() -> Result<Self> {
        Ok(StorageManager {
            storage: Storage::new()?,
            file_modified_times: HashMap::new(),
        })
    }

    /// Create a new StorageManager with a custom directory (for testing)
    #[doc(hidden)]
    #[allow(dead_code)]
    pub fn new_with_dir(data_dir: PathBuf) -> Result<Self> {
        Ok(StorageManager {
            storage: Storage::new_with_dir(data_dir)?,
            file_modified_times: HashMap::new(),
        })
    }

    /// Load day data with automatic revision tracking
    pub fn load_with_tracking(&mut self, date: Date) -> Result<DayData> {
        let data = self.storage.load(&date)?;
        let modified_time = self.storage.try_get_file_modified_time(&date)?;
        self.file_modified_times.insert(date, modified_time);
        Ok(data)
    }

    /// Check if day data changed externally and reload if needed.
    /// Returns Some(DayData) if data was modified and reloaded, None if no change.
    pub fn check_and_reload(&mut self, date: Date) -> Result<Option<DayData>> {
        let current_modified = self.storage.try_get_file_modified_time(&date)?;

        let is_tracked = self.file_modified_times.contains_key(&date);
        if !is_tracked {
            let data = self.storage.load(&date)?;
            self.file_modified_times.insert(date, current_modified);
            return Ok(Some(data));
        }

        let last_known = self.file_modified_times.get(&date).copied().flatten();
        // We only need inequality detection here. We intentionally do not rely
        // on ordering semantics across processes.
        if current_modified != last_known {
            let data = self.storage.load(&date)?;
            self.file_modified_times.insert(date, current_modified);
            Ok(Some(data))
        } else {
            Ok(None)
        }
    }

    /// Add a new work record (transactional: load -> add -> save -> track)
    #[allow(dead_code)]
    pub fn add_record(&mut self, date: Date, record: WorkRecord) -> Result<()> {
        let mut day_data = self.storage.load(&date)?;
        day_data.add_record(record);
        self.storage.save(&day_data)?;

        let modified_time = self.storage.try_get_file_modified_time(&date)?;
        self.file_modified_times.insert(date, modified_time);

        Ok(())
    }

    /// Update an existing work record (transactional: load -> update -> save -> track)
    #[allow(dead_code)]
    pub fn update_record(&mut self, date: Date, record: WorkRecord) -> Result<()> {
        let mut day_data = self.storage.load(&date)?;
        day_data.add_record(record);
        self.storage.save(&day_data)?;

        let modified_time = self.storage.try_get_file_modified_time(&date)?;
        self.file_modified_times.insert(date, modified_time);

        Ok(())
    }

    /// Remove a work record by ID (transactional: load -> remove -> save -> track)
    /// Returns the removed record if found.
    #[allow(dead_code)]
    pub fn remove_record(&mut self, date: Date, id: u32) -> Result<WorkRecord> {
        let mut day_data = self.storage.load(&date)?;

        let record = day_data
            .work_records
            .remove(&id)
            .context(format!("Record with ID {} not found", id))?;

        self.storage.save(&day_data)?;

        let modified_time = self.storage.try_get_file_modified_time(&date)?;
        self.file_modified_times.insert(date, modified_time);

        Ok(record)
    }

    /// Save day data and update tracking
    pub fn save(&mut self, day_data: &DayData) -> Result<()> {
        self.storage.save(day_data)?;

        let modified_time = self.storage.try_get_file_modified_time(&day_data.date)?;
        self.file_modified_times
            .insert(day_data.date, modified_time);

        Ok(())
    }

    /// Get the last known modification token for a date
    pub fn get_last_modified(&self, date: &Date) -> Option<SystemTime> {
        self.file_modified_times.get(date).copied().flatten()
    }

    pub fn recent_task_names(&self, date: Date, days_back: u8) -> Result<Vec<String>> {
        self.storage.recent_task_names(date, days_back)
    }

    #[allow(dead_code)]
    pub fn save_active_timer(&self, timer: &TimerState) -> Result<()> {
        self.storage.save_active_timer(timer)
    }

    pub fn load_active_timer(&self) -> Result<Option<TimerState>> {
        self.storage.load_active_timer()
    }

    #[allow(dead_code)]
    pub fn clear_active_timer(&self) -> Result<()> {
        self.storage.clear_active_timer()
    }

    fn create_timer_manager(&self) -> crate::timer::TimerManager {
        crate::timer::TimerManager::new(self.storage.clone())
    }

    pub fn start_timer(
        &self,
        task_name: String,
        description: Option<String>,
        project: Option<String>,
        customer: Option<String>,
        source_record_id: Option<u32>,
        source_record_date: Option<time::Date>,
    ) -> Result<TimerState> {
        let timer_manager = self.create_timer_manager();
        timer_manager.start(
            task_name,
            description,
            project,
            customer,
            source_record_id,
            source_record_date,
        )
    }

    pub fn stop_timer(&self) -> Result<crate::models::WorkRecord> {
        let timer_manager = self.create_timer_manager();
        timer_manager.stop()
    }

    pub fn pause_timer(&self) -> Result<TimerState> {
        let timer_manager = self.create_timer_manager();
        timer_manager.pause()
    }

    pub fn resume_timer(&self) -> Result<TimerState> {
        let timer_manager = self.create_timer_manager();
        timer_manager.resume()
    }

    #[allow(dead_code)]
    pub fn get_timer_elapsed(&self, timer: &TimerState) -> std::time::Duration {
        let timer_manager = self.create_timer_manager();
        timer_manager.get_elapsed_duration(timer)
    }
}

impl Storage {
    pub fn new() -> Result<Self> {
        let data_dir = Self::get_data_directory()?;
        Self::new_with_dir(data_dir)
    }

    /// Create a new Storage with a custom directory (for testing)
    #[doc(hidden)]
    #[allow(dead_code)]
    pub fn new_with_dir(data_dir: PathBuf) -> Result<Self> {
        fs::create_dir_all(&data_dir).context("Failed to create data directory")?;

        let db_path = data_dir.join(DATABASE_FILE_NAME);
        let repository = SqliteRepository::new(db_path);
        repository.initialize(&data_dir)?;

        Ok(Storage { repository })
    }

    fn get_data_directory() -> Result<PathBuf> {
        if let Some(data_dir) = dirs::data_local_dir() {
            let app_dir = data_dir.join("work-tuimer");
            if fs::create_dir_all(&app_dir).is_ok() {
                return Ok(app_dir);
            }
        }

        let local_data = PathBuf::from("./data");
        if fs::create_dir_all(&local_data).is_ok() {
            return Ok(local_data);
        }

        anyhow::bail!("Failed to create data directory in system location or ./data")
    }

    pub fn load(&self, date: &Date) -> Result<DayData> {
        self.repository.load_day(date)
    }

    pub fn save(&self, day_data: &DayData) -> Result<()> {
        self.repository.save_day(day_data)
    }

    /// Get a synthetic monotonic token derived from day revision state.
    /// Returns None if the day has never been written.
    ///
    /// This is not a wall-clock timestamp; it is used only for equality/
    /// inequality change detection.
    #[allow(dead_code)]
    pub fn get_file_modified_time(&self, date: &Date) -> Option<SystemTime> {
        match self.try_get_file_modified_time(date) {
            Ok(token) => token,
            Err(err) => {
                eprintln!(
                    "Failed to read day revision token for {} from storage: {err:#}",
                    date
                );
                None
            }
        }
    }

    pub fn try_get_file_modified_time(&self, date: &Date) -> Result<Option<SystemTime>> {
        self.repository.day_revision_token(date)
    }

    pub fn save_active_timer(&self, timer: &TimerState) -> Result<()> {
        self.repository.save_active_timer(timer)
    }

    pub fn load_active_timer(&self) -> Result<Option<TimerState>> {
        self.repository.load_active_timer()
    }

    pub fn clear_active_timer(&self) -> Result<()> {
        self.repository.clear_active_timer()
    }

    pub fn diagnostics(&self) -> Result<StorageDiagnostics> {
        self.repository.diagnostics()
    }

    pub fn list_dates_with_records(&self) -> Result<Vec<Date>> {
        self.repository.list_dates_with_records()
    }

    pub fn recent_task_names(&self, date: Date, days_back: u8) -> Result<Vec<String>> {
        self.repository.recent_task_names(date, days_back)
    }

    #[cfg(test)]
    fn get_db_path(&self) -> PathBuf {
        self.repository.db_path.clone()
    }
}

impl SqliteRepository {
    fn new(db_path: PathBuf) -> Self {
        Self { db_path }
    }

    fn initialize(&self, data_dir: &Path) -> Result<()> {
        let conn = self.open_connection()?;
        Self::apply_database_pragmas(&conn)?;
        Self::initialize_schema(&conn)?;
        Self::apply_schema_migrations(&conn)?;
        drop(conn);

        self
            .migrate_from_legacy_json_if_needed(data_dir)
            .context("Failed to migrate legacy JSON into SQLite. Fix or remove malformed legacy JSON files and restart")
    }

    fn open_connection(&self) -> Result<Connection> {
        let conn = Connection::open(&self.db_path).context(format!(
            "Failed to open SQLite database at {:?}",
            self.db_path
        ))?;
        conn.execute_batch("PRAGMA foreign_keys = ON;")
            .context("Failed to enable SQLite foreign key support")?;
        Ok(conn)
    }

    fn apply_database_pragmas(conn: &Connection) -> Result<()> {
        conn.execute_batch(
            "
            PRAGMA journal_mode = WAL;
            PRAGMA synchronous = NORMAL;
            ",
        )
        .context("Failed to configure SQLite pragmas")?;
        Ok(())
    }

    fn initialize_schema(conn: &Connection) -> Result<()> {
        conn.execute_batch(
            "
            CREATE TABLE IF NOT EXISTS meta (
                key TEXT PRIMARY KEY,
                value TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS day_meta (
                date TEXT PRIMARY KEY,
                last_id INTEGER NOT NULL DEFAULT 0 CHECK (last_id >= 0),
                revision INTEGER NOT NULL DEFAULT 0 CHECK (revision >= 0)
            );

            CREATE TABLE IF NOT EXISTS work_records (
                date TEXT NOT NULL,
                id INTEGER NOT NULL,
                name TEXT NOT NULL,
                start_minutes INTEGER NOT NULL,
                end_minutes INTEGER NOT NULL,
                total_minutes INTEGER NOT NULL,
                project TEXT NOT NULL DEFAULT '',
                customer TEXT NOT NULL DEFAULT '',
                description TEXT NOT NULL DEFAULT '',
                PRIMARY KEY (date, id),
                FOREIGN KEY (date) REFERENCES day_meta(date) ON DELETE CASCADE
            );

            CREATE INDEX IF NOT EXISTS idx_work_records_date_start
                ON work_records(date, start_minutes);

            CREATE TABLE IF NOT EXISTS active_timer (
                singleton_id INTEGER PRIMARY KEY CHECK (singleton_id = 1),
                id INTEGER,
                task_name TEXT NOT NULL,
                description TEXT,
                project TEXT,
                customer TEXT,
                start_time TEXT NOT NULL,
                end_time TEXT,
                date TEXT NOT NULL,
                status TEXT NOT NULL CHECK (status IN ('running', 'paused', 'stopped')),
                paused_duration_secs INTEGER NOT NULL CHECK (paused_duration_secs >= 0),
                paused_at TEXT,
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL,
                source_record_id INTEGER,
                source_record_date TEXT
            );
            ",
        )
        .context("Failed to initialize SQLite schema")?;

        Ok(())
    }

    fn apply_schema_migrations(conn: &Connection) -> Result<()> {
        Self::ensure_column_exists(conn, "work_records", "project", "TEXT NOT NULL DEFAULT ''")?;
        Self::ensure_column_exists(conn, "work_records", "customer", "TEXT NOT NULL DEFAULT ''")?;
        Self::ensure_column_exists(conn, "active_timer", "project", "TEXT")?;
        Self::ensure_column_exists(conn, "active_timer", "customer", "TEXT")?;
        Ok(())
    }

    fn ensure_column_exists(
        conn: &Connection,
        table: &str,
        column: &str,
        definition: &str,
    ) -> Result<()> {
        if Self::has_column(conn, table, column)? {
            return Ok(());
        }

        let alter_sql = format!("ALTER TABLE {table} ADD COLUMN {column} {definition}");
        conn.execute(&alter_sql, [])
            .context(format!("Failed to add column {column} to {table}"))?;
        Ok(())
    }

    fn has_column(conn: &Connection, table: &str, column: &str) -> Result<bool> {
        let pragma_sql = format!("PRAGMA table_info({table})");
        let mut stmt = conn
            .prepare(&pragma_sql)
            .context(format!("Failed to inspect table schema for {table}"))?;
        let mut rows = stmt.query([]).context("Failed to query table_info")?;

        while let Some(row) = rows.next().context("Failed to read table_info row")? {
            let column_name = row.get::<_, String>(1)?;
            if column_name == column {
                return Ok(true);
            }
        }

        Ok(false)
    }

    fn load_day(&self, date: &Date) -> Result<DayData> {
        let conn = self.open_connection()?;
        let date_key = format_date(*date);

        let last_id_opt = conn
            .query_row(
                "SELECT last_id FROM day_meta WHERE date = ?1",
                params![date_key.clone()],
                |row| row.get::<_, i64>(0),
            )
            .optional()
            .context("Failed to query day metadata")?;

        let Some(last_id_raw) = last_id_opt else {
            return Ok(DayData::new(*date));
        };

        let last_id = i64_to_u32(last_id_raw, "day_meta.last_id")?;

        let mut day_data = DayData {
            date: *date,
            last_id,
            work_records: HashMap::new(),
        };

        let mut stmt = conn
            .prepare(
                "
                SELECT id, name, start_minutes, end_minutes, total_minutes, project, customer, description
                FROM work_records
                WHERE date = ?1
                ORDER BY id
                ",
            )
            .context("Failed to prepare work records query")?;

        let mut rows = stmt
            .query(params![date_key])
            .context("Failed to query work records")?;

        while let Some(row) = rows.next().context("Failed to read work record row")? {
            let id = i64_to_u32(row.get::<_, i64>(0)?, "work_records.id")?;
            let name = row.get::<_, String>(1)?;
            let start_minutes = i64_to_u32(row.get::<_, i64>(2)?, "work_records.start_minutes")?;
            let end_minutes = i64_to_u32(row.get::<_, i64>(3)?, "work_records.end_minutes")?;
            let total_minutes = i64_to_u32(row.get::<_, i64>(4)?, "work_records.total_minutes")?;
            let project = row.get::<_, Option<String>>(5)?.unwrap_or_default();
            let customer = row.get::<_, Option<String>>(6)?.unwrap_or_default();
            let description = row.get::<_, Option<String>>(7)?.unwrap_or_default();

            let start = TimePoint::from_minutes_since_midnight(start_minutes)
                .map_err(|e| anyhow!(e))
                .context("Invalid start_minutes value in database")?;
            let end = TimePoint::from_minutes_since_midnight(end_minutes)
                .map_err(|e| anyhow!(e))
                .context("Invalid end_minutes value in database")?;

            let record = WorkRecord {
                id,
                name,
                start,
                end,
                total_minutes,
                project,
                customer,
                description,
            };

            day_data.work_records.insert(id, record);
        }

        Ok(day_data)
    }

    fn save_day(&self, day_data: &DayData) -> Result<()> {
        let mut conn = self.open_connection()?;
        let tx = conn
            .transaction()
            .context("Failed to begin day save transaction")?;

        Self::write_day_data_tx(&tx, day_data, true)?;

        tx.commit()
            .context("Failed to commit day save transaction")?;
        Ok(())
    }

    fn day_revision_token(&self, date: &Date) -> Result<Option<SystemTime>> {
        let conn = self.open_connection()?;
        let date_key = format_date(*date);

        let revision_opt = conn
            .query_row(
                "SELECT revision FROM day_meta WHERE date = ?1",
                params![date_key],
                |row| row.get::<_, i64>(0),
            )
            .optional()
            .context("Failed to query day revision")?;

        match revision_opt {
            None => Ok(None),
            Some(revision) => {
                if revision < 0 {
                    anyhow::bail!("Invalid negative revision in database: {}", revision);
                }
                // Convert revision counter into a synthetic token. We only use
                // this value as a monotonic change marker, not as a real time.
                let token = SystemTime::UNIX_EPOCH
                    .checked_add(StdDuration::from_secs(revision as u64))
                    .context("Failed to convert revision to SystemTime")?;
                Ok(Some(token))
            }
        }
    }

    fn save_active_timer(&self, timer: &TimerState) -> Result<()> {
        let mut conn = self.open_connection()?;
        let tx = conn
            .transaction()
            .context("Failed to begin active timer transaction")?;

        Self::save_active_timer_tx(&tx, timer)?;
        tx.commit()
            .context("Failed to commit active timer transaction")?;

        Ok(())
    }

    fn load_active_timer(&self) -> Result<Option<TimerState>> {
        let conn = self.open_connection()?;

        let timer_row = conn
            .query_row(
                "
                SELECT
                    id,
                    task_name,
                    description,
                    project,
                    customer,
                    start_time,
                    end_time,
                    date,
                    status,
                    paused_duration_secs,
                    paused_at,
                    created_at,
                    updated_at,
                    source_record_id,
                    source_record_date
                FROM active_timer
                WHERE singleton_id = 1
                ",
                [],
                |row| {
                    Ok((
                        row.get::<_, Option<i64>>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, Option<String>>(2)?,
                        row.get::<_, Option<String>>(3)?,
                        row.get::<_, Option<String>>(4)?,
                        row.get::<_, String>(5)?,
                        row.get::<_, Option<String>>(6)?,
                        row.get::<_, String>(7)?,
                        row.get::<_, String>(8)?,
                        row.get::<_, i64>(9)?,
                        row.get::<_, Option<String>>(10)?,
                        row.get::<_, String>(11)?,
                        row.get::<_, String>(12)?,
                        row.get::<_, Option<i64>>(13)?,
                        row.get::<_, Option<String>>(14)?,
                    ))
                },
            )
            .optional()
            .context("Failed to query active timer")?;

        let Some(row) = timer_row else {
            return Ok(None);
        };

        let id = row
            .0
            .map(|v| i64_to_u32(v, "active_timer.id"))
            .transpose()?;
        let task_name = row.1;
        let description = row.2;
        let project = row.3;
        let customer = row.4;
        let start_time = parse_datetime(&row.5, "active_timer.start_time")?;
        let end_time = row
            .6
            .as_deref()
            .map(|v| parse_datetime(v, "active_timer.end_time"))
            .transpose()?;
        let date = parse_date(&row.7).context("Invalid active_timer.date")?;
        let status = parse_timer_status(&row.8)?;
        let paused_duration_secs = row.9;
        let paused_at = row
            .10
            .as_deref()
            .map(|v| parse_datetime(v, "active_timer.paused_at"))
            .transpose()?;
        let created_at = parse_datetime(&row.11, "active_timer.created_at")?;
        let updated_at = parse_datetime(&row.12, "active_timer.updated_at")?;
        let source_record_id = row
            .13
            .map(|v| i64_to_u32(v, "active_timer.source_record_id"))
            .transpose()?;
        let source_record_date = row
            .14
            .as_deref()
            .map(parse_date)
            .transpose()
            .context("Invalid active_timer.source_record_date")?;

        Ok(Some(TimerState {
            id,
            task_name,
            description,
            project,
            customer,
            start_time,
            end_time,
            date,
            status,
            paused_duration_secs,
            paused_at,
            created_at,
            updated_at,
            source_record_id,
            source_record_date,
        }))
    }

    fn clear_active_timer(&self) -> Result<()> {
        let conn = self.open_connection()?;
        conn.execute("DELETE FROM active_timer WHERE singleton_id = 1", [])
            .context("Failed to clear active timer")?;
        Ok(())
    }

    fn diagnostics(&self) -> Result<StorageDiagnostics> {
        let conn = self.open_connection()?;

        let migration_marker = conn
            .query_row(
                "SELECT value FROM meta WHERE key = ?1",
                params![JSON_MIGRATION_META_KEY],
                |row| row.get::<_, String>(0),
            )
            .optional()
            .context("Failed to query migration marker")?;

        let days_count = i64_to_u64(
            conn.query_row("SELECT COUNT(*) FROM day_meta", [], |row| {
                row.get::<_, i64>(0)
            })
            .context("Failed to count day rows")?,
            "day_meta count",
        )?;

        let work_records_count = i64_to_u64(
            conn.query_row("SELECT COUNT(*) FROM work_records", [], |row| {
                row.get::<_, i64>(0)
            })
            .context("Failed to count work record rows")?,
            "work_records count",
        )?;

        let active_timer_present = conn
            .query_row(
                "SELECT 1 FROM active_timer WHERE singleton_id = 1 LIMIT 1",
                [],
                |_row| Ok(()),
            )
            .optional()
            .context("Failed to check active timer existence")?
            .is_some();

        let database_path = self.db_path.clone();
        let data_dir = database_path
            .parent()
            .context("Database path has no parent directory")?;

        let (legacy_day_json_files, legacy_timer_json_files) =
            Self::count_legacy_json_files(data_dir)?;

        Ok(StorageDiagnostics {
            database_path,
            migration_marker,
            days_count,
            work_records_count,
            active_timer_present,
            legacy_day_json_files,
            legacy_timer_json_files,
        })
    }

    fn list_dates_with_records(&self) -> Result<Vec<Date>> {
        let conn = self.open_connection()?;
        let mut stmt = conn
            .prepare(
                "
                SELECT DISTINCT date
                FROM work_records
                ORDER BY date
                ",
            )
            .context("Failed to prepare query for exportable dates")?;

        let mut rows = stmt
            .query([])
            .context("Failed to query dates with work records")?;
        let mut dates = Vec::new();

        while let Some(row) = rows.next().context("Failed to read date row")? {
            let raw_date = row.get::<_, String>(0)?;
            let date = parse_date(&raw_date)
                .context(format!("Failed to parse date '{}' from storage", raw_date))?;
            dates.push(date);
        }

        Ok(dates)
    }

    fn recent_task_names(&self, date: Date, days_back: u8) -> Result<Vec<String>> {
        let start_date = (0..days_back).try_fold(date, |date, _| {
            date.previous_day()
                .context("Failed to calculate recent task date range")
        })?;

        let conn = self.open_connection()?;
        let mut stmt = conn
            .prepare(
                "
                SELECT name
                FROM work_records
                WHERE date >= ?1 AND date <= ?2
                ORDER BY date DESC, id DESC
                ",
            )
            .context("Failed to prepare recent task names query")?;

        let mut rows = stmt
            .query(params![format_date(start_date), format_date(date)])
            .context("Failed to query recent task names")?;

        let mut seen = HashSet::new();
        let mut task_names = Vec::new();

        while let Some(row) = rows.next().context("Failed to read recent task name row")? {
            let name = row.get::<_, String>(0)?.trim().to_string();
            if name.is_empty() || name == "New Task" || !seen.insert(name.clone()) {
                continue;
            }

            task_names.push(name);
        }

        Ok(task_names)
    }

    fn count_legacy_json_files(data_dir: &Path) -> Result<(u64, u64)> {
        let entries = fs::read_dir(data_dir).context("Failed to read data directory")?;

        let mut day_files_count = 0_u64;
        let mut timer_files_count = 0_u64;

        for entry in entries {
            let entry = entry.context("Failed to read directory entry")?;
            let path = entry.path();

            if !path.is_file() {
                continue;
            }

            let Some(file_name_os) = path.file_name() else {
                continue;
            };
            let Some(file_name) = file_name_os.to_str() else {
                continue;
            };

            if !file_name.ends_with(".json") {
                continue;
            }

            if matches!(
                file_name,
                LEGACY_RUNNING_TIMER_FILE_NAME | LEGACY_ACTIVE_TIMER_FILE_NAME
            ) {
                timer_files_count += 1;
                continue;
            }

            let Some(date_part) = file_name.strip_suffix(".json") else {
                continue;
            };

            if parse_date(date_part).is_ok() {
                day_files_count += 1;
            }
        }

        Ok((day_files_count, timer_files_count))
    }

    fn migrate_from_legacy_json_if_needed(&self, data_dir: &Path) -> Result<()> {
        let mut conn = self.open_connection()?;
        let tx = conn
            .transaction()
            .context("Failed to begin JSON migration transaction")?;

        if Self::get_meta_value_tx(&tx, JSON_MIGRATION_META_KEY)?.is_some() {
            tx.commit()
                .context("Failed to finalize already-completed migration transaction")?;
            return Ok(());
        }

        Self::import_legacy_day_files_tx(&tx, data_dir)?;
        Self::import_legacy_timer_tx(&tx, data_dir)?;
        Self::set_meta_value_tx(&tx, JSON_MIGRATION_META_KEY, "done")?;

        tx.commit().context("Failed to commit JSON migration")?;
        Ok(())
    }

    fn write_day_data_tx(
        tx: &Transaction<'_>,
        day_data: &DayData,
        increment_revision: bool,
    ) -> Result<()> {
        let date_key = format_date(day_data.date);

        tx.execute(
            "
            INSERT INTO day_meta(date, last_id, revision)
            VALUES (?1, 0, 0)
            ON CONFLICT(date) DO NOTHING
            ",
            params![date_key.clone()],
        )
        .context("Failed to upsert day_meta row")?;

        tx.execute(
            "DELETE FROM work_records WHERE date = ?1",
            params![date_key.clone()],
        )
        .context("Failed to clear existing day work records")?;

        {
            let mut stmt = tx
                .prepare(
                    "
                    INSERT INTO work_records (
                        date,
                        id,
                        name,
                        start_minutes,
                        end_minutes,
                        total_minutes,
                        project,
                        customer,
                        description
                    )
                    VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
                    ",
                )
                .context("Failed to prepare work record insert statement")?;

            for record in day_data.work_records.values() {
                stmt.execute(params![
                    date_key.clone(),
                    i64::from(record.id),
                    &record.name,
                    i64::from(record.start.to_minutes_since_midnight()),
                    i64::from(record.end.to_minutes_since_midnight()),
                    i64::from(record.total_minutes),
                    &record.project,
                    &record.customer,
                    &record.description,
                ])
                .context("Failed to insert work record")?;
            }
        }

        if increment_revision {
            // Revision is an i64 counter. It would require an impractical number
            // of writes to overflow, and serves only as a monotonic change token.
            tx.execute(
                "
                UPDATE day_meta
                SET last_id = ?1,
                    revision = revision + 1
                WHERE date = ?2
                ",
                params![i64::from(day_data.last_id), date_key],
            )
            .context("Failed to update day_meta with incremented revision")?;
        } else {
            // Migration initializes revision to at least 1 so downstream change
            // detection has a stable non-zero token from first persisted state.
            tx.execute(
                "
                UPDATE day_meta
                SET last_id = ?1,
                    revision = CASE WHEN revision = 0 THEN 1 ELSE revision END
                WHERE date = ?2
                ",
                params![i64::from(day_data.last_id), date_key],
            )
            .context("Failed to update day_meta during migration")?;
        }

        Ok(())
    }

    fn save_active_timer_tx(tx: &Transaction<'_>, timer: &TimerState) -> Result<()> {
        let id = timer.id.map(i64::from);
        let start_time = format_datetime(timer.start_time)?;
        let end_time = timer.end_time.map(format_datetime).transpose()?;
        let date = format_date(timer.date);
        let status = timer_status_to_str(timer.status);
        let project = timer.project.as_deref();
        let customer = timer.customer.as_deref();
        let paused_at = timer.paused_at.map(format_datetime).transpose()?;
        let created_at = format_datetime(timer.created_at)?;
        let updated_at = format_datetime(timer.updated_at)?;
        let source_record_id = timer.source_record_id.map(i64::from);
        let source_record_date = timer.source_record_date.map(format_date);

        tx.execute(
            "
            INSERT INTO active_timer (
                singleton_id,
                id,
                task_name,
                description,
                project,
                customer,
                start_time,
                end_time,
                date,
                status,
                paused_duration_secs,
                paused_at,
                created_at,
                updated_at,
                source_record_id,
                source_record_date
            )
            VALUES (
                1,
                ?1,
                ?2,
                ?3,
                ?4,
                ?5,
                ?6,
                ?7,
                ?8,
                ?9,
                ?10,
                ?11,
                ?12,
                ?13,
                ?14,
                ?15
            )
            ON CONFLICT(singleton_id) DO UPDATE SET
                id = excluded.id,
                task_name = excluded.task_name,
                description = excluded.description,
                project = excluded.project,
                customer = excluded.customer,
                start_time = excluded.start_time,
                end_time = excluded.end_time,
                date = excluded.date,
                status = excluded.status,
                paused_duration_secs = excluded.paused_duration_secs,
                paused_at = excluded.paused_at,
                created_at = excluded.created_at,
                updated_at = excluded.updated_at,
                source_record_id = excluded.source_record_id,
                source_record_date = excluded.source_record_date
            ",
            params![
                id,
                &timer.task_name,
                timer.description.as_deref(),
                project,
                customer,
                start_time,
                end_time,
                date,
                status,
                timer.paused_duration_secs,
                paused_at,
                created_at,
                updated_at,
                source_record_id,
                source_record_date,
            ],
        )
        .context("Failed to upsert active timer")?;

        Ok(())
    }

    fn get_meta_value_tx(tx: &Transaction<'_>, key: &str) -> Result<Option<String>> {
        tx.query_row(
            "SELECT value FROM meta WHERE key = ?1",
            params![key],
            |row| row.get::<_, String>(0),
        )
        .optional()
        .context(format!("Failed to query meta key '{}'", key))
    }

    fn set_meta_value_tx(tx: &Transaction<'_>, key: &str, value: &str) -> Result<()> {
        tx.execute(
            "
            INSERT INTO meta(key, value)
            VALUES (?1, ?2)
            ON CONFLICT(key) DO UPDATE SET value = excluded.value
            ",
            params![key, value],
        )
        .context(format!("Failed to upsert meta key '{}'", key))?;
        Ok(())
    }

    fn day_exists_tx(tx: &Transaction<'_>, date_key: &str) -> Result<bool> {
        let exists = tx
            .query_row(
                "SELECT 1 FROM day_meta WHERE date = ?1 LIMIT 1",
                params![date_key],
                |_row| Ok(()),
            )
            .optional()
            .context("Failed to check day existence")?
            .is_some();
        Ok(exists)
    }

    fn import_legacy_day_files_tx(tx: &Transaction<'_>, data_dir: &Path) -> Result<()> {
        let entries = fs::read_dir(data_dir).context("Failed to read data directory")?;
        let mut paths = Vec::new();

        for entry in entries {
            let entry = entry.context("Failed to read directory entry")?;
            paths.push(entry.path());
        }

        paths.sort();

        for path in paths {
            if !path.is_file() {
                continue;
            }

            let Some(file_name_os) = path.file_name() else {
                continue;
            };
            let Some(file_name) = file_name_os.to_str() else {
                continue;
            };

            if !file_name.ends_with(".json") {
                continue;
            }

            if matches!(
                file_name,
                LEGACY_RUNNING_TIMER_FILE_NAME | LEGACY_ACTIVE_TIMER_FILE_NAME
            ) {
                continue;
            }

            let Some(date_part) = file_name.strip_suffix(".json") else {
                continue;
            };

            let parsed_file_date = match parse_date(date_part) {
                Ok(date) => date,
                Err(_) => continue,
            };

            let date_key = format_date(parsed_file_date);
            if Self::day_exists_tx(tx, &date_key)? {
                eprintln!(
                    "Skipping legacy JSON import for {} from {:?}: data already exists in SQLite",
                    date_key, path
                );
                continue;
            }

            let contents = fs::read_to_string(&path)
                .context(format!("Failed to read legacy JSON file: {:?}", path))?;
            let day_data: DayData = serde_json::from_str(&contents)
                .context(format!("Failed to parse legacy day JSON: {:?}", path))?;

            Self::write_day_data_tx(tx, &day_data, false)?;
        }

        Ok(())
    }

    fn import_legacy_timer_tx(tx: &Transaction<'_>, data_dir: &Path) -> Result<()> {
        let timer_exists = tx
            .query_row(
                "SELECT 1 FROM active_timer WHERE singleton_id = 1 LIMIT 1",
                [],
                |_row| Ok(()),
            )
            .optional()
            .context("Failed to check existing active timer")?
            .is_some();

        if timer_exists {
            return Ok(());
        }

        let running_timer_path = data_dir.join(LEGACY_RUNNING_TIMER_FILE_NAME);
        let active_timer_path = data_dir.join(LEGACY_ACTIVE_TIMER_FILE_NAME);

        if running_timer_path.exists() && active_timer_path.exists() {
            eprintln!(
                "Found both legacy timer files ({:?}, {:?}); preferring {:?}",
                running_timer_path, active_timer_path, running_timer_path
            );
        }

        let timer_paths = [running_timer_path, active_timer_path];

        for path in timer_paths {
            if !path.exists() {
                continue;
            }

            let contents = fs::read_to_string(&path)
                .context(format!("Failed to read legacy timer JSON: {:?}", path))?;
            let timer: TimerState = serde_json::from_str(&contents)
                .context(format!("Failed to parse legacy timer JSON: {:?}", path))?;

            Self::save_active_timer_tx(tx, &timer)?;
            break;
        }

        Ok(())
    }
}

fn format_date(date: Date) -> String {
    format!(
        "{:04}-{:02}-{:02}",
        date.year(),
        date.month() as u8,
        date.day()
    )
}

fn parse_date(value: &str) -> Result<Date> {
    Date::parse(value, format_description!("[year]-[month]-[day]"))
        .context(format!("Invalid date: '{}'. Expected YYYY-MM-DD", value))
}

fn format_datetime(value: OffsetDateTime) -> Result<String> {
    value
        .format(&Rfc3339)
        .context("Failed to serialize datetime as RFC3339")
}

fn parse_datetime(value: &str, field: &str) -> Result<OffsetDateTime> {
    OffsetDateTime::parse(value, &Rfc3339).context(format!(
        "Invalid RFC3339 datetime in {}: '{}'",
        field, value
    ))
}

fn timer_status_to_str(status: TimerStatus) -> &'static str {
    match status {
        TimerStatus::Running => "running",
        TimerStatus::Paused => "paused",
        TimerStatus::Stopped => "stopped",
    }
}

fn parse_timer_status(value: &str) -> Result<TimerStatus> {
    match value {
        "running" => Ok(TimerStatus::Running),
        "paused" => Ok(TimerStatus::Paused),
        "stopped" => Ok(TimerStatus::Stopped),
        _ => anyhow::bail!("Invalid timer status in database: {}", value),
    }
}

fn i64_to_u32(value: i64, field_name: &str) -> Result<u32> {
    u32::try_from(value).context(format!(
        "Invalid value in {}: {} (must fit in u32)",
        field_name, value
    ))
}

fn i64_to_u64(value: i64, field_name: &str) -> Result<u64> {
    u64::try_from(value).context(format!(
        "Invalid value in {}: {} (must fit in u64)",
        field_name, value
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn create_test_date() -> Date {
        Date::from_calendar_date(2025, time::Month::November, 6).unwrap()
    }

    fn create_test_record(id: u32, name: &str) -> WorkRecord {
        let start = TimePoint::new(9, 0).unwrap();
        let end = TimePoint::new(17, 0).unwrap();
        WorkRecord::new(id, name.to_string(), start, end)
    }

    fn create_test_timer() -> TimerState {
        let now = OffsetDateTime::now_utc();
        TimerState {
            id: None,
            task_name: "Test Timer".to_string(),
            description: Some("Timer description".to_string()),
            project: Some("Platform".to_string()),
            customer: Some("ACME".to_string()),
            start_time: now,
            end_time: None,
            date: now.date(),
            status: TimerStatus::Running,
            paused_duration_secs: 0,
            paused_at: None,
            created_at: now,
            updated_at: now,
            source_record_id: None,
            source_record_date: None,
        }
    }

    #[test]
    fn test_new_storage_with_temp_dir_initializes_database() {
        let temp_dir = TempDir::new().unwrap();
        let storage = Storage::new_with_dir(temp_dir.path().to_path_buf()).unwrap();

        assert!(temp_dir.path().exists());
        assert!(storage.get_db_path().exists());
    }

    #[test]
    fn test_save_and_load_day_data_round_trip() {
        let temp_dir = TempDir::new().unwrap();
        let storage = Storage::new_with_dir(temp_dir.path().to_path_buf()).unwrap();
        let date = create_test_date();

        let mut day_data = DayData::new(date);
        day_data.add_record(create_test_record(1, "Coding"));
        day_data.add_record(create_test_record(2, "Meeting"));

        storage.save(&day_data).unwrap();
        let loaded = storage.load(&date).unwrap();

        assert_eq!(loaded.date, date);
        assert_eq!(loaded.last_id, 2);
        assert_eq!(loaded.work_records.len(), 2);
        assert_eq!(loaded.work_records.get(&1).unwrap().name, "Coding");
        assert_eq!(loaded.work_records.get(&2).unwrap().name, "Meeting");
    }

    #[test]
    fn test_list_dates_with_records_only_returns_non_empty_days() {
        let temp_dir = TempDir::new().unwrap();
        let storage = Storage::new_with_dir(temp_dir.path().to_path_buf()).unwrap();

        let date1 = Date::from_calendar_date(2025, time::Month::November, 6).unwrap();
        let date2 = Date::from_calendar_date(2025, time::Month::November, 7).unwrap();
        let date3 = Date::from_calendar_date(2025, time::Month::November, 8).unwrap();

        let mut day1 = DayData::new(date1);
        day1.add_record(create_test_record(1, "Coding"));
        storage.save(&day1).unwrap();

        // Saving an empty day should not make it exportable.
        storage.save(&DayData::new(date2)).unwrap();

        let mut day3 = DayData::new(date3);
        day3.add_record(create_test_record(1, "Meeting"));
        storage.save(&day3).unwrap();

        let dates = storage.list_dates_with_records().unwrap();
        assert_eq!(dates, vec![date1, date3]);
    }

    #[test]
    fn test_recent_task_names_are_recent_unique_and_exclude_new_task() {
        let temp_dir = TempDir::new().unwrap();
        let storage = Storage::new_with_dir(temp_dir.path().to_path_buf()).unwrap();
        let today = create_test_date();
        let yesterday = today.previous_day().unwrap();
        let three_days_ago = yesterday.previous_day().unwrap().previous_day().unwrap();
        let four_days_ago = three_days_ago.previous_day().unwrap();

        let mut today_data = DayData::new(today);
        today_data.add_record(create_test_record(1, "Today Task"));
        today_data.add_record(create_test_record(2, "Repeat Task"));
        today_data.add_record(create_test_record(3, "New Task"));
        storage.save(&today_data).unwrap();

        let mut yesterday_data = DayData::new(yesterday);
        yesterday_data.add_record(create_test_record(1, "Yesterday Task"));
        yesterday_data.add_record(create_test_record(2, "Repeat Task"));
        storage.save(&yesterday_data).unwrap();

        let mut three_days_ago_data = DayData::new(three_days_ago);
        three_days_ago_data.add_record(create_test_record(1, "Three Days Ago Task"));
        storage.save(&three_days_ago_data).unwrap();

        let mut four_days_ago_data = DayData::new(four_days_ago);
        four_days_ago_data.add_record(create_test_record(1, "Too Old Task"));
        storage.save(&four_days_ago_data).unwrap();

        let task_names = storage.recent_task_names(today, 3).unwrap();

        assert_eq!(
            task_names,
            vec![
                "Repeat Task",
                "Today Task",
                "Yesterday Task",
                "Three Days Ago Task"
            ]
        );
    }

    #[test]
    fn test_get_file_modified_time_uses_revision_token() {
        let temp_dir = TempDir::new().unwrap();
        let storage = Storage::new_with_dir(temp_dir.path().to_path_buf()).unwrap();
        let date = create_test_date();

        assert!(storage.get_file_modified_time(&date).is_none());

        let mut day_data = DayData::new(date);
        day_data.add_record(create_test_record(1, "Task 1"));
        storage.save(&day_data).unwrap();
        let first = storage.get_file_modified_time(&date);

        day_data.add_record(create_test_record(2, "Task 2"));
        storage.save(&day_data).unwrap();
        let second = storage.get_file_modified_time(&date);

        assert!(first.is_some());
        assert!(second.is_some());
        assert_ne!(first, second);
    }

    #[test]
    fn test_save_load_and_clear_active_timer() {
        let temp_dir = TempDir::new().unwrap();
        let storage = Storage::new_with_dir(temp_dir.path().to_path_buf()).unwrap();
        let timer = create_test_timer();

        storage.save_active_timer(&timer).unwrap();
        let loaded = storage.load_active_timer().unwrap();
        assert!(loaded.is_some());
        let loaded_timer = loaded.unwrap();
        assert_eq!(loaded_timer.task_name, "Test Timer");
        assert_eq!(loaded_timer.project.as_deref(), Some("Platform"));
        assert_eq!(loaded_timer.customer.as_deref(), Some("ACME"));

        storage.clear_active_timer().unwrap();
        assert!(storage.load_active_timer().unwrap().is_none());
    }

    #[test]
    fn test_storage_manager_check_and_reload_detects_external_changes() {
        let temp_dir = TempDir::new().unwrap();
        let mut manager1 = StorageManager::new_with_dir(temp_dir.path().to_path_buf()).unwrap();
        let mut manager2 = StorageManager::new_with_dir(temp_dir.path().to_path_buf()).unwrap();
        let date = create_test_date();

        manager1.load_with_tracking(date).unwrap();
        manager2
            .add_record(date, create_test_record(1, "External Change"))
            .unwrap();

        let reloaded = manager1.check_and_reload(date).unwrap();
        assert!(reloaded.is_some());
        assert_eq!(reloaded.unwrap().work_records.len(), 1);
    }

    #[test]
    fn test_json_migration_imports_legacy_day_data_and_timer() {
        let temp_dir = TempDir::new().unwrap();
        let data_dir = temp_dir.path();

        let date = create_test_date();
        let mut day_data = DayData::new(date);
        day_data.add_record(create_test_record(1, "Legacy Task"));

        let day_json_path = data_dir.join("2025-11-06.json");
        fs::write(
            &day_json_path,
            serde_json::to_string_pretty(&day_data).unwrap(),
        )
        .unwrap();

        let timer = create_test_timer();
        let timer_json_path = data_dir.join(LEGACY_RUNNING_TIMER_FILE_NAME);
        fs::write(
            &timer_json_path,
            serde_json::to_string_pretty(&timer).unwrap(),
        )
        .unwrap();

        let storage = Storage::new_with_dir(data_dir.to_path_buf()).unwrap();

        let loaded_day = storage.load(&date).unwrap();
        assert_eq!(loaded_day.work_records.len(), 1);
        assert_eq!(loaded_day.work_records.get(&1).unwrap().name, "Legacy Task");

        let loaded_timer = storage.load_active_timer().unwrap();
        assert!(loaded_timer.is_some());
        assert_eq!(loaded_timer.unwrap().task_name, "Test Timer");

        // Legacy files should be preserved as backup.
        assert!(day_json_path.exists());
        assert!(timer_json_path.exists());
    }

    #[test]
    fn test_json_migration_is_idempotent() {
        let temp_dir = TempDir::new().unwrap();
        let data_dir = temp_dir.path();

        let date = create_test_date();
        let mut day_data = DayData::new(date);
        day_data.add_record(create_test_record(1, "Legacy Task"));

        let day_json_path = data_dir.join("2025-11-06.json");
        fs::write(
            &day_json_path,
            serde_json::to_string_pretty(&day_data).unwrap(),
        )
        .unwrap();

        let storage1 = Storage::new_with_dir(data_dir.to_path_buf()).unwrap();
        let loaded1 = storage1.load(&date).unwrap();
        assert_eq!(loaded1.work_records.len(), 1);

        let storage2 = Storage::new_with_dir(data_dir.to_path_buf()).unwrap();
        let loaded2 = storage2.load(&date).unwrap();
        assert_eq!(loaded2.work_records.len(), 1);
        assert_eq!(loaded2.work_records.get(&1).unwrap().name, "Legacy Task");
    }

    #[test]
    fn test_json_migration_fails_on_malformed_legacy_day_file() {
        let temp_dir = TempDir::new().unwrap();
        let data_dir = temp_dir.path();

        let malformed_day_path = data_dir.join("2025-11-08.json");
        fs::write(&malformed_day_path, "{ invalid json").unwrap();

        let result = Storage::new_with_dir(data_dir.to_path_buf());
        assert!(result.is_err());
        let err = result.err().expect("expected malformed migration to fail");
        let err_msg = format!("{err:#}");
        let malformed_path_str = malformed_day_path.display().to_string();
        assert!(
            err_msg.contains(&malformed_path_str),
            "error message '{}' did not include malformed file path '{}'",
            err_msg,
            malformed_path_str
        );

        let db_path = data_dir.join(DATABASE_FILE_NAME);
        let conn = Connection::open(db_path).unwrap();
        let day_rows: i64 = conn
            .query_row("SELECT COUNT(*) FROM day_meta", [], |row| row.get(0))
            .unwrap();
        assert_eq!(day_rows, 0);
    }

    #[test]
    fn test_json_migration_prefers_running_timer_when_both_timer_files_exist() {
        let temp_dir = TempDir::new().unwrap();
        let data_dir = temp_dir.path();

        let mut running_timer = create_test_timer();
        running_timer.task_name = "running timer file".to_string();
        fs::write(
            data_dir.join(LEGACY_RUNNING_TIMER_FILE_NAME),
            serde_json::to_string_pretty(&running_timer).unwrap(),
        )
        .unwrap();

        let mut active_timer = create_test_timer();
        active_timer.task_name = "active timer file".to_string();
        fs::write(
            data_dir.join(LEGACY_ACTIVE_TIMER_FILE_NAME),
            serde_json::to_string_pretty(&active_timer).unwrap(),
        )
        .unwrap();

        let storage = Storage::new_with_dir(data_dir.to_path_buf()).unwrap();
        let loaded = storage.load_active_timer().unwrap().unwrap();

        assert_eq!(loaded.task_name, "running timer file");
    }

    #[test]
    fn test_storage_diagnostics_reports_expected_values() {
        let temp_dir = TempDir::new().unwrap();
        let storage = Storage::new_with_dir(temp_dir.path().to_path_buf()).unwrap();

        let date = create_test_date();
        let mut day_data = DayData::new(date);
        day_data.add_record(create_test_record(1, "Task One"));
        day_data.add_record(create_test_record(2, "Task Two"));
        storage.save(&day_data).unwrap();

        fs::write(
            temp_dir.path().join("2025-11-09.json"),
            serde_json::to_string_pretty(&day_data).unwrap(),
        )
        .unwrap();

        let diagnostics = storage.diagnostics().unwrap();

        assert_eq!(diagnostics.migration_marker.as_deref(), Some("done"));
        assert_eq!(diagnostics.days_count, 1);
        assert_eq!(diagnostics.work_records_count, 2);
        assert!(!diagnostics.active_timer_present);
        assert_eq!(diagnostics.legacy_day_json_files, 1);
        assert_eq!(diagnostics.legacy_timer_json_files, 0);
    }

    #[test]
    fn test_schema_migration_adds_project_and_customer_columns() {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join(DATABASE_FILE_NAME);

        {
            let conn = Connection::open(&db_path).unwrap();
            conn.execute_batch(
                "
                CREATE TABLE meta (
                    key TEXT PRIMARY KEY,
                    value TEXT NOT NULL
                );

                CREATE TABLE day_meta (
                    date TEXT PRIMARY KEY,
                    last_id INTEGER NOT NULL DEFAULT 0 CHECK (last_id >= 0),
                    revision INTEGER NOT NULL DEFAULT 0 CHECK (revision >= 0)
                );

                CREATE TABLE work_records (
                    date TEXT NOT NULL,
                    id INTEGER NOT NULL,
                    name TEXT NOT NULL,
                    start_minutes INTEGER NOT NULL,
                    end_minutes INTEGER NOT NULL,
                    total_minutes INTEGER NOT NULL,
                    description TEXT NOT NULL DEFAULT '',
                    PRIMARY KEY (date, id),
                    FOREIGN KEY (date) REFERENCES day_meta(date) ON DELETE CASCADE
                );

                CREATE TABLE active_timer (
                    singleton_id INTEGER PRIMARY KEY CHECK (singleton_id = 1),
                    id INTEGER,
                    task_name TEXT NOT NULL,
                    description TEXT,
                    start_time TEXT NOT NULL,
                    end_time TEXT,
                    date TEXT NOT NULL,
                    status TEXT NOT NULL CHECK (status IN ('running', 'paused', 'stopped')),
                    paused_duration_secs INTEGER NOT NULL CHECK (paused_duration_secs >= 0),
                    paused_at TEXT,
                    created_at TEXT NOT NULL,
                    updated_at TEXT NOT NULL,
                    source_record_id INTEGER,
                    source_record_date TEXT
                );
                ",
            )
            .unwrap();
        }

        let _storage = Storage::new_with_dir(temp_dir.path().to_path_buf()).unwrap();
        let conn = Connection::open(db_path).unwrap();

        let work_records_has_project: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM pragma_table_info('work_records') WHERE name = 'project'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        let work_records_has_customer: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM pragma_table_info('work_records') WHERE name = 'customer'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        let active_timer_has_project: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM pragma_table_info('active_timer') WHERE name = 'project'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        let active_timer_has_customer: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM pragma_table_info('active_timer') WHERE name = 'customer'",
                [],
                |row| row.get(0),
            )
            .unwrap();

        assert_eq!(work_records_has_project, 1);
        assert_eq!(work_records_has_customer, 1);
        assert_eq!(active_timer_has_project, 1);
        assert_eq!(active_timer_has_customer, 1);
    }
}
