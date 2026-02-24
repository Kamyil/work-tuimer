use anyhow::Result;
use rusqlite::{Connection, OptionalExtension, params};
use tempfile::TempDir;
use time::{Date, OffsetDateTime};
use work_tuimer::models::{DayData, TimePoint, WorkRecord};
use work_tuimer::storage::Storage;
use work_tuimer::timer::{TimerState, TimerStatus};

fn create_test_record(id: u32, name: &str, start_hour: u8, end_hour: u8) -> WorkRecord {
    let start = TimePoint::new(start_hour, 0).unwrap();
    let end = TimePoint::new(end_hour, 0).unwrap();
    WorkRecord::new(id, name.to_string(), start, end)
}

#[test]
fn test_sqlite_day_data_persistence_is_queryable() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let storage = Storage::new_with_dir(temp_dir.path().to_path_buf())?;

    let date = Date::from_calendar_date(2025, time::Month::November, 7).unwrap();
    let mut day_data = DayData::new(date);
    day_data.add_record(create_test_record(1, "Coding", 9, 11));
    day_data.add_record(create_test_record(2, "Review", 11, 12));

    storage.save(&day_data)?;

    let db_path = temp_dir.path().join("work-tuimer.db");
    let conn = Connection::open(db_path)?;

    let date_key = "2025-11-07";
    let record_count: i64 = conn.query_row(
        "SELECT COUNT(*) FROM work_records WHERE date = ?1",
        params![date_key],
        |row| row.get(0),
    )?;
    assert_eq!(record_count, 2);

    let (name, total_minutes): (String, i64) = conn.query_row(
        "SELECT name, total_minutes FROM work_records WHERE date = ?1 AND id = 1",
        params![date_key],
        |row| Ok((row.get(0)?, row.get(1)?)),
    )?;
    assert_eq!(name, "Coding");
    assert_eq!(total_minutes, 120);

    let (last_id, revision): (i64, i64) = conn.query_row(
        "SELECT last_id, revision FROM day_meta WHERE date = ?1",
        params![date_key],
        |row| Ok((row.get(0)?, row.get(1)?)),
    )?;
    assert_eq!(last_id, 2);
    assert!(revision >= 1);

    Ok(())
}

#[test]
fn test_sqlite_active_timer_persistence_is_queryable() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let storage = Storage::new_with_dir(temp_dir.path().to_path_buf())?;

    let now = OffsetDateTime::now_utc();
    let timer = TimerState {
        id: None,
        task_name: "DB Timer Test".to_string(),
        description: Some("Timer persisted in SQLite".to_string()),
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
    };

    storage.save_active_timer(&timer)?;

    let db_path = temp_dir.path().join("work-tuimer.db");
    let conn = Connection::open(db_path)?;

    let (task_name, status, description): (String, String, Option<String>) = conn.query_row(
        "SELECT task_name, status, description FROM active_timer WHERE singleton_id = 1",
        [],
        |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
    )?;

    assert_eq!(task_name, "DB Timer Test");
    assert_eq!(status, "running");
    assert_eq!(description, Some("Timer persisted in SQLite".to_string()));

    storage.clear_active_timer()?;

    let exists = conn
        .query_row(
            "SELECT 1 FROM active_timer WHERE singleton_id = 1",
            [],
            |_row| Ok(()),
        )
        .optional()?;
    assert!(exists.is_none());

    Ok(())
}
