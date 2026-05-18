use super::history::History;
use crate::config::{Config, Theme};
use crate::models::{DayData, WorkRecord};
use crate::timer::TimerState;
use time::Date;

pub enum AppMode {
    Browse,
    Edit,
    Visual,
    CommandPalette,
    Calendar,
    TaskPicker,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EditField {
    Name,
    Start,
    End,
    Project,
    Customer,
    Description,
}

pub struct Command {
    pub key: &'static str,
    pub description: &'static str,
    pub action: CommandAction,
}

#[derive(Debug, Clone, Copy)]
pub enum CommandAction {
    MoveUp,
    MoveDown,
    MoveLeft,
    MoveRight,
    Edit,
    Change,
    New,
    Break,
    Delete,
    Visual,
    SetNow,
    Undo,
    Redo,
    Save,
    StartTimer,
    PauseTimer,
    Quit,
}

pub struct AppState {
    pub day_data: DayData,
    pub current_date: Date,
    pub mode: AppMode,
    pub selected_index: usize,
    pub edit_field: EditField,
    pub input_buffer: String,
    pub time_cursor: usize,
    pub should_quit: bool,
    pub visual_start: usize,
    pub visual_end: usize,
    pub command_palette_input: String,
    pub command_palette_selected: usize,
    pub available_commands: Vec<Command>,
    pub date_changed: bool,
    pub calendar_selected_date: Date,
    pub calendar_view_month: time::Month,
    pub calendar_view_year: i32,
    pub config: Config,
    pub theme: Theme,
    pub last_error_message: Option<String>,
    pub task_picker_selected: usize,
    pub task_picker_tasks: Vec<String>,
    pub active_timer: Option<TimerState>,
    pub last_file_modified: Option<std::time::SystemTime>,
    history: History,
}

impl AppState {
    pub fn new(day_data: DayData) -> Self {
        let current_date = day_data.date;
        let available_commands = vec![
            Command {
                key: "↑/k",
                description: "Move selection up",
                action: CommandAction::MoveUp,
            },
            Command {
                key: "↓/j",
                description: "Move selection down",
                action: CommandAction::MoveDown,
            },
            Command {
                key: "←/h",
                description: "Move field left",
                action: CommandAction::MoveLeft,
            },
            Command {
                key: "→/l",
                description: "Move field right",
                action: CommandAction::MoveRight,
            },
            Command {
                key: "Enter/i",
                description: "Enter edit mode",
                action: CommandAction::Edit,
            },
            Command {
                key: "c",
                description: "Change task name",
                action: CommandAction::Change,
            },
            Command {
                key: "n",
                description: "Add new task",
                action: CommandAction::New,
            },
            Command {
                key: "b",
                description: "Add break",
                action: CommandAction::Break,
            },
            Command {
                key: "d",
                description: "Delete selected record",
                action: CommandAction::Delete,
            },
            Command {
                key: "v",
                description: "Enter visual mode",
                action: CommandAction::Visual,
            },
            Command {
                key: "t",
                description: "Set current time on field",
                action: CommandAction::SetNow,
            },
            Command {
                key: "u",
                description: "Undo last change",
                action: CommandAction::Undo,
            },
            Command {
                key: "r",
                description: "Redo last change",
                action: CommandAction::Redo,
            },
            Command {
                key: "s",
                description: "Save to file",
                action: CommandAction::Save,
            },
            Command {
                key: "S",
                description: "Start/Stop session (toggle)",
                action: CommandAction::StartTimer,
            },
            Command {
                key: "P",
                description: "Pause/Resume active session",
                action: CommandAction::PauseTimer,
            },
            Command {
                key: "q",
                description: "Quit application",
                action: CommandAction::Quit,
            },
        ];

        let config = Config::load().unwrap_or_default();
        let theme = config.get_theme();

        AppState {
            calendar_selected_date: current_date,
            calendar_view_month: current_date.month(),
            calendar_view_year: current_date.year(),
            day_data,
            current_date,
            mode: AppMode::Browse,
            selected_index: 0,
            edit_field: EditField::Name,
            input_buffer: String::new(),
            time_cursor: 0,
            should_quit: false,
            visual_start: 0,
            visual_end: 0,
            command_palette_input: String::new(),
            command_palette_selected: 0,
            available_commands,
            date_changed: false,
            config,
            theme,
            last_error_message: None,
            task_picker_selected: 0,
            task_picker_tasks: Vec::new(),
            active_timer: None,
            last_file_modified: None,
            history: History::new(),
        }
    }

    pub fn get_selected_record(&self) -> Option<&WorkRecord> {
        let records = self.day_data.get_sorted_records();
        records.get(self.selected_index).copied()
    }

    pub fn show_project_column(&self) -> bool {
        self.config.columns.project
    }

    pub fn show_customer_column(&self) -> bool {
        self.config.columns.customer
    }

    pub fn show_description_column(&self) -> bool {
        self.config.columns.description
    }

    pub fn visible_edit_fields(&self) -> Vec<EditField> {
        let mut fields = vec![EditField::Name, EditField::Start, EditField::End];
        if self.show_project_column() {
            fields.push(EditField::Project);
        }
        if self.show_customer_column() {
            fields.push(EditField::Customer);
        }
        if self.show_description_column() {
            fields.push(EditField::Description);
        }
        fields
    }

    fn normalize_edit_field(&mut self) {
        if !self.visible_edit_fields().contains(&self.edit_field) {
            self.edit_field = EditField::Name;
        }
    }

    fn set_input_from_field(&mut self, record: &WorkRecord, field: EditField) {
        self.input_buffer = match field {
            EditField::Name => record.name.clone(),
            EditField::Start => record.start.to_string(),
            EditField::End => record.end.to_string(),
            EditField::Project => record.project.clone(),
            EditField::Customer => record.customer.clone(),
            EditField::Description => record.description.clone(),
        };
        self.time_cursor = 0;
    }

    fn step_field(&mut self, direction: i32) {
        let fields = self.visible_edit_fields();
        if fields.is_empty() {
            return;
        }

        let current_index = fields
            .iter()
            .position(|field| *field == self.edit_field)
            .unwrap_or(0);
        let len = fields.len() as i32;
        let next_index = (current_index as i32 + direction).rem_euclid(len) as usize;
        self.edit_field = fields[next_index];

        if let Some(record) = self.get_selected_record().cloned() {
            self.set_input_from_field(&record, self.edit_field);
        }
    }

    pub fn move_selection_up(&mut self) {
        if self.selected_index > 0 {
            self.selected_index -= 1;
        }
        if matches!(self.mode, AppMode::Visual) {
            self.visual_end = self.selected_index;
        }
    }

    pub fn move_selection_down(&mut self) {
        let record_count = self.day_data.work_records.len();
        if self.selected_index < record_count.saturating_sub(1) {
            self.selected_index += 1;
        }
        if matches!(self.mode, AppMode::Visual) {
            self.visual_end = self.selected_index;
        }
    }

    pub fn enter_edit_mode(&mut self) {
        self.normalize_edit_field();
        if let Some(record) = self.get_selected_record().cloned() {
            self.mode = AppMode::Edit;
            self.set_input_from_field(&record, self.edit_field);
        }
    }

    pub fn change_task_name(&mut self, task_names: Vec<String>) {
        if matches!(self.edit_field, EditField::Name) && self.get_selected_record().is_some() {
            // Check if there are any existing tasks to pick from
            if !task_names.is_empty() {
                // Open task picker if tasks exist
                self.input_buffer.clear();
                self.task_picker_selected = 0;
                self.task_picker_tasks = task_names;
                self.mode = AppMode::TaskPicker;
            } else {
                // Go directly to edit mode if no tasks exist
                self.mode = AppMode::Edit;
                self.input_buffer.clear();
                self.time_cursor = 0;
            }
        }
    }

    pub fn exit_edit_mode(&mut self) {
        self.mode = AppMode::Browse;
        self.input_buffer.clear();
        self.edit_field = EditField::Name;
        self.time_cursor = 0;
    }

    pub fn next_field(&mut self) {
        self.step_field(1);
    }

    pub fn handle_char_input(&mut self, c: char) {
        match self.edit_field {
            EditField::Name | EditField::Project | EditField::Customer | EditField::Description => {
                self.input_buffer.push(c);
            }
            EditField::Start | EditField::End => {
                if !c.is_ascii_digit() {
                    return;
                }

                if self.input_buffer.len() != 5 {
                    return;
                }

                let positions = [0, 1, 3, 4];
                if self.time_cursor >= positions.len() {
                    return;
                }

                let pos = positions[self.time_cursor];
                let mut chars: Vec<char> = self.input_buffer.chars().collect();
                chars[pos] = c;
                self.input_buffer = chars.into_iter().collect();

                self.time_cursor += 1;

                if self.time_cursor >= positions.len() && self.save_current_field().is_ok() {
                    self.exit_edit_mode();
                }
            }
        }
    }

    pub fn handle_backspace(&mut self) {
        match self.edit_field {
            EditField::Name | EditField::Project | EditField::Customer | EditField::Description => {
                self.input_buffer.pop();
            }
            EditField::Start | EditField::End => {
                if self.time_cursor > 0 {
                    self.time_cursor -= 1;
                }
            }
        }
    }

    fn save_current_field(&mut self) -> Result<(), String> {
        let records = self.day_data.get_sorted_records();
        if let Some(&record) = records.get(self.selected_index) {
            let id = record.id;

            if let Some(record_mut) = self.day_data.work_records.get_mut(&id) {
                match self.edit_field {
                    EditField::Name => {
                        if self.input_buffer.trim().is_empty() {
                            return Err("Name cannot be empty".to_string());
                        }
                        record_mut.name = self.input_buffer.trim().to_string();
                    }
                    EditField::Start => {
                        record_mut.start = self
                            .input_buffer
                            .parse()
                            .map_err(|_| "Invalid start time format (use HH:MM)".to_string())?;
                        record_mut.update_duration();
                    }
                    EditField::End => {
                        record_mut.end = self
                            .input_buffer
                            .parse()
                            .map_err(|_| "Invalid end time format (use HH:MM)".to_string())?;
                        record_mut.update_duration();
                    }
                    EditField::Project => {
                        record_mut.project = self.input_buffer.trim().to_string();
                    }
                    EditField::Customer => {
                        record_mut.customer = self.input_buffer.trim().to_string();
                    }
                    EditField::Description => {
                        record_mut.description = self.input_buffer.trim().to_string();
                    }
                }
            }
        }
        Ok(())
    }

    pub fn save_edit(&mut self) -> Result<(), String> {
        self.save_snapshot();
        self.save_current_field()?;
        self.exit_edit_mode();
        Ok(())
    }

    pub fn add_new_record(&mut self) {
        use crate::models::{TimePoint, WorkRecord};

        self.save_snapshot();

        let id = self.day_data.next_id();

        let (default_start, default_end) = if let Some(current_record) = self.get_selected_record()
        {
            let start_minutes = current_record.end.to_minutes_since_midnight();
            let end_minutes = (start_minutes + 60).min(24 * 60 - 1);
            (
                current_record.end,
                TimePoint::from_minutes_since_midnight(end_minutes).unwrap(),
            )
        } else {
            (
                TimePoint::new(9, 0).unwrap(),
                TimePoint::new(10, 0).unwrap(),
            )
        };

        let record = WorkRecord::new(id, "New Task".to_string(), default_start, default_end);

        self.day_data.add_record(record);

        let records = self.day_data.get_sorted_records();
        self.selected_index = records.iter().position(|r| r.id == id).unwrap_or(0);
    }

    pub fn add_break(&mut self) {
        use crate::models::{TimePoint, WorkRecord};

        self.save_snapshot();

        let id = self.day_data.next_id();

        let (default_start, default_end) = if let Some(current_record) = self.get_selected_record()
        {
            let start_minutes = current_record.end.to_minutes_since_midnight();
            let end_minutes = (start_minutes + 15).min(24 * 60 - 1);
            (
                current_record.end,
                TimePoint::from_minutes_since_midnight(end_minutes).unwrap(),
            )
        } else {
            (
                TimePoint::new(12, 0).unwrap(),
                TimePoint::new(12, 15).unwrap(),
            )
        };

        let record = WorkRecord::new(id, "Break".to_string(), default_start, default_end);

        self.day_data.add_record(record);

        let records = self.day_data.get_sorted_records();
        self.selected_index = records.iter().position(|r| r.id == id).unwrap_or(0);
    }

    pub fn delete_selected_record(&mut self) {
        self.save_snapshot();

        let records = self.day_data.get_sorted_records();
        if let Some(&record) = records.get(self.selected_index) {
            self.day_data.remove_record(record.id);

            if self.selected_index >= self.day_data.work_records.len() {
                self.selected_index = self.day_data.work_records.len().saturating_sub(1);
            }
        }
    }

    pub fn move_field_left(&mut self) {
        self.normalize_edit_field();
        self.step_field(-1);
    }

    pub fn move_field_right(&mut self) {
        self.normalize_edit_field();
        self.step_field(1);
    }

    pub fn set_current_time_on_field(&mut self) {
        use time::{OffsetDateTime, UtcOffset};

        self.save_snapshot();

        let local_offset = UtcOffset::current_local_offset().unwrap_or(UtcOffset::UTC);
        let now = OffsetDateTime::now_utc().to_offset(local_offset);
        let current_time = format!("{:02}:{:02}", now.hour(), now.minute());

        let records = self.day_data.get_sorted_records();
        if let Some(&record) = records.get(self.selected_index) {
            let id = record.id;

            if let Some(record_mut) = self.day_data.work_records.get_mut(&id) {
                match self.edit_field {
                    EditField::Start => {
                        if let Ok(time_point) = current_time.parse() {
                            record_mut.start = time_point;
                            record_mut.update_duration();
                        }
                    }
                    EditField::End => {
                        if let Ok(time_point) = current_time.parse() {
                            record_mut.end = time_point;
                            record_mut.update_duration();
                        }
                    }
                    _ => {}
                }
            }
        }
    }

    pub fn enter_visual_mode(&mut self) {
        self.mode = AppMode::Visual;
        self.visual_start = self.selected_index;
        self.visual_end = self.selected_index;
    }

    pub fn exit_visual_mode(&mut self) {
        self.mode = AppMode::Browse;
    }

    pub fn is_in_visual_selection(&self, index: usize) -> bool {
        let start = self.visual_start.min(self.visual_end);
        let end = self.visual_start.max(self.visual_end);

        index >= start && index <= end
    }

    pub fn delete_visual_selection(&mut self) {
        self.save_snapshot();

        let records = self.day_data.get_sorted_records();
        let start = self.visual_start.min(self.visual_end);
        let end = self.visual_start.max(self.visual_end);

        let ids_to_delete: Vec<u32> = records
            .iter()
            .enumerate()
            .filter(|(i, _)| *i >= start && *i <= end)
            .map(|(_, record)| record.id)
            .collect();

        for id in ids_to_delete {
            self.day_data.remove_record(id);
        }

        if self.selected_index >= self.day_data.work_records.len() {
            self.selected_index = self.day_data.work_records.len().saturating_sub(1);
        }

        self.exit_visual_mode();
    }

    fn save_snapshot(&mut self) {
        self.history.push(self.day_data.clone());
    }

    pub fn undo(&mut self) {
        if let Some(previous_state) = self.history.undo(self.day_data.clone()) {
            self.day_data = previous_state;

            if self.selected_index >= self.day_data.work_records.len() {
                self.selected_index = self.day_data.work_records.len().saturating_sub(1);
            }
        }
    }

    pub fn redo(&mut self) {
        if let Some(next_state) = self.history.redo(self.day_data.clone()) {
            self.day_data = next_state;

            if self.selected_index >= self.day_data.work_records.len() {
                self.selected_index = self.day_data.work_records.len().saturating_sub(1);
            }
        }
    }

    pub fn open_command_palette(&mut self) {
        self.mode = AppMode::CommandPalette;
        self.command_palette_input.clear();
        self.command_palette_selected = 0;
    }

    pub fn close_command_palette(&mut self) {
        self.mode = AppMode::Browse;
        self.command_palette_input.clear();
        self.command_palette_selected = 0;
    }

    pub fn handle_command_palette_char(&mut self, c: char) {
        self.command_palette_input.push(c);
        self.command_palette_selected = 0;
    }

    pub fn handle_command_palette_backspace(&mut self) {
        self.command_palette_input.pop();
        self.command_palette_selected = 0;
    }

    pub fn move_command_palette_up(&mut self) {
        if self.command_palette_selected > 0 {
            self.command_palette_selected -= 1;
        }
    }

    pub fn move_command_palette_down(&mut self, filtered_count: usize) {
        if self.command_palette_selected < filtered_count.saturating_sub(1) {
            self.command_palette_selected += 1;
        }
    }

    pub fn get_filtered_commands(&self) -> Vec<(usize, i64, &Command)> {
        use fuzzy_matcher::FuzzyMatcher;
        use fuzzy_matcher::skim::SkimMatcherV2;

        let matcher = SkimMatcherV2::default();
        let query = self.command_palette_input.as_str();

        if query.is_empty() {
            return self
                .available_commands
                .iter()
                .enumerate()
                .map(|(i, cmd)| (i, 0, cmd))
                .collect();
        }

        let mut results: Vec<(usize, i64, &Command)> = self
            .available_commands
            .iter()
            .enumerate()
            .filter_map(|(i, cmd)| {
                let search_text = format!("{} {}", cmd.key, cmd.description);
                matcher
                    .fuzzy_match(&search_text, query)
                    .map(|score| (i, score, cmd))
            })
            .collect();

        results.sort_by(|a, b| b.1.cmp(&a.1));
        results
    }

    pub fn execute_selected_command(&mut self) -> Option<CommandAction> {
        let filtered = self.get_filtered_commands();
        let action = filtered
            .get(self.command_palette_selected)
            .map(|(_, _, cmd)| cmd.action);
        self.close_command_palette();
        action
    }

    pub fn navigate_to_previous_day(&mut self) {
        use time::Duration;

        self.current_date = self.current_date.saturating_sub(Duration::days(1));
        self.date_changed = true;
    }

    pub fn navigate_to_next_day(&mut self) {
        use time::Duration;

        self.current_date = self.current_date.saturating_add(Duration::days(1));
        self.date_changed = true;
    }

    pub fn load_new_day_data(&mut self, new_day_data: DayData) {
        self.day_data = new_day_data;
        self.selected_index = 0;
        self.history = History::new();
        self.date_changed = false;
    }

    pub fn open_calendar(&mut self) {
        self.mode = AppMode::Calendar;
        self.calendar_selected_date = self.current_date;
        self.calendar_view_month = self.current_date.month();
        self.calendar_view_year = self.current_date.year();
    }

    pub fn close_calendar(&mut self) {
        self.mode = AppMode::Browse;
    }

    pub fn calendar_navigate_left(&mut self) {
        use time::Duration;
        self.calendar_selected_date = self
            .calendar_selected_date
            .saturating_sub(Duration::days(1));
        self.calendar_view_month = self.calendar_selected_date.month();
        self.calendar_view_year = self.calendar_selected_date.year();
    }

    pub fn calendar_navigate_right(&mut self) {
        use time::Duration;
        self.calendar_selected_date = self
            .calendar_selected_date
            .saturating_add(Duration::days(1));
        self.calendar_view_month = self.calendar_selected_date.month();
        self.calendar_view_year = self.calendar_selected_date.year();
    }

    pub fn calendar_navigate_up(&mut self) {
        use time::Duration;
        self.calendar_selected_date = self
            .calendar_selected_date
            .saturating_sub(Duration::days(7));
        self.calendar_view_month = self.calendar_selected_date.month();
        self.calendar_view_year = self.calendar_selected_date.year();
    }

    pub fn calendar_navigate_down(&mut self) {
        use time::Duration;
        self.calendar_selected_date = self
            .calendar_selected_date
            .saturating_add(Duration::days(7));
        self.calendar_view_month = self.calendar_selected_date.month();
        self.calendar_view_year = self.calendar_selected_date.year();
    }

    pub fn calendar_previous_month(&mut self) {
        use time::Month;

        let (new_month, new_year) = match self.calendar_view_month {
            Month::January => (Month::December, self.calendar_view_year - 1),
            Month::February => (Month::January, self.calendar_view_year),
            Month::March => (Month::February, self.calendar_view_year),
            Month::April => (Month::March, self.calendar_view_year),
            Month::May => (Month::April, self.calendar_view_year),
            Month::June => (Month::May, self.calendar_view_year),
            Month::July => (Month::June, self.calendar_view_year),
            Month::August => (Month::July, self.calendar_view_year),
            Month::September => (Month::August, self.calendar_view_year),
            Month::October => (Month::September, self.calendar_view_year),
            Month::November => (Month::October, self.calendar_view_year),
            Month::December => (Month::November, self.calendar_view_year),
        };

        self.calendar_view_month = new_month;
        self.calendar_view_year = new_year;

        // Adjust selected date if it's in a different month
        if self.calendar_selected_date.month() != new_month
            || self.calendar_selected_date.year() != new_year
        {
            // Try to keep same day of month, or use last valid day
            let day = self
                .calendar_selected_date
                .day()
                .min(days_in_month(new_month, new_year));
            self.calendar_selected_date =
                time::Date::from_calendar_date(new_year, new_month, day).unwrap();
        }
    }

    pub fn calendar_next_month(&mut self) {
        use time::Month;

        let (new_month, new_year) = match self.calendar_view_month {
            Month::January => (Month::February, self.calendar_view_year),
            Month::February => (Month::March, self.calendar_view_year),
            Month::March => (Month::April, self.calendar_view_year),
            Month::April => (Month::May, self.calendar_view_year),
            Month::May => (Month::June, self.calendar_view_year),
            Month::June => (Month::July, self.calendar_view_year),
            Month::July => (Month::August, self.calendar_view_year),
            Month::August => (Month::September, self.calendar_view_year),
            Month::September => (Month::October, self.calendar_view_year),
            Month::October => (Month::November, self.calendar_view_year),
            Month::November => (Month::December, self.calendar_view_year),
            Month::December => (Month::January, self.calendar_view_year + 1),
        };

        self.calendar_view_month = new_month;
        self.calendar_view_year = new_year;

        // Adjust selected date if it's in a different month
        if self.calendar_selected_date.month() != new_month
            || self.calendar_selected_date.year() != new_year
        {
            // Try to keep same day of month, or use last valid day
            let day = self
                .calendar_selected_date
                .day()
                .min(days_in_month(new_month, new_year));
            self.calendar_selected_date =
                time::Date::from_calendar_date(new_year, new_month, day).unwrap();
        }
    }

    pub fn calendar_select_date(&mut self) {
        self.current_date = self.calendar_selected_date;
        self.date_changed = true;
        self.close_calendar();
    }

    pub fn open_ticket_in_browser(&mut self) {
        use crate::integrations::{build_url, detect_tracker, extract_ticket_from_name};

        if let Some(record) = self.get_selected_record() {
            if let Some(ticket_id) = extract_ticket_from_name(&record.name) {
                if let Some(tracker_name) = detect_tracker(&ticket_id, &self.config) {
                    match build_url(&ticket_id, &tracker_name, &self.config, false) {
                        Ok(url) => {
                            if let Err(e) = open_url_in_browser(&url) {
                                self.last_error_message =
                                    Some(format!("Failed to open browser: {}", e));
                            }
                        }
                        Err(e) => {
                            self.last_error_message = Some(format!("Failed to build URL: {}", e));
                        }
                    }
                } else {
                    self.last_error_message =
                        Some("Could not detect tracker for ticket".to_string());
                }
            } else {
                self.last_error_message = Some("No ticket found in task name".to_string());
            }
        }
    }

    pub fn open_worklog_in_browser(&mut self) {
        use crate::integrations::{build_url, detect_tracker, extract_ticket_from_name};

        if let Some(record) = self.get_selected_record() {
            if let Some(ticket_id) = extract_ticket_from_name(&record.name) {
                if let Some(tracker_name) = detect_tracker(&ticket_id, &self.config) {
                    match build_url(&ticket_id, &tracker_name, &self.config, true) {
                        Ok(url) => {
                            if let Err(e) = open_url_in_browser(&url) {
                                self.last_error_message =
                                    Some(format!("Failed to open browser: {}", e));
                            }
                        }
                        Err(e) => {
                            self.last_error_message = Some(format!("Failed to build URL: {}", e));
                        }
                    }
                } else {
                    self.last_error_message =
                        Some("Could not detect tracker for ticket".to_string());
                }
            } else {
                self.last_error_message = Some("No ticket found in task name".to_string());
            }
        }
    }

    pub fn clear_error(&mut self) {
        self.last_error_message = None;
    }

    pub fn close_task_picker(&mut self) {
        // Cancel and return to Browse mode
        self.input_buffer.clear();
        self.task_picker_tasks.clear();
        self.mode = AppMode::Browse;
    }

    pub fn get_unique_task_names(&self) -> Vec<String> {
        if !self.task_picker_tasks.is_empty() {
            return self.task_picker_tasks.clone();
        }

        use std::collections::HashSet;

        let mut seen = HashSet::new();
        let mut task_names = Vec::new();

        for record in self.day_data.work_records.values() {
            let name = record.name.trim().to_string();
            if !name.is_empty() && seen.insert(name.clone()) {
                task_names.push(name);
            }
        }

        task_names.sort();
        task_names
    }

    pub fn get_filtered_task_names(&self) -> Vec<String> {
        let all_tasks = self.get_unique_task_names();
        let filter = self.input_buffer.to_lowercase();

        if filter.is_empty() {
            return all_tasks;
        }

        all_tasks
            .into_iter()
            .filter(|task| task.to_lowercase().contains(&filter))
            .collect()
    }

    pub fn move_task_picker_up(&mut self) {
        if self.task_picker_selected > 0 {
            self.task_picker_selected -= 1;
        }
    }

    pub fn move_task_picker_down(&mut self, task_count: usize) {
        if self.task_picker_selected < task_count.saturating_sub(1) {
            self.task_picker_selected += 1;
        }
    }

    pub fn select_task_from_picker(&mut self) {
        let filtered_tasks = self.get_filtered_task_names();

        if filtered_tasks.is_empty() {
            // No matches - use the typed input as-is (creating new task)
            // input_buffer already contains the typed text
        } else if let Some(selected_name) = filtered_tasks.get(self.task_picker_selected) {
            // Select from filtered list
            self.input_buffer = selected_name.clone();
        }

        // Save the task name and return to Browse mode
        if let Some(record) = self.get_selected_record() {
            let record_id = record.id;
            let new_name = self.input_buffer.trim().to_string();

            self.save_snapshot();
            if let Some(work_record) = self.day_data.work_records.get_mut(&record_id) {
                work_record.name = new_name;
            }
        }

        self.input_buffer.clear();
        self.task_picker_tasks.clear();
        self.mode = AppMode::Browse;
    }

    pub fn handle_task_picker_char(&mut self, c: char) {
        self.input_buffer.push(c);
        // Reset selection when typing
        self.task_picker_selected = 0;
    }

    pub fn handle_task_picker_backspace(&mut self) {
        self.input_buffer.pop();
        // Reset selection when deleting
        self.task_picker_selected = 0;
    }

    /// Start a new timer with the current selected task
    pub fn start_timer_for_selected(
        &mut self,
        storage: &crate::storage::StorageManager,
    ) -> Result<(), String> {
        if let Some(record) = self.get_selected_record() {
            let description = if record.description.trim().is_empty() {
                None
            } else {
                Some(record.description.clone())
            };
            let project = if record.project.trim().is_empty() {
                None
            } else {
                Some(record.project.clone())
            };
            let customer = if record.customer.trim().is_empty() {
                None
            } else {
                Some(record.customer.clone())
            };

            match storage.start_timer(
                record.name.clone(),
                description,
                project,
                customer,
                Some(record.id),
                Some(self.current_date),
            ) {
                Ok(timer) => {
                    self.active_timer = Some(timer);
                    Ok(())
                }
                Err(e) => Err(e.to_string()),
            }
        } else {
            Err("No record selected".to_string())
        }
    }

    /// Stop the active timer and convert to work record
    pub fn stop_active_timer(
        &mut self,
        storage: &mut crate::storage::StorageManager,
    ) -> Result<(), String> {
        if self.active_timer.is_some() {
            match storage.stop_timer() {
                Ok(_work_record) => {
                    self.active_timer = None;
                    // Reload day data to reflect the new work record
                    match storage.load_with_tracking(self.current_date) {
                        Ok(new_day_data) => {
                            self.day_data = new_day_data;
                            self.selected_index = 0;
                            Ok(())
                        }
                        Err(e) => Err(format!("Failed to reload day data: {}", e)),
                    }
                }
                Err(e) => Err(e.to_string()),
            }
        } else {
            Err("No active timer".to_string())
        }
    }

    /// Pause the active timer
    pub fn pause_active_timer(
        &mut self,
        storage: &crate::storage::StorageManager,
    ) -> Result<(), String> {
        if self.active_timer.is_some() {
            match storage.pause_timer() {
                Ok(paused_timer) => {
                    self.active_timer = Some(paused_timer);
                    Ok(())
                }
                Err(e) => Err(e.to_string()),
            }
        } else {
            Err("No active timer".to_string())
        }
    }

    /// Resume a paused timer
    pub fn resume_active_timer(
        &mut self,
        storage: &crate::storage::StorageManager,
    ) -> Result<(), String> {
        if self.active_timer.is_some() {
            match storage.resume_timer() {
                Ok(resumed_timer) => {
                    self.active_timer = Some(resumed_timer);
                    Ok(())
                }
                Err(e) => Err(e.to_string()),
            }
        } else {
            Err("No active timer".to_string())
        }
    }

    /// Get current status of active timer or None if no timer running
    pub fn get_timer_status(&self) -> Option<&TimerState> {
        self.active_timer.as_ref()
    }

    /// Check if the data file has been modified externally and reload if needed
    /// Returns true if the file was reloaded
    pub fn check_and_reload_if_modified(
        &mut self,
        storage: &mut crate::storage::StorageManager,
    ) -> bool {
        let mut changed = false;

        // Check if day data file has been modified
        if let Ok(Some(new_data)) = storage.check_and_reload(self.current_date) {
            self.day_data = new_data;
            self.last_file_modified = storage.get_last_modified(&self.current_date);

            // Adjust selected_index if it's now out of bounds
            let record_count = self.day_data.work_records.len();
            if self.selected_index >= record_count && record_count > 0 {
                self.selected_index = record_count - 1;
            }

            changed = true;
        }

        // Check if active timer has been modified externally (e.g., started/stopped from CLI)
        if let Ok(Some(timer)) = storage.load_active_timer() {
            // Timer exists - update if different from current state
            if self.active_timer.is_none() || self.active_timer.as_ref() != Some(&timer) {
                self.active_timer = Some(timer);
                changed = true;
            }
        } else if self.active_timer.is_some() {
            // Timer was cleared externally
            self.active_timer = None;
            changed = true;
        }

        changed
    }
}

/// Open a URL in the default browser using platform-specific commands.
///
/// On Windows, special care is taken to handle URLs with query parameters
/// containing `&` characters. The `start` command requires an empty string
/// as the window title argument before the URL, otherwise `&` is interpreted
/// as a command separator by cmd.exe.
fn open_url_in_browser(url: &str) -> std::io::Result<()> {
    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("open").arg(url).spawn()?;
    }

    #[cfg(target_os = "windows")]
    {
        // Windows cmd.exe treats & as a command separator.
        // We use raw_arg to pass the complete command string with proper quoting.
        // Format: cmd /C start "" "url" - empty quotes for title, quoted URL.
        use std::os::windows::process::CommandExt;
        std::process::Command::new("cmd")
            .raw_arg(format!("/C start \"\" \"{}\"", url))
            .spawn()?;
    }

    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        // Linux/Unix
        std::process::Command::new("xdg-open").arg(url).spawn()?;
    }

    Ok(())
}

fn days_in_month(month: time::Month, year: i32) -> u8 {
    use time::Month;
    match month {
        Month::January
        | Month::March
        | Month::May
        | Month::July
        | Month::August
        | Month::October
        | Month::December => 31,
        Month::April | Month::June | Month::September | Month::November => 30,
        Month::February => {
            if is_leap_year(year) {
                29
            } else {
                28
            }
        }
    }
}

fn is_leap_year(year: i32) -> bool {
    (year % 4 == 0 && year % 100 != 0) || (year % 400 == 0)
}
