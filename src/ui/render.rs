use crate::timer::{TimerState, TimerStatus};
use crate::ui::AppState;
use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    widgets::{Block, BorderType, Borders, Cell, Padding, Paragraph, Row, Table, TableState},
};
use std::time::Duration as StdDuration;
use time::OffsetDateTime;

/// Calculate elapsed duration for a timer (extracted from TimerManager to avoid storage dependency)
fn calculate_timer_elapsed(timer: &TimerState) -> StdDuration {
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

    // Subtract paused time
    elapsed_std.saturating_sub(paused_duration_std)
}

pub fn render(frame: &mut Frame, app: &AppState) {
    // Layout changes if timer is active: add timer bar at top
    let main_constraints = if app.active_timer.is_some() {
        vec![
            Constraint::Length(3), // Timer bar (needs 3 lines for borders + content)
            Constraint::Length(3), // Header
            Constraint::Min(10),   // Content
            Constraint::Length(3), // Footer
        ]
    } else {
        vec![
            Constraint::Length(3), // Header
            Constraint::Min(10),   // Content
            Constraint::Length(3), // Footer
        ]
    };

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints(main_constraints)
        .split(frame.size());

    // Render timer bar if active
    if app.active_timer.is_some() {
        render_timer_bar(frame, chunks[0], app);
        let start_idx = 1;
        let header_chunk = chunks[start_idx];
        let content_chunk = chunks[start_idx + 1];
        let footer_chunk = chunks[start_idx + 2];

        let is_wide = frame.size().width >= 100;
        let middle_chunks = if is_wide {
            Layout::default()
                .direction(Direction::Horizontal)
                .constraints([Constraint::Percentage(70), Constraint::Percentage(30)])
                .split(content_chunk)
        } else {
            Layout::default()
                .direction(Direction::Vertical)
                .constraints([Constraint::Min(10), Constraint::Length(15)])
                .split(content_chunk)
        };

        render_header(frame, header_chunk, app);
        render_records(frame, middle_chunks[0], app);
        render_grouped_totals(frame, middle_chunks[1], app);
        render_footer(frame, footer_chunk, app);
    } else {
        // Original layout without timer
        let is_wide = frame.size().width >= 100;
        let middle_chunks = if is_wide {
            Layout::default()
                .direction(Direction::Horizontal)
                .constraints([Constraint::Percentage(70), Constraint::Percentage(30)])
                .split(chunks[1])
        } else {
            Layout::default()
                .direction(Direction::Vertical)
                .constraints([Constraint::Min(10), Constraint::Length(15)])
                .split(chunks[1])
        };

        render_header(frame, chunks[0], app);
        render_records(frame, middle_chunks[0], app);
        render_grouped_totals(frame, middle_chunks[1], app);
        render_footer(frame, chunks[2], app);
    }

    // Render command palette overlay if active
    if matches!(app.mode, crate::ui::AppMode::CommandPalette) {
        render_command_palette(frame, app);
    }

    // Render calendar modal if active
    if matches!(app.mode, crate::ui::AppMode::Calendar) {
        render_calendar(frame, app);
    }

    // Render task picker modal if active
    if matches!(app.mode, crate::ui::AppMode::TaskPicker) {
        render_task_picker(frame, app);
    }

    // Render error modal if there's an error
    if app.last_error_message.is_some() {
        render_error_modal(frame, app);
    }
}

fn render_header(frame: &mut Frame, area: Rect, app: &AppState) {
    let date_str = format!("{}", app.current_date);

    let total_minutes: u32 = app
        .day_data
        .work_records
        .values()
        .map(|r| r.total_minutes)
        .sum();
    let total_hours = total_minutes / 60;
    let total_mins = total_minutes % 60;

    // Create a more visual header with sections
    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(area);

    let title_text = format!("⏱  WorkTimer - {} [←prev] [next→]", date_str);
    let title = Paragraph::new(title_text)
        .style(
            Style::default()
                .fg(app.theme.highlight_text)
                .add_modifier(Modifier::BOLD),
        )
        .alignment(Alignment::Left)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .border_style(Style::default().fg(app.theme.active_border)),
        );

    let total_text = format!("Total: {}h {:02}m", total_hours, total_mins);
    let total = Paragraph::new(total_text)
        .style(
            Style::default()
                .fg(app.theme.success)
                .add_modifier(Modifier::BOLD),
        )
        .alignment(Alignment::Right)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .border_style(Style::default().fg(app.theme.success)),
        );

    frame.render_widget(title, chunks[0]);
    frame.render_widget(total, chunks[1]);
}

fn render_records(frame: &mut Frame, area: Rect, app: &AppState) {
    let records = app.day_data.get_sorted_records();

    // Calculate how many rows can fit in the visible area
    // Account for: borders (2) + header (2) + margin (1) = 5 lines
    let available_height = area.height.saturating_sub(5) as usize;

    // Calculate scroll offset to keep selected item visible
    let scroll_offset = if records.len() > available_height {
        if app.selected_index >= available_height {
            app.selected_index.saturating_sub(available_height - 1)
        } else {
            0
        }
    } else {
        0
    };

    let rows: Vec<Row> = records
        .iter()
        .enumerate()
        .map(|(i, record)| {
            let is_selected = i == app.selected_index;
            let is_editing = matches!(app.mode, crate::ui::AppMode::Edit) && is_selected;
            let is_in_visual =
                matches!(app.mode, crate::ui::AppMode::Visual) && app.is_in_visual_selection(i);

            // Check if this record has an active timer running
            // Compare by source_record_id to highlight only the specific record, not all with same name
            let has_active_timer = app
                .active_timer
                .as_ref()
                .is_some_and(|timer| timer.source_record_id == Some(record.id));

            // Enhanced styling with more vibrant colors
            let style = if is_in_visual {
                Style::default()
                    .bg(app.theme.visual_bg)
                    .fg(app.theme.primary_text)
                    .add_modifier(Modifier::BOLD)
            } else if has_active_timer {
                // Highlight record with active timer in green/gold
                Style::default()
                    .bg(app.theme.timer_active_bg)
                    .fg(app.theme.timer_text)
                    .add_modifier(Modifier::BOLD)
            } else if is_selected {
                Style::default()
                    .bg(app.theme.selected_bg)
                    .fg(app.theme.highlight_text)
                    .add_modifier(Modifier::BOLD)
            } else if i % 2 == 0 {
                Style::default().bg(app.theme.row_alternate_bg)
            } else {
                Style::default()
            };

            // Add icon/emoji based on task type, with timer indicator if active
            let icon = if has_active_timer {
                "⏱ " // Timer icon for active timers
            } else if record.name.to_lowercase().contains("break") {
                "☕"
            } else if record.name.to_lowercase().contains("meeting") {
                "👥"
            } else if record.name.to_lowercase().contains("code")
                || record.name.to_lowercase().contains("dev")
            {
                "💻"
            } else {
                "📋"
            };

            let format_name_with_badge = |display_name: &str, ticket_source: &str| {
                if app.config.has_integrations()
                    && crate::integrations::extract_ticket_from_name(ticket_source).is_some()
                {
                    format!("🎫 {} {}", icon, display_name)
                } else {
                    format!("{} {}", icon, display_name)
                }
            };

            let mut name_display = format_name_with_badge(&record.name, &record.name);
            let mut start_display = record.start.to_string();
            let mut end_display = record.end.to_string();
            let mut project_display = record.project.clone();
            let mut customer_display = record.customer.clone();
            let mut description_display = record.description.clone();

            if is_editing {
                match app.edit_field {
                    crate::ui::EditField::Name => {
                        let name_with_cursor = format!("{}▏", app.input_buffer);
                        name_display = format_name_with_badge(&name_with_cursor, &app.input_buffer);
                    }
                    crate::ui::EditField::Start | crate::ui::EditField::End => {
                        let positions = [0, 1, 3, 4];
                        let cursor_pos = if app.time_cursor < positions.len() {
                            positions[app.time_cursor]
                        } else {
                            positions[positions.len() - 1]
                        };

                        let mut display = String::new();
                        for (index, ch) in app.input_buffer.chars().enumerate() {
                            if index == cursor_pos {
                                display.push('[');
                                display.push(ch);
                                display.push(']');
                            } else {
                                display.push(ch);
                            }
                        }

                        match app.edit_field {
                            crate::ui::EditField::Start => start_display = display,
                            crate::ui::EditField::End => end_display = display,
                            _ => {}
                        }
                    }
                    crate::ui::EditField::Project => {
                        project_display = format!("{}▏", app.input_buffer);
                    }
                    crate::ui::EditField::Customer => {
                        customer_display = format!("{}▏", app.input_buffer);
                    }
                    crate::ui::EditField::Description => {
                        description_display = format!("{}▏", app.input_buffer);
                    }
                }
            }

            let field_style = |field: crate::ui::EditField, default_style: Style| {
                if is_editing && app.edit_field == field {
                    Style::default()
                        .bg(app.theme.edit_bg)
                        .fg(app.theme.primary_text)
                        .add_modifier(Modifier::BOLD)
                } else if is_selected && app.edit_field == field {
                    Style::default()
                        .bg(app.theme.focus_bg)
                        .fg(app.theme.primary_text)
                        .add_modifier(Modifier::BOLD)
                } else {
                    default_style
                }
            };

            let mut cells = vec![
                Cell::from(name_display)
                    .style(field_style(crate::ui::EditField::Name, Style::default())),
                Cell::from(start_display).style(field_style(
                    crate::ui::EditField::Start,
                    Style::default().fg(app.theme.success),
                )),
                Cell::from(end_display).style(field_style(
                    crate::ui::EditField::End,
                    Style::default().fg(app.theme.error),
                )),
                Cell::from(record.format_duration()).style(Style::default().fg(app.theme.badge)),
            ];

            if app.show_project_column() {
                cells.push(Cell::from(project_display).style(field_style(
                    crate::ui::EditField::Project,
                    Style::default().fg(app.theme.primary_text),
                )));
            }

            if app.show_customer_column() {
                cells.push(Cell::from(customer_display).style(field_style(
                    crate::ui::EditField::Customer,
                    Style::default().fg(app.theme.primary_text),
                )));
            }

            if app.show_description_column() {
                cells.push(Cell::from(description_display).style(field_style(
                    crate::ui::EditField::Description,
                    Style::default().fg(app.theme.primary_text),
                )));
            }

            Row::new(cells).style(style)
        })
        .collect();

    let mut constraints = vec![
        Constraint::Min(18),
        Constraint::Length(10),
        Constraint::Length(10),
        Constraint::Length(12),
    ];
    let mut header_cells = vec![
        Cell::from("📝 Task"),
        Cell::from("🕐 Start"),
        Cell::from("🕐 End"),
        Cell::from("⏱  Duration"),
    ];

    if app.show_project_column() {
        constraints.push(Constraint::Length(16));
        header_cells.push(Cell::from("🗂 Project"));
    }

    if app.show_customer_column() {
        constraints.push(Constraint::Length(16));
        header_cells.push(Cell::from("👤 Customer"));
    }

    if app.show_description_column() {
        constraints.push(Constraint::Min(20));
        header_cells.push(Cell::from("📄 Description"));
    }

    let table = Table::new(rows, constraints)
        .header(
            Row::new(header_cells)
                .style(
                    Style::default()
                        .fg(app.theme.warning)
                        .add_modifier(Modifier::BOLD),
                )
                .bottom_margin(1),
        )
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .border_style(Style::default().fg(app.theme.active_border))
                .title("📊 Work Records")
                .title_style(
                    Style::default()
                        .fg(app.theme.highlight_text)
                        .add_modifier(Modifier::BOLD),
                ),
        );

    // Use stateful rendering to handle scrolling
    let mut table_state = TableState::default()
        .with_selected(Some(app.selected_index))
        .with_offset(scroll_offset);

    frame.render_stateful_widget(table, area, &mut table_state);
}

fn render_grouped_totals(frame: &mut Frame, area: Rect, app: &AppState) {
    let grouped = app.day_data.get_grouped_totals();

    if grouped.is_empty() {
        let paragraph = Paragraph::new("No records yet")
            .style(Style::default().fg(app.theme.secondary_text))
            .alignment(Alignment::Center)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_type(BorderType::Rounded)
                    .border_style(Style::default().fg(app.theme.warning))
                    .title("📈 Summary")
                    .title_style(
                        Style::default()
                            .fg(app.theme.warning)
                            .add_modifier(Modifier::BOLD),
                    ),
            );
        frame.render_widget(paragraph, area);
        return;
    }

    let rows: Vec<Row> = grouped
        .iter()
        .map(|(name, minutes)| {
            let hours = minutes / 60;
            let mins = minutes % 60;

            // Choose icon based on task type
            let icon = if name.to_lowercase().contains("break") {
                "☕"
            } else if name.to_lowercase().contains("meeting") {
                "👥"
            } else if name.to_lowercase().contains("code") || name.to_lowercase().contains("dev") {
                "💻"
            } else {
                "📋"
            };

            Row::new(vec![
                Cell::from(format!("{} {}", icon, name)),
                Cell::from(format!("{}h {:02}m", hours, mins)).style(
                    Style::default()
                        .fg(app.theme.badge)
                        .add_modifier(Modifier::BOLD),
                ),
            ])
        })
        .collect();

    let table = Table::new(
        rows,
        [Constraint::Percentage(65), Constraint::Percentage(35)],
    )
    .header(
        Row::new(vec![Cell::from("Task"), Cell::from("Total")])
            .style(
                Style::default()
                    .fg(app.theme.warning)
                    .add_modifier(Modifier::BOLD),
            )
            .bottom_margin(1),
    )
    .block(
        Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(app.theme.warning))
            .title("📈 Summary")
            .title_style(
                Style::default()
                    .fg(app.theme.warning)
                    .add_modifier(Modifier::BOLD),
            ),
    );

    frame.render_widget(table, area);
}

fn render_footer(frame: &mut Frame, area: Rect, app: &AppState) {
    // Build help text for Browse mode conditionally
    let browse_help = if app.config.has_integrations() {
        "↑/↓: Row | ←/→: Field | [/]: Day | C: Calendar | Enter: Edit | c: Change | n: New | b: Break | d: Delete | v: Visual | t: Now | T: Ticket | L: Worklog | S: Session Start/Stop | P: Pause | ?: Help | q: Quit"
    } else {
        "↑/↓: Row | ←/→: Field | [/]: Day | C: Calendar | Enter: Edit | c: Change | n: New | b: Break | d: Delete | v: Visual | t: Now | S: Session Start/Stop | P: Pause | ?: Help | q: Quit"
    };

    let (help_text, mode_color, mode_label) = match app.mode {
        crate::ui::AppMode::Browse => (browse_help, app.theme.info, "BROWSE"),
        crate::ui::AppMode::Edit => (
            "Tab: Next field | Enter/Esc: Save",
            app.theme.warning,
            "EDIT",
        ),
        crate::ui::AppMode::Visual => (
            "↑/↓: Extend selection | d: Delete | Esc: Exit visual",
            app.theme.badge,
            "VISUAL",
        ),
        crate::ui::AppMode::CommandPalette => (
            "↑/↓: Navigate | Enter: Execute | Esc: Cancel",
            app.theme.success,
            "COMMAND PALETTE",
        ),
        crate::ui::AppMode::Calendar => (
            "hjkl/arrows: Navigate | </>: Month | Enter: Select | Esc: Cancel",
            app.theme.badge,
            "CALENDAR",
        ),
        crate::ui::AppMode::TaskPicker => (
            "Type: Filter/Create | ↑/↓: Navigate | Enter: Select | Esc: Cancel",
            app.theme.info,
            "TASK PICKER",
        ),
    };

    let footer = Paragraph::new(help_text)
        .style(Style::default().fg(app.theme.secondary_text))
        .alignment(Alignment::Center)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .border_style(Style::default().fg(mode_color))
                .title(format!("⌨  {} MODE", mode_label))
                .title_style(Style::default().fg(mode_color).add_modifier(Modifier::BOLD))
                .padding(Padding::horizontal(1)),
        );

    frame.render_widget(footer, area);
}

fn render_command_palette(frame: &mut Frame, app: &AppState) {
    use ratatui::widgets::Clear;

    // Create a centered modal
    let area = frame.size();
    let width = area.width.min(80);
    let height = area.height.min(20);
    let x = (area.width.saturating_sub(width)) / 2;
    let y = (area.height.saturating_sub(height)) / 2;

    let modal_area = Rect {
        x,
        y,
        width,
        height,
    };

    // Clear the background
    frame.render_widget(Clear, modal_area);

    // Add a background block for the entire modal
    let bg_block = Block::default().style(Style::default().bg(app.theme.row_alternate_bg));
    frame.render_widget(bg_block, modal_area);

    // Split modal into input and results
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(3), Constraint::Min(5)])
        .split(modal_area);

    // Render search input
    let input_text = if app.command_palette_input.is_empty() {
        "Type to search commands...".to_string()
    } else {
        app.command_palette_input.clone()
    };

    let input_style = if app.command_palette_input.is_empty() {
        Style::default()
            .fg(app.theme.secondary_text)
            .bg(app.theme.edit_bg)
    } else {
        Style::default()
            .fg(app.theme.primary_text)
            .bg(app.theme.edit_bg)
    };

    let input = Paragraph::new(input_text).style(input_style).block(
        Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(app.theme.active_border))
            .title("🔍 Search Commands")
            .title_style(
                Style::default()
                    .fg(app.theme.active_border)
                    .add_modifier(Modifier::BOLD),
            )
            .style(Style::default().bg(app.theme.edit_bg)),
    );

    frame.render_widget(input, chunks[0]);

    // Render filtered commands
    let filtered = app.get_filtered_commands();

    let rows: Vec<Row> = filtered
        .iter()
        .enumerate()
        .map(|(i, (_, score, cmd))| {
            let is_selected = i == app.command_palette_selected;

            let style = if is_selected {
                Style::default()
                    .bg(app.theme.selected_bg)
                    .fg(app.theme.primary_text)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().bg(app.theme.row_alternate_bg)
            };

            let key_display = format!("  {}  ", cmd.key);
            let score_display = if *score > 0 {
                format!(" ({})", score)
            } else {
                String::new()
            };

            Row::new(vec![
                Cell::from(key_display).style(
                    Style::default()
                        .fg(app.theme.success)
                        .add_modifier(Modifier::BOLD),
                ),
                Cell::from(cmd.description).style(Style::default().fg(app.theme.primary_text)),
                Cell::from(score_display).style(Style::default().fg(app.theme.secondary_text)),
            ])
            .style(style)
        })
        .collect();

    let results_table = Table::new(
        rows,
        [
            Constraint::Length(15),
            Constraint::Min(30),
            Constraint::Length(10),
        ],
    )
    .block(
        Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(app.theme.active_border))
            .title(format!("📋 Commands ({} found)", filtered.len()))
            .title_style(
                Style::default()
                    .fg(app.theme.active_border)
                    .add_modifier(Modifier::BOLD),
            )
            .style(Style::default().bg(app.theme.row_alternate_bg)),
    );

    frame.render_widget(results_table, chunks[1]);
}

fn render_calendar(frame: &mut Frame, app: &AppState) {
    use ratatui::widgets::Clear;
    use time::{Date, Month, Weekday};

    // Create a centered modal
    let area = frame.size();
    let width = area.width.min(60);
    let height = area.height.min(25);
    let x = (area.width.saturating_sub(width)) / 2;
    let y = (area.height.saturating_sub(height)) / 2;

    let modal_area = Rect {
        x,
        y,
        width,
        height,
    };

    // Clear the background
    frame.render_widget(Clear, modal_area);

    // Add a background block for the entire modal
    let bg_block = Block::default().style(Style::default().bg(app.theme.row_alternate_bg));
    frame.render_widget(bg_block, modal_area);

    // Create the calendar layout
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // Month/Year header
            Constraint::Min(15),   // Calendar grid
        ])
        .split(modal_area);

    // Render month/year header
    let month_name = match app.calendar_view_month {
        Month::January => "January",
        Month::February => "February",
        Month::March => "March",
        Month::April => "April",
        Month::May => "May",
        Month::June => "June",
        Month::July => "July",
        Month::August => "August",
        Month::September => "September",
        Month::October => "October",
        Month::November => "November",
        Month::December => "December",
    };

    let header_text = format!(
        "📅  {} {}  [< prev] [next >]",
        month_name, app.calendar_view_year
    );
    let header = Paragraph::new(header_text)
        .style(
            Style::default()
                .fg(app.theme.info)
                .add_modifier(Modifier::BOLD),
        )
        .alignment(Alignment::Center)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .border_style(Style::default().fg(app.theme.info))
                .style(Style::default().bg(app.theme.edit_bg)),
        );

    frame.render_widget(header, chunks[0]);

    // Build calendar grid
    let first_day =
        Date::from_calendar_date(app.calendar_view_year, app.calendar_view_month, 1).unwrap();

    let days_in_month = get_days_in_month(app.calendar_view_month, app.calendar_view_year);
    let first_weekday = first_day.weekday();

    // Calculate starting offset (0 = Monday, 6 = Sunday)
    let offset = match first_weekday {
        Weekday::Monday => 0,
        Weekday::Tuesday => 1,
        Weekday::Wednesday => 2,
        Weekday::Thursday => 3,
        Weekday::Friday => 4,
        Weekday::Saturday => 5,
        Weekday::Sunday => 6,
    };

    // Create calendar rows
    let mut rows = Vec::new();

    // Header row with weekday names
    rows.push(Row::new(vec![
        Cell::from("Mon").style(
            Style::default()
                .fg(app.theme.warning)
                .add_modifier(Modifier::BOLD),
        ),
        Cell::from("Tue").style(
            Style::default()
                .fg(app.theme.warning)
                .add_modifier(Modifier::BOLD),
        ),
        Cell::from("Wed").style(
            Style::default()
                .fg(app.theme.warning)
                .add_modifier(Modifier::BOLD),
        ),
        Cell::from("Thu").style(
            Style::default()
                .fg(app.theme.warning)
                .add_modifier(Modifier::BOLD),
        ),
        Cell::from("Fri").style(
            Style::default()
                .fg(app.theme.warning)
                .add_modifier(Modifier::BOLD),
        ),
        Cell::from("Sat").style(
            Style::default()
                .fg(app.theme.info)
                .add_modifier(Modifier::BOLD),
        ),
        Cell::from("Sun").style(
            Style::default()
                .fg(app.theme.info)
                .add_modifier(Modifier::BOLD),
        ),
    ]));

    // Calendar days
    let mut current_day = 1;
    let mut week_row = Vec::new();

    // Fill in the offset days with empty cells
    for _ in 0..offset {
        week_row.push(Cell::from("  "));
    }

    // Fill in the actual days
    for day_of_week in offset..7 {
        if current_day <= days_in_month {
            let date = Date::from_calendar_date(
                app.calendar_view_year,
                app.calendar_view_month,
                current_day,
            )
            .unwrap();

            let is_selected = date == app.calendar_selected_date;
            let is_today = date == time::OffsetDateTime::now_utc().date();
            let is_current_view = date == app.current_date;

            let day_str = format!("{:2}", current_day);

            let style = if is_selected {
                Style::default()
                    .bg(app.theme.visual_bg)
                    .fg(app.theme.primary_text)
                    .add_modifier(Modifier::BOLD)
            } else if is_current_view {
                Style::default()
                    .bg(app.theme.selected_bg)
                    .fg(app.theme.primary_text)
                    .add_modifier(Modifier::BOLD)
            } else if is_today {
                Style::default()
                    .fg(app.theme.success)
                    .add_modifier(Modifier::BOLD | Modifier::UNDERLINED)
            } else {
                Style::default().fg(app.theme.primary_text)
            };

            week_row.push(Cell::from(day_str).style(style));
            current_day += 1;
        } else {
            week_row.push(Cell::from("  "));
        }

        if day_of_week == 6 {
            rows.push(Row::new(week_row.clone()));
            week_row.clear();
        }
    }

    // Continue filling remaining weeks
    while current_day <= days_in_month {
        for _ in 0..7 {
            if current_day <= days_in_month {
                let date = Date::from_calendar_date(
                    app.calendar_view_year,
                    app.calendar_view_month,
                    current_day,
                )
                .unwrap();

                let is_selected = date == app.calendar_selected_date;
                let is_today = date == time::OffsetDateTime::now_utc().date();
                let is_current_view = date == app.current_date;

                let day_str = format!("{:2}", current_day);

                let style = if is_selected {
                    Style::default()
                        .bg(app.theme.visual_bg)
                        .fg(app.theme.primary_text)
                        .add_modifier(Modifier::BOLD)
                } else if is_current_view {
                    Style::default()
                        .bg(app.theme.selected_bg)
                        .fg(app.theme.primary_text)
                        .add_modifier(Modifier::BOLD)
                } else if is_today {
                    Style::default()
                        .fg(app.theme.success)
                        .add_modifier(Modifier::BOLD | Modifier::UNDERLINED)
                } else {
                    Style::default().fg(app.theme.primary_text)
                };

                week_row.push(Cell::from(day_str).style(style));
                current_day += 1;
            } else {
                week_row.push(Cell::from("  "));
            }
        }
        rows.push(Row::new(week_row.clone()));
        week_row.clear();
    }

    let calendar_table = Table::new(
        rows,
        [
            Constraint::Length(6),
            Constraint::Length(6),
            Constraint::Length(6),
            Constraint::Length(6),
            Constraint::Length(6),
            Constraint::Length(6),
            Constraint::Length(6),
        ],
    )
    .block(
        Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(app.theme.info))
            .title("📆 Select Date")
            .title_style(
                Style::default()
                    .fg(app.theme.info)
                    .add_modifier(Modifier::BOLD),
            )
            .style(Style::default().bg(app.theme.row_alternate_bg)),
    );

    frame.render_widget(calendar_table, chunks[1]);
}

fn get_days_in_month(month: time::Month, year: i32) -> u8 {
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
            if is_leap_year_render(year) {
                29
            } else {
                28
            }
        }
    }
}

fn is_leap_year_render(year: i32) -> bool {
    (year % 4 == 0 && year % 100 != 0) || (year % 400 == 0)
}

fn render_error_modal(frame: &mut Frame, app: &AppState) {
    use ratatui::text::Line;
    use ratatui::widgets::Clear;

    // Create a centered modal
    let area = frame.size();
    let width = area.width.min(70);
    let height = 10;
    let x = (area.width.saturating_sub(width)) / 2;
    let y = (area.height.saturating_sub(height)) / 2;

    let modal_area = Rect {
        x,
        y,
        width,
        height,
    };

    // Clear the background
    frame.render_widget(Clear, modal_area);

    // Add a background block for the entire modal
    let bg_block = Block::default().style(Style::default().bg(app.theme.row_alternate_bg));
    frame.render_widget(bg_block, modal_area);

    // Get error message
    let error_text = if let Some(ref error_msg) = app.last_error_message {
        error_msg.clone()
    } else {
        "Unknown error".to_string()
    };

    // Split modal into message and footer
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(5), Constraint::Length(3)])
        .split(modal_area);

    // Render error message
    let lines = vec![
        Line::from(""),
        Line::from(format!("  {}", error_text)).style(Style::default().fg(app.theme.primary_text)),
        Line::from(""),
    ];

    let error_msg = Paragraph::new(lines).alignment(Alignment::Left).block(
        Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(app.theme.error))
            .title("❌ ERROR")
            .title_style(
                Style::default()
                    .fg(app.theme.error)
                    .add_modifier(Modifier::BOLD),
            )
            .style(Style::default().bg(app.theme.row_alternate_bg)),
    );

    frame.render_widget(error_msg, chunks[0]);

    // Render help text
    let help = Paragraph::new("Press any key to dismiss")
        .alignment(Alignment::Center)
        .style(Style::default().fg(app.theme.secondary_text));

    frame.render_widget(help, chunks[1]);
}

fn render_task_picker(frame: &mut Frame, app: &AppState) {
    use ratatui::widgets::Clear;

    let filtered_tasks = app.get_filtered_task_names();
    let all_tasks = app.get_unique_task_names();

    // Create a smaller centered modal (mini-picker style)
    let area = frame.size();
    let width = area.width.min(60);
    let height = (filtered_tasks.len() as u16 + 8).clamp(12, 20); // Ensure minimum height for visibility
    let x = (area.width.saturating_sub(width)) / 2;
    let y = (area.height.saturating_sub(height)) / 2;

    let modal_area = Rect {
        x,
        y,
        width,
        height,
    };

    // Clear the background
    frame.render_widget(Clear, modal_area);

    // Add a background block for the entire modal
    let bg_block = Block::default().style(Style::default().bg(app.theme.selected_inactive_bg));
    frame.render_widget(bg_block, modal_area);

    // Split modal into header, input, and list
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // Header
            Constraint::Length(4), // Input field (increased for better visibility)
            Constraint::Min(5),    // List
        ])
        .split(modal_area);

    // Render header with help text
    let header_text = if app.input_buffer.is_empty() {
        "Select existing task or type new name"
    } else {
        "Type to filter, or create new task"
    };

    let header = Paragraph::new(header_text)
        .style(Style::default().fg(app.theme.primary_text))
        .alignment(Alignment::Center)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .border_style(Style::default().fg(app.theme.info))
                .title("📋 Task Picker")
                .title_style(
                    Style::default()
                        .fg(app.theme.info)
                        .add_modifier(Modifier::BOLD),
                )
                .style(Style::default().bg(app.theme.selected_inactive_bg)),
        );

    frame.render_widget(header, chunks[0]);

    // Render input field
    let input_display = if app.input_buffer.is_empty() {
        "Start typing...".to_string()
    } else {
        app.input_buffer.clone()
    };

    let input = Paragraph::new(input_display)
        .style(if app.input_buffer.is_empty() {
            Style::default().fg(app.theme.secondary_text)
        } else {
            Style::default()
                .fg(app.theme.primary_text)
                .add_modifier(Modifier::BOLD)
        })
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .border_style(Style::default().fg(app.theme.warning))
                .title("Filter / New Task")
                .title_style(Style::default().fg(app.theme.warning))
                .style(Style::default().bg(app.theme.selected_inactive_bg))
                .padding(ratatui::widgets::Padding::horizontal(1)),
        );

    frame.render_widget(input, chunks[1]);

    // Render task list
    if all_tasks.is_empty() {
        let empty_msg = Paragraph::new("No existing tasks. Type to create new one.")
            .style(Style::default().fg(app.theme.secondary_text))
            .alignment(Alignment::Center)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_type(BorderType::Rounded)
                    .border_style(Style::default().fg(app.theme.info))
                    .style(Style::default().bg(app.theme.selected_inactive_bg)),
            );
        frame.render_widget(empty_msg, chunks[2]);
    } else if filtered_tasks.is_empty() && !app.input_buffer.is_empty() {
        let new_task_msg =
            Paragraph::new(format!("Press Enter to create: \"{}\"", app.input_buffer))
                .style(
                    Style::default()
                        .fg(app.theme.success)
                        .add_modifier(Modifier::BOLD),
                )
                .alignment(Alignment::Center)
                .block(
                    Block::default()
                        .borders(Borders::ALL)
                        .border_type(BorderType::Rounded)
                        .border_style(Style::default().fg(app.theme.success))
                        .title("New Task")
                        .title_style(
                            Style::default()
                                .fg(app.theme.success)
                                .add_modifier(Modifier::BOLD),
                        )
                        .style(Style::default().bg(app.theme.selected_inactive_bg)),
                );
        frame.render_widget(new_task_msg, chunks[2]);
    } else {
        let rows: Vec<Row> = filtered_tasks
            .iter()
            .enumerate()
            .map(|(i, name)| {
                let is_selected = i == app.task_picker_selected;

                let style = if is_selected {
                    Style::default()
                        .bg(app.theme.selected_bg)
                        .fg(app.theme.primary_text)
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default().bg(app.theme.selected_inactive_bg)
                };

                // Add icon based on task type
                let icon = if name.to_lowercase().contains("break") {
                    "☕"
                } else if name.to_lowercase().contains("meeting") {
                    "👥"
                } else if name.to_lowercase().contains("code")
                    || name.to_lowercase().contains("dev")
                {
                    "💻"
                } else {
                    "📋"
                };

                let display_name = format!("{} {}", icon, name);

                Row::new(vec![
                    Cell::from(display_name).style(Style::default().fg(app.theme.primary_text)),
                ])
                .style(style)
            })
            .collect();

        let title = if app.input_buffer.is_empty() {
            format!("Tasks ({} available)", filtered_tasks.len())
        } else {
            format!("Filtered ({}/{})", filtered_tasks.len(), all_tasks.len())
        };

        let task_table = Table::new(rows, [Constraint::Percentage(100)]).block(
            Block::default()
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .border_style(Style::default().fg(app.theme.info))
                .title(title)
                .title_style(
                    Style::default()
                        .fg(app.theme.info)
                        .add_modifier(Modifier::BOLD),
                )
                .style(Style::default().bg(app.theme.selected_inactive_bg)),
        );

        frame.render_widget(task_table, chunks[2]);
    }
}

/// Render timer bar showing active timer status at the top of the screen
fn render_timer_bar(frame: &mut Frame, area: Rect, app: &AppState) {
    use crate::timer::TimerStatus;

    if let Some(timer) = &app.active_timer {
        // Calculate elapsed time directly without needing storage
        let elapsed = calculate_timer_elapsed(timer);

        // Format elapsed time
        let secs = elapsed.as_secs();
        let hours = secs / 3600;
        let mins = (secs % 3600) / 60;
        let seconds = secs % 60;

        let status_icon = match timer.status {
            TimerStatus::Running => "▶",
            TimerStatus::Paused => "⏸",
            TimerStatus::Stopped => "⏹",
        };

        let timer_text = if hours > 0 {
            format!(
                "{} {} - {}:{}:{}",
                status_icon, timer.task_name, hours, mins, seconds
            )
        } else {
            format!(
                "{} {} - {:02}:{:02}",
                status_icon, timer.task_name, mins, seconds
            )
        };

        let timer_color = match timer.status {
            TimerStatus::Running => app.theme.success,
            TimerStatus::Paused => app.theme.warning,
            TimerStatus::Stopped => app.theme.error,
        };

        let timer_paragraph = Paragraph::new(timer_text)
            .style(
                Style::default()
                    .fg(timer_color)
                    .add_modifier(Modifier::BOLD),
            )
            .alignment(Alignment::Center)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_type(BorderType::Rounded)
                    .border_style(Style::default().fg(timer_color)),
            );

        frame.render_widget(timer_paragraph, area);
    }
}
