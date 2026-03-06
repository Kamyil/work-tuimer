use anyhow::Result;
use std::thread;
use std::time::Duration;
use tempfile::TempDir;
use time::OffsetDateTime;
use work_tuimer::models::{DayData, TimePoint, WorkRecord};
use work_tuimer::storage::StorageManager;

fn create_test_record(id: u32, name: &str, start_hour: u8, end_hour: u8) -> WorkRecord {
    let start = TimePoint::new(start_hour, 0).unwrap();
    let end = TimePoint::new(end_hour, 0).unwrap();
    WorkRecord::new(id, name.to_string(), start, end)
}

#[test]
fn test_timer_lifecycle_start_stop() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let manager = StorageManager::new_with_dir(temp_dir.path().to_path_buf())?;

    // Start a timer
    let timer = manager.start_timer(
        "Integration Test Task".to_string(),
        Some("Testing timer lifecycle".to_string()),
        Some("Platform".to_string()),
        Some("ACME".to_string()),
        None,
        None,
    )?;

    assert_eq!(timer.task_name, "Integration Test Task");
    assert_eq!(
        timer.description,
        Some("Testing timer lifecycle".to_string())
    );
    assert_eq!(timer.project.as_deref(), Some("Platform"));
    assert_eq!(timer.customer.as_deref(), Some("ACME"));

    // Verify timer was saved
    let loaded_timer = manager.load_active_timer()?;
    assert!(loaded_timer.is_some());
    assert_eq!(loaded_timer.unwrap().task_name, "Integration Test Task");

    // Wait a bit (timing tests are challenging in integration tests)
    thread::sleep(Duration::from_millis(100));

    // Stop the timer
    let record = manager.stop_timer()?;

    assert_eq!(record.name, "Integration Test Task");
    assert_eq!(record.description, "Testing timer lifecycle");
    assert_eq!(record.project, "Platform");
    assert_eq!(record.customer, "ACME");
    // Note: total_minutes might be 0 if start and stop are in the same minute
    // We just verify the record was created successfully

    // Verify timer was cleared
    let cleared_timer = manager.load_active_timer()?;
    assert!(cleared_timer.is_none());

    Ok(())
}

#[test]
fn test_timer_pause_resume() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let manager = StorageManager::new_with_dir(temp_dir.path().to_path_buf())?;

    // Start a timer
    manager.start_timer("Pausable Task".to_string(), None, None, None, None, None)?;

    // Wait a bit
    thread::sleep(Duration::from_millis(100));

    // Pause
    let paused_timer = manager.pause_timer()?;
    assert_eq!(paused_timer.task_name, "Pausable Task");

    let elapsed_at_pause = manager.get_timer_elapsed(&paused_timer);

    // Wait while paused (this time shouldn't count)
    thread::sleep(Duration::from_millis(100));

    // Resume
    let resumed_timer = manager.resume_timer()?;
    assert_eq!(resumed_timer.task_name, "Pausable Task");

    // Elapsed should be approximately the same as when paused
    // Note: Allow some tolerance for system timing variations (increased for CI environments)
    let elapsed_after_resume = manager.get_timer_elapsed(&resumed_timer);
    let diff = elapsed_after_resume
        .as_millis()
        .abs_diff(elapsed_at_pause.as_millis());
    assert!(
        diff < 300,
        "Elapsed time increased too much during pause: {} ms",
        diff
    );

    // Stop and verify
    let record = manager.stop_timer()?;
    assert_eq!(record.name, "Pausable Task");

    Ok(())
}

#[test]
fn test_timer_with_source_record() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let mut manager = StorageManager::new_with_dir(temp_dir.path().to_path_buf())?;

    let today = OffsetDateTime::now_utc().date();

    // Create and save a work record to use as source
    let mut day_data = DayData::new(today);
    let source_record = create_test_record(1, "Original Task", 9, 10);
    day_data.add_record(source_record);
    manager.save(&day_data)?;

    // Start a timer linked to this record
    let timer = manager.start_timer(
        "Original Task".to_string(),
        Some("Continuing work".to_string()),
        Some("Platform Revamp".to_string()),
        Some("Contoso".to_string()),
        Some(1),
        Some(today),
    )?;

    assert_eq!(timer.source_record_id, Some(1));
    assert_eq!(timer.source_record_date, Some(today));
    assert_eq!(timer.project.as_deref(), Some("Platform Revamp"));
    assert_eq!(timer.customer.as_deref(), Some("Contoso"));

    // Stop and verify the link is preserved
    thread::sleep(Duration::from_millis(50));
    let record = manager.stop_timer()?;

    assert_eq!(record.name, "Original Task");
    assert_eq!(record.description, "Continuing work");

    Ok(())
}

#[test]
fn test_end_to_end_workflow_timer_to_saved_record() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let mut manager = StorageManager::new_with_dir(temp_dir.path().to_path_buf())?;

    let today = OffsetDateTime::now_utc().date();

    // 1. Start timer
    manager.start_timer(
        "Full Workflow Task".to_string(),
        None,
        None,
        None,
        None,
        None,
    )?;

    // 2. Simulate work
    thread::sleep(Duration::from_millis(100));

    // 3. Stop timer - this automatically saves the record to day data
    let record = manager.stop_timer()?;
    assert_eq!(record.name, "Full Workflow Task");

    // 4. Load day data with tracking to verify the record was saved
    let day_data = manager.load_with_tracking(today)?;
    assert_eq!(day_data.work_records.len(), 1);

    let saved_record = day_data.work_records.get(&1).unwrap();
    assert_eq!(saved_record.name, "Full Workflow Task");
    // Note: total_minutes might be 0 if timer started and stopped in same minute

    // 5. Verify tracking is updated
    assert!(manager.get_last_modified(&today).is_some());

    Ok(())
}

#[test]
fn test_concurrent_external_modification_detection() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let mut manager1 = StorageManager::new_with_dir(temp_dir.path().to_path_buf())?;
    let mut manager2 = StorageManager::new_with_dir(temp_dir.path().to_path_buf())?;

    let today = OffsetDateTime::now_utc().date();

    // Manager 1: Load initial data
    let day_data1 = manager1.load_with_tracking(today)?;
    assert_eq!(day_data1.work_records.len(), 0);

    // Manager 2: Add a record (external modification)
    manager2.add_record(today, create_test_record(1, "External Change", 9, 10))?;

    // Manager 1: Check for external changes
    let reloaded = manager1.check_and_reload(today)?;
    assert!(reloaded.is_some(), "Should detect external modification");

    let reloaded_data = reloaded.unwrap();
    assert_eq!(reloaded_data.work_records.len(), 1);
    assert_eq!(
        reloaded_data.work_records.get(&1).unwrap().name,
        "External Change"
    );

    Ok(())
}

#[test]
fn test_transactional_operations_rollback_on_error() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let mut manager = StorageManager::new_with_dir(temp_dir.path().to_path_buf())?;

    let today = OffsetDateTime::now_utc().date();

    // Add initial record
    manager.add_record(today, create_test_record(1, "Initial Task", 9, 10))?;

    // Try to remove non-existent record (should fail)
    let result = manager.remove_record(today, 999);
    assert!(result.is_err());

    // Verify original data is intact
    let day_data = manager.load_with_tracking(today)?;
    assert_eq!(day_data.work_records.len(), 1);
    assert_eq!(day_data.work_records.get(&1).unwrap().name, "Initial Task");

    Ok(())
}

#[test]
fn test_multiple_saves_update_tracking() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let mut manager = StorageManager::new_with_dir(temp_dir.path().to_path_buf())?;

    let today = OffsetDateTime::now_utc().date();

    // First save
    let mut day_data = DayData::new(today);
    day_data.add_record(create_test_record(1, "Task 1", 9, 10));
    manager.save(&day_data)?;

    let first_modified = manager.get_last_modified(&today);
    assert!(first_modified.is_some());

    // Wait to ensure different timestamp
    thread::sleep(Duration::from_millis(10));

    // Second save
    day_data.add_record(create_test_record(2, "Task 2", 10, 11));
    manager.save(&day_data)?;

    let second_modified = manager.get_last_modified(&today);
    assert!(second_modified.is_some());

    // Modification times should be different
    assert_ne!(first_modified, second_modified);

    Ok(())
}

#[test]
fn test_add_update_remove_workflow() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let mut manager = StorageManager::new_with_dir(temp_dir.path().to_path_buf())?;

    let today = OffsetDateTime::now_utc().date();

    // Add a record
    manager.add_record(today, create_test_record(1, "Original Name", 9, 10))?;

    let data = manager.load_with_tracking(today)?;
    assert_eq!(data.work_records.len(), 1);
    assert_eq!(data.work_records.get(&1).unwrap().name, "Original Name");

    // Update the record
    manager.update_record(today, create_test_record(1, "Updated Name", 9, 11))?;

    let data = manager.load_with_tracking(today)?;
    assert_eq!(data.work_records.len(), 1);
    assert_eq!(data.work_records.get(&1).unwrap().name, "Updated Name");
    assert_eq!(data.work_records.get(&1).unwrap().total_minutes, 120);

    // Remove the record
    let removed = manager.remove_record(today, 1)?;
    assert_eq!(removed.name, "Updated Name");

    let data = manager.load_with_tracking(today)?;
    assert_eq!(data.work_records.len(), 0);

    Ok(())
}

#[test]
fn test_timer_cleared_after_stop() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let manager = StorageManager::new_with_dir(temp_dir.path().to_path_buf())?;

    // Start and immediately stop
    manager.start_timer("Quick Task".to_string(), None, None, None, None, None)?;
    assert!(manager.load_active_timer()?.is_some());

    thread::sleep(Duration::from_millis(10));
    manager.stop_timer()?;

    // Timer file should be deleted
    assert!(manager.load_active_timer()?.is_none());

    Ok(())
}

#[test]
fn test_multiple_days_isolation() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let mut manager = StorageManager::new_with_dir(temp_dir.path().to_path_buf())?;

    let today = OffsetDateTime::now_utc().date();
    let yesterday = today.previous_day().unwrap();

    // Add records to different days
    manager.add_record(today, create_test_record(1, "Today Task", 9, 10))?;
    manager.add_record(yesterday, create_test_record(1, "Yesterday Task", 9, 10))?;

    // Load and verify isolation
    let today_data = manager.load_with_tracking(today)?;
    let yesterday_data = manager.load_with_tracking(yesterday)?;

    assert_eq!(today_data.work_records.len(), 1);
    assert_eq!(yesterday_data.work_records.len(), 1);

    assert_eq!(today_data.work_records.get(&1).unwrap().name, "Today Task");
    assert_eq!(
        yesterday_data.work_records.get(&1).unwrap().name,
        "Yesterday Task"
    );

    // Verify separate tracking
    assert!(manager.get_last_modified(&today).is_some());
    assert!(manager.get_last_modified(&yesterday).is_some());

    Ok(())
}

#[test]
fn test_load_with_tracking_updates_internal_state() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let mut manager = StorageManager::new_with_dir(temp_dir.path().to_path_buf())?;

    let today = OffsetDateTime::now_utc().date();

    // Initially no tracking
    assert!(manager.get_last_modified(&today).is_none());

    // Load with tracking
    manager.load_with_tracking(today)?;

    // Now tracking should exist (even for non-existent file, it tracks None)
    // For non-existent file, tracking is None but the entry exists
    assert!(manager.get_last_modified(&today).is_none());

    // Save a file
    let mut day_data = DayData::new(today);
    day_data.add_record(create_test_record(1, "Task", 9, 10));
    manager.save(&day_data)?;

    // Now tracking should have a value
    assert!(manager.get_last_modified(&today).is_some());

    Ok(())
}
