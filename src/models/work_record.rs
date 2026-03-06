use super::TimePoint;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkRecord {
    pub id: u32,
    pub name: String,
    pub start: TimePoint,
    pub end: TimePoint,
    pub total_minutes: u32,
    #[serde(default)]
    pub project: String,
    #[serde(default)]
    pub customer: String,
    #[serde(default)]
    pub description: String,
}

impl WorkRecord {
    pub fn new(id: u32, name: String, start: TimePoint, end: TimePoint) -> Self {
        let total_minutes = Self::calculate_duration(&start, &end);
        WorkRecord {
            id,
            name,
            start,
            end,
            total_minutes,
            project: String::new(),
            customer: String::new(),
            description: String::new(),
        }
    }

    pub fn calculate_duration(start: &TimePoint, end: &TimePoint) -> u32 {
        let start_mins = start.to_minutes_since_midnight();
        let end_mins = end.to_minutes_since_midnight();

        if end_mins >= start_mins {
            end_mins - start_mins
        } else {
            (24 * 60 - start_mins) + end_mins
        }
    }

    pub fn update_duration(&mut self) {
        self.total_minutes = Self::calculate_duration(&self.start, &self.end);
    }

    pub fn format_duration(&self) -> String {
        let hours = self.total_minutes / 60;
        let minutes = self.total_minutes % 60;
        format!("{}h {:02}m", hours, minutes)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_work_record() {
        let start = TimePoint::new(9, 0).unwrap();
        let end = TimePoint::new(17, 0).unwrap();
        let record = WorkRecord::new(1, "Coding".to_string(), start, end);

        assert_eq!(record.id, 1);
        assert_eq!(record.name, "Coding");
        assert_eq!(record.start, start);
        assert_eq!(record.end, end);
        assert_eq!(record.total_minutes, 480); // 8 hours
        assert_eq!(record.project, "");
        assert_eq!(record.customer, "");
        assert_eq!(record.description, "");
    }

    #[test]
    fn test_calculate_duration_same_day() {
        let start = TimePoint::new(9, 0).unwrap();
        let end = TimePoint::new(17, 30).unwrap();
        let duration = WorkRecord::calculate_duration(&start, &end);
        assert_eq!(duration, 510); // 8h 30m = 510 minutes
    }

    #[test]
    fn test_calculate_duration_zero() {
        let time = TimePoint::new(12, 0).unwrap();
        let duration = WorkRecord::calculate_duration(&time, &time);
        assert_eq!(duration, 0);
    }

    #[test]
    fn test_calculate_duration_one_minute() {
        let start = TimePoint::new(12, 0).unwrap();
        let end = TimePoint::new(12, 1).unwrap();
        let duration = WorkRecord::calculate_duration(&start, &end);
        assert_eq!(duration, 1);
    }

    #[test]
    fn test_calculate_duration_overnight() {
        let start = TimePoint::new(23, 0).unwrap();
        let end = TimePoint::new(1, 0).unwrap();
        let duration = WorkRecord::calculate_duration(&start, &end);
        assert_eq!(duration, 120); // 2 hours
    }

    #[test]
    fn test_calculate_duration_overnight_complex() {
        let start = TimePoint::new(22, 30).unwrap();
        let end = TimePoint::new(2, 15).unwrap();
        let duration = WorkRecord::calculate_duration(&start, &end);
        assert_eq!(duration, 225); // 3h 45m = 225 minutes
    }

    #[test]
    fn test_calculate_duration_full_day() {
        let start = TimePoint::new(0, 0).unwrap();
        let end = TimePoint::new(0, 0).unwrap();
        let duration = WorkRecord::calculate_duration(&start, &end);
        assert_eq!(duration, 0); // Same time = 0 duration
    }

    #[test]
    fn test_calculate_duration_almost_full_day() {
        let start = TimePoint::new(0, 1).unwrap();
        let end = TimePoint::new(0, 0).unwrap();
        let duration = WorkRecord::calculate_duration(&start, &end);
        assert_eq!(duration, 1439); // 23h 59m
    }

    #[test]
    fn test_update_duration() {
        let start = TimePoint::new(9, 0).unwrap();
        let end = TimePoint::new(10, 0).unwrap();
        let mut record = WorkRecord::new(1, "Task".to_string(), start, end);
        assert_eq!(record.total_minutes, 60);

        // Change the end time
        record.end = TimePoint::new(11, 30).unwrap();
        record.update_duration();
        assert_eq!(record.total_minutes, 150); // 2h 30m
    }

    #[test]
    fn test_format_duration_zero() {
        let start = TimePoint::new(9, 0).unwrap();
        let end = TimePoint::new(9, 0).unwrap();
        let record = WorkRecord::new(1, "Task".to_string(), start, end);
        assert_eq!(record.format_duration(), "0h 00m");
    }

    #[test]
    fn test_format_duration_minutes_only() {
        let start = TimePoint::new(9, 0).unwrap();
        let end = TimePoint::new(9, 45).unwrap();
        let record = WorkRecord::new(1, "Task".to_string(), start, end);
        assert_eq!(record.format_duration(), "0h 45m");
    }

    #[test]
    fn test_format_duration_hours_only() {
        let start = TimePoint::new(9, 0).unwrap();
        let end = TimePoint::new(12, 0).unwrap();
        let record = WorkRecord::new(1, "Task".to_string(), start, end);
        assert_eq!(record.format_duration(), "3h 00m");
    }

    #[test]
    fn test_format_duration_hours_and_minutes() {
        let start = TimePoint::new(9, 15).unwrap();
        let end = TimePoint::new(17, 45).unwrap();
        let record = WorkRecord::new(1, "Task".to_string(), start, end);
        assert_eq!(record.format_duration(), "8h 30m");
    }

    #[test]
    fn test_format_duration_long() {
        let start = TimePoint::new(0, 0).unwrap();
        let end = TimePoint::new(23, 59).unwrap();
        let record = WorkRecord::new(1, "Task".to_string(), start, end);
        assert_eq!(record.format_duration(), "23h 59m");
    }

    #[test]
    fn test_description_field() {
        let start = TimePoint::new(9, 0).unwrap();
        let end = TimePoint::new(10, 0).unwrap();
        let mut record = WorkRecord::new(1, "Task".to_string(), start, end);

        assert_eq!(record.description, "");
        record.description = "Important meeting notes".to_string();
        assert_eq!(record.description, "Important meeting notes");
    }

    #[test]
    fn test_project_and_customer_fields() {
        let start = TimePoint::new(9, 0).unwrap();
        let end = TimePoint::new(10, 0).unwrap();
        let mut record = WorkRecord::new(1, "Task".to_string(), start, end);

        assert_eq!(record.project, "");
        assert_eq!(record.customer, "");

        record.project = "Internal Platform".to_string();
        record.customer = "ACME".to_string();

        assert_eq!(record.project, "Internal Platform");
        assert_eq!(record.customer, "ACME");
    }

    #[test]
    fn test_clone() {
        let start = TimePoint::new(9, 0).unwrap();
        let end = TimePoint::new(17, 0).unwrap();
        let record1 = WorkRecord::new(1, "Coding".to_string(), start, end);
        let record2 = record1.clone();

        assert_eq!(record1.id, record2.id);
        assert_eq!(record1.name, record2.name);
        assert_eq!(record1.start, record2.start);
        assert_eq!(record1.end, record2.end);
        assert_eq!(record1.total_minutes, record2.total_minutes);
        assert_eq!(record1.project, record2.project);
        assert_eq!(record1.customer, record2.customer);
    }
}
