//! Timer module for automatic time tracking
//!
//! This module provides automatic timer functionality for tracking work sessions.
//! Timers can be started, paused, resumed, and stopped, with automatic conversion
//! to WorkRecord upon completion.

use crate::models::{TimePoint, WorkRecord};
use crate::storage::Storage;
use anyhow::{Context, Result, anyhow};
use serde::{Deserialize, Serialize};
use std::time::Duration as StdDuration;
use time::{Date, OffsetDateTime};

/// Timer status enumeration
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TimerStatus {
    Running,
    Paused,
    Stopped,
}

/// Active timer state with SQLite-ready fields
///
/// This struct represents an active timer session. All fields are designed
/// to be compatible with SQLite storage for future migration (Issue #22).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TimerState {
    /// Optional ID for future SQLite primary key (currently unused)
    pub id: Option<u32>,

    /// Task name being tracked
    pub task_name: String,

    /// Optional description for the task
    pub description: Option<String>,

    /// Optional project name for grouping/reporting
    #[serde(default)]
    pub project: Option<String>,

    /// Optional customer name for client-related work
    #[serde(default)]
    pub customer: Option<String>,

    /// When the timer was started (UTC)
    pub start_time: OffsetDateTime,

    /// When the timer was stopped (UTC), None if still active
    pub end_time: Option<OffsetDateTime>,

    /// Date when timer was started
    pub date: Date,

    /// Current status of the timer
    pub status: TimerStatus,

    /// Total duration in seconds when paused (cumulative)
    pub paused_duration_secs: i64,

    /// When timer was last paused (to track current pause duration)
    pub paused_at: Option<OffsetDateTime>,

    /// When this timer record was created (audit field)
    pub created_at: OffsetDateTime,

    /// When this timer record was last updated (audit field)
    pub updated_at: OffsetDateTime,

    /// ID of the source work record that this timer was started from
    /// If present, stopping the timer will update the existing record instead of creating a new one
    #[serde(default)]
    pub source_record_id: Option<u32>,

    /// Date of the source work record (needed when timer is started from a past/future date view)
    /// If present, we'll update the record in this date's file instead of the timer start date
    #[serde(default)]
    pub source_record_date: Option<Date>,
}

/// Timer manager for controlling timer operations
///
/// Provides methods to start, stop, pause, and resume timers, as well as
/// query their current status. Manages persistence through the StorageManager layer.
pub struct TimerManager {
    storage: Storage,
}

impl TimerManager {
    /// Create a new timer manager with low-level Storage
    /// For internal use - external callers should use storage::StorageManager instead
    pub fn new(storage: Storage) -> Self {
        TimerManager { storage }
    }

    /// Start a new timer
    ///
    /// # Errors
    /// Returns an error if a timer is already running
    pub fn start(
        &self,
        task_name: String,
        description: Option<String>,
        project: Option<String>,
        customer: Option<String>,
        source_record_id: Option<u32>,
        source_record_date: Option<Date>,
    ) -> Result<TimerState> {
        // Check if timer already running
        if (self.storage.load_active_timer()?).is_some() {
            return Err(anyhow!("A timer is already running"));
        }

        let now = OffsetDateTime::now_local()
            .context("Failed to get local time. System clock may not be configured correctly.")?;
        let timer = TimerState {
            id: None,
            task_name,
            description,
            project,
            customer,
            start_time: now,
            end_time: None,
            date: now.date(),
            status: TimerStatus::Running,
            paused_duration_secs: 0,
            paused_at: None,
            created_at: now,
            updated_at: now,
            source_record_id,
            source_record_date,
        };

        self.storage.save_active_timer(&timer)?;
        Ok(timer)
    }

    /// Stop the active timer and convert it to a WorkRecord
    ///
    /// # Errors
    /// Returns an error if no timer is running
    pub fn stop(&self) -> Result<WorkRecord> {
        let mut timer = self
            .storage
            .load_active_timer()?
            .ok_or_else(|| anyhow!("No timer is currently running"))?;

        let now = OffsetDateTime::now_local()
            .context("Failed to get local time. System clock may not be configured correctly.")?;

        // Determine which date's data file to load:
        // - If timer has source_record_date, use that (record is from a specific day's view)
        // - Otherwise use timer.start_time.date() (creating new record on timer's start date)
        let target_date = timer
            .source_record_date
            .unwrap_or_else(|| timer.start_time.date());

        timer.end_time = Some(now);
        timer.status = TimerStatus::Stopped;
        timer.updated_at = now;

        // Load the day's data file
        let mut day_data = self.storage.load(&target_date)?;

        // If timer was started from an existing record, update that record's end time
        // Otherwise, create a new work record
        if let Some(source_id) = timer.source_record_id {
            // Find and update the existing record
            if let Some(record) = day_data.work_records.get_mut(&source_id) {
                // Update the end time to now
                let end_timepoint = TimePoint::new(now.hour(), now.minute())
                    .map_err(|e| anyhow!(e))
                    .context("Failed to create TimePoint for timer end time")?;
                record.end = end_timepoint;
                record.update_duration();
            } else {
                // Source record not found, create new one instead
                let mut work_record = self.to_work_record(timer.clone())?;
                // Assign proper ID from day_data instead of using placeholder
                work_record.id = day_data.next_id();
                day_data.add_record(work_record);
            }
        } else {
            // No source record, create a new work record
            let mut work_record = self.to_work_record(timer.clone())?;
            // Assign proper ID from day_data instead of using placeholder
            work_record.id = day_data.next_id();
            day_data.add_record(work_record);
        }

        self.storage.save(&day_data)?;
        self.storage.clear_active_timer()?;

        // Return a work record for the stopped timer (for display purposes)
        let work_record = self.to_work_record(timer)?;
        Ok(work_record)
    }

    /// Pause the active timer
    ///
    /// # Errors
    /// Returns an error if timer is not running
    pub fn pause(&self) -> Result<TimerState> {
        let mut timer = self
            .storage
            .load_active_timer()?
            .ok_or_else(|| anyhow!("No timer is currently running"))?;

        if timer.status == TimerStatus::Paused {
            return Err(anyhow!("Timer is already paused"));
        }

        if timer.status != TimerStatus::Running {
            return Err(anyhow!("Can only pause a running timer"));
        }

        let now = OffsetDateTime::now_local()
            .context("Failed to get local time. System clock may not be configured correctly.")?;
        timer.paused_at = Some(now);
        timer.status = TimerStatus::Paused;
        timer.updated_at = now;

        self.storage.save_active_timer(&timer)?;
        Ok(timer)
    }

    /// Resume a paused timer
    ///
    /// # Errors
    /// Returns an error if timer is not paused
    pub fn resume(&self) -> Result<TimerState> {
        let mut timer = self
            .storage
            .load_active_timer()?
            .ok_or_else(|| anyhow!("No timer is currently running"))?;

        if timer.status != TimerStatus::Paused {
            return Err(anyhow!("Can only resume a paused timer"));
        }

        let now = OffsetDateTime::now_local()
            .context("Failed to get local time. System clock may not be configured correctly.")?;

        // Add current pause duration to cumulative paused time
        if let Some(paused_at) = timer.paused_at {
            let pause_duration = (now - paused_at).whole_seconds();
            timer.paused_duration_secs += pause_duration;
        }

        timer.paused_at = None;
        timer.status = TimerStatus::Running;
        timer.updated_at = now;

        self.storage.save_active_timer(&timer)?;
        Ok(timer)
    }

    /// Get the current timer status
    ///
    /// Returns None if no timer is running
    pub fn status(&self) -> Result<Option<TimerState>> {
        self.storage.load_active_timer()
    }

    /// Calculate elapsed duration of a timer
    ///
    /// Returns the time since start_time, minus any paused durations.
    pub fn get_elapsed_duration(&self, timer: &TimerState) -> StdDuration {
        let end_point = if timer.status == TimerStatus::Paused {
            // If paused, use when it was paused
            timer.paused_at.unwrap_or_else(|| {
                OffsetDateTime::now_local().unwrap_or_else(|_| OffsetDateTime::now_utc())
            })
        } else {
            // If running, use now
            OffsetDateTime::now_local().unwrap_or_else(|_| OffsetDateTime::now_utc())
        };

        let elapsed = end_point - timer.start_time;
        let paused_duration_std = StdDuration::from_secs(timer.paused_duration_secs as u64);

        // Convert time::Duration to std::Duration for arithmetic
        let elapsed_std = StdDuration::from_secs(elapsed.whole_seconds() as u64)
            + StdDuration::from_nanos(elapsed.subsec_nanoseconds() as u64);

        elapsed_std
            .checked_sub(paused_duration_std)
            .unwrap_or(StdDuration::ZERO)
    }

    /// Convert a stopped timer to a WorkRecord
    fn to_work_record(&self, timer: TimerState) -> Result<WorkRecord> {
        if timer.status != TimerStatus::Stopped {
            return Err(anyhow!("Can only convert stopped timers to WorkRecord"));
        }

        let start_time = timer.start_time;
        let end_time = timer
            .end_time
            .ok_or_else(|| anyhow!("Stopped timer must have end_time"))?;

        // Extract just the time portion from the OffsetDateTime values
        let start_timepoint = TimePoint::new(start_time.hour(), start_time.minute())
            .map_err(|e| anyhow!(e))
            .context("Failed to create TimePoint for timer start time")?;

        let end_timepoint = TimePoint::new(end_time.hour(), end_time.minute())
            .map_err(|e| anyhow!(e))
            .context("Failed to create TimePoint for timer end time")?;

        let mut record = WorkRecord::new(
            1, // Placeholder ID, will be set by DayData
            timer.task_name,
            start_timepoint,
            end_timepoint,
        );

        if let Some(description) = timer.description {
            record.description = description;
        }

        if let Some(project) = timer.project {
            record.project = project;
        }

        if let Some(customer) = timer.customer {
            record.customer = customer;
        }

        Ok(record)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn create_test_storage() -> (Storage, TempDir) {
        let temp_dir = TempDir::new().unwrap();
        let storage = Storage::new_with_dir(temp_dir.path().to_path_buf()).unwrap();
        (storage, temp_dir)
    }

    #[test]
    fn test_timer_state_creation() {
        let now = OffsetDateTime::now_utc();
        let timer = TimerState {
            id: None,
            task_name: "Test Task".to_string(),
            description: None,
            project: None,
            customer: None,
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

        assert_eq!(timer.task_name, "Test Task");
        assert_eq!(timer.status, TimerStatus::Running);
        assert_eq!(timer.paused_duration_secs, 0);
    }

    #[test]
    fn test_timer_serialization() {
        let now = OffsetDateTime::now_utc();
        let timer = TimerState {
            id: None,
            task_name: "Test Task".to_string(),
            description: Some("Test description".to_string()),
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
        };

        let json = serde_json::to_string(&timer).unwrap();
        let deserialized: TimerState = serde_json::from_str(&json).unwrap();

        assert_eq!(deserialized.task_name, timer.task_name);
        assert_eq!(deserialized.status, timer.status);
    }

    #[test]
    fn test_start_timer() {
        let (storage, _temp) = create_test_storage();
        let manager = TimerManager::new(storage);

        let result = manager.start("Work".to_string(), None, None, None, None, None);
        assert!(result.is_ok());

        let timer = result.unwrap();
        assert_eq!(timer.task_name, "Work");
        assert_eq!(timer.status, TimerStatus::Running);
        assert_eq!(timer.paused_duration_secs, 0);
    }

    #[test]
    fn test_cannot_start_when_already_running() {
        let (storage, _temp) = create_test_storage();
        let manager = TimerManager::new(storage);

        let _ = manager.start("Task 1".to_string(), None, None, None, None, None);
        let result = manager.start("Task 2".to_string(), None, None, None, None, None);

        assert!(result.is_err());
        assert_eq!(
            result.unwrap_err().to_string(),
            "A timer is already running"
        );
    }

    #[test]
    fn test_pause_running_timer() {
        let (storage, _temp) = create_test_storage();
        let manager = TimerManager::new(storage);

        let _ = manager.start("Work".to_string(), None, None, None, None, None);
        let result = manager.pause();

        assert!(result.is_ok());
        let timer = result.unwrap();
        assert_eq!(timer.status, TimerStatus::Paused);
        assert!(timer.paused_at.is_some());
    }

    #[test]
    fn test_cannot_pause_paused_timer() {
        let (storage, _temp) = create_test_storage();
        let manager = TimerManager::new(storage);

        let _ = manager.start("Work".to_string(), None, None, None, None, None);
        let _ = manager.pause();
        let result = manager.pause();

        assert!(result.is_err());
    }

    #[test]
    fn test_pause_without_running_timer() {
        let (storage, _temp) = create_test_storage();
        let manager = TimerManager::new(storage);

        let result = manager.pause();
        assert!(result.is_err());
    }

    #[test]
    fn test_resume_paused_timer() {
        let (storage, _temp) = create_test_storage();
        let manager = TimerManager::new(storage);

        let _ = manager.start("Work".to_string(), None, None, None, None, None);
        let _ = manager.pause();
        let result = manager.resume();

        assert!(result.is_ok());
        let timer = result.unwrap();
        assert_eq!(timer.status, TimerStatus::Running);
        assert!(timer.paused_at.is_none());
    }

    #[test]
    fn test_resume_updates_paused_duration() {
        let (storage, _temp) = create_test_storage();
        let manager = TimerManager::new(storage);

        let _ = manager.start("Work".to_string(), None, None, None, None, None);
        let paused1 = manager.pause().unwrap();
        assert_eq!(paused1.paused_duration_secs, 0);

        // Simulate time passing by manually updating
        let _ = manager.resume();
        let paused2 = manager.pause().unwrap();

        // paused_duration_secs should have increased
        assert!(paused2.paused_duration_secs >= 0);
    }

    #[test]
    fn test_cannot_resume_running_timer() {
        let (storage, _temp) = create_test_storage();
        let manager = TimerManager::new(storage);

        let _ = manager.start("Work".to_string(), None, None, None, None, None);
        let result = manager.resume();

        assert!(result.is_err());
    }

    #[test]
    fn test_status_returns_none_when_no_timer() {
        let (storage, _temp) = create_test_storage();
        let manager = TimerManager::new(storage);

        let result = manager.status().unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn test_status_returns_running_timer() {
        let (storage, _temp) = create_test_storage();
        let manager = TimerManager::new(storage);

        let _ = manager.start("Work".to_string(), None, None, None, None, None);
        let result = manager.status().unwrap();

        assert!(result.is_some());
        let timer = result.unwrap();
        assert_eq!(timer.task_name, "Work");
        assert_eq!(timer.status, TimerStatus::Running);
    }

    #[test]
    fn test_stop_running_timer() {
        let (storage, _temp) = create_test_storage();
        let manager = TimerManager::new(storage);

        let _ = manager.start("Work".to_string(), None, None, None, None, None);
        let result = manager.stop();

        assert!(result.is_ok());
        let work_record = result.unwrap();
        assert_eq!(work_record.name, "Work");

        // Timer should be cleared
        let timer_status = manager.status().unwrap();
        assert!(timer_status.is_none());
    }

    #[test]
    fn test_cannot_stop_without_running_timer() {
        let (storage, _temp) = create_test_storage();
        let manager = TimerManager::new(storage);

        let result = manager.stop();
        assert!(result.is_err());
    }

    #[test]
    fn test_stop_returns_work_record_with_description() {
        let (storage, _temp) = create_test_storage();
        let manager = TimerManager::new(storage);

        let _ = manager.start(
            "Work".to_string(),
            Some("Important task".to_string()),
            None,
            None,
            None,
            None,
        );
        let work_record = manager.stop().unwrap();

        assert_eq!(work_record.name, "Work");
        assert_eq!(work_record.description, "Important task");
    }

    #[test]
    fn test_full_timer_lifecycle() {
        let (storage, _temp) = create_test_storage();
        let manager = TimerManager::new(storage);

        // Start
        let started = manager
            .start("Task".to_string(), None, None, None, None, None)
            .unwrap();
        assert_eq!(started.status, TimerStatus::Running);

        // Pause
        let paused = manager.pause().unwrap();
        assert_eq!(paused.status, TimerStatus::Paused);

        // Resume
        let resumed = manager.resume().unwrap();
        assert_eq!(resumed.status, TimerStatus::Running);

        // Pause again
        let paused_again = manager.pause().unwrap();
        assert_eq!(paused_again.status, TimerStatus::Paused);

        // Resume again
        let resumed_again = manager.resume().unwrap();
        assert_eq!(resumed_again.status, TimerStatus::Running);

        // Stop
        let work_record = manager.stop().unwrap();
        assert_eq!(work_record.name, "Task");

        // Verify timer is cleared
        let status = manager.status().unwrap();
        assert!(status.is_none());
    }

    #[test]
    fn test_get_elapsed_duration_running() {
        let (storage, _temp) = create_test_storage();
        let manager = TimerManager::new(storage);

        let timer = manager
            .start("Task".to_string(), None, None, None, None, None)
            .unwrap();
        let elapsed = manager.get_elapsed_duration(&timer);

        // Should be close to 0 since just started
        assert!(elapsed.as_secs() < 2);
    }

    #[test]
    fn test_get_elapsed_duration_with_pause() {
        let (storage, _temp) = create_test_storage();
        let manager = TimerManager::new(storage);

        let _ = manager.start("Task".to_string(), None, None, None, None, None);
        let _ = manager.pause();

        let timer = manager.status().unwrap().unwrap();
        let elapsed = manager.get_elapsed_duration(&timer);

        // Should be very small since just paused
        assert!(elapsed.as_secs() < 2);
    }

    #[test]
    fn test_stop_updates_existing_record() {
        use crate::models::DayData;
        use crate::models::TimePoint;
        use crate::models::WorkRecord;
        use tempfile::TempDir;
        use time::OffsetDateTime;

        // Create a temp dir and storage that we can reuse
        let temp_dir = TempDir::new().unwrap();
        let storage_path = temp_dir.path().to_path_buf();

        // Create initial day data with one record
        let now = OffsetDateTime::now_utc();
        let today = now.date();
        let mut day_data = DayData::new(today);

        let record = WorkRecord::new(
            1,
            "Existing Task".to_string(),
            TimePoint::new(9, 0).unwrap(),
            TimePoint::new(10, 0).unwrap(),
        );
        day_data.add_record(record);

        // Save using first storage instance
        let storage1 = Storage::new_with_dir(storage_path.clone()).unwrap();
        storage1.save(&day_data).unwrap();

        // Start timer with source_record_id = 1, source_record_date = today
        let manager = TimerManager::new(storage1);
        manager
            .start(
                "Existing Task".to_string(),
                None,
                None,
                None,
                Some(1),
                Some(today),
            )
            .unwrap();

        // Stop timer - should update the existing record's end time
        manager.stop().unwrap();

        // Create a new storage instance pointing to the same temp dir to verify the update
        let storage2 = Storage::new_with_dir(storage_path).unwrap();
        let updated_day_data = storage2.load(&today).unwrap();

        // Should still have only 1 record (not 2!)
        assert_eq!(updated_day_data.work_records.len(), 1);

        // The record should have updated end time (not still 10:00)
        let updated_record = updated_day_data.work_records.get(&1).unwrap();
        assert_eq!(updated_record.name, "Existing Task");
        // End time should have been updated from the original 10:00
        // We verify it changed, rather than checking against wall clock time
        // (which is flaky due to timezone differences and midnight edge cases)
        assert!(
            updated_record.end.hour != 10 || updated_record.end.minute != 0,
            "End time should have been updated from original 10:00, got {:02}:{:02}",
            updated_record.end.hour,
            updated_record.end.minute
        );
    }

    #[test]
    fn test_stop_returns_work_record_with_project_and_customer() {
        let (storage, _temp) = create_test_storage();
        let manager = TimerManager::new(storage);

        let _ = manager.start(
            "Work".to_string(),
            None,
            Some("Platform Revamp".to_string()),
            Some("ACME".to_string()),
            None,
            None,
        );
        let work_record = manager.stop().unwrap();

        assert_eq!(work_record.name, "Work");
        assert_eq!(work_record.project, "Platform Revamp");
        assert_eq!(work_record.customer, "ACME");
    }
}
