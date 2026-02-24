# Timer Sessions

Sessions track time in real-time with automatic updates, pause/resume support, and shared state between TUI and CLI.

## Table of Contents

- [What are Sessions?](#what-are-sessions)
- [TUI Usage](#tui-usage)
- [CLI Usage](#cli-usage)
- [Session Features](#session-features)
- [Common Workflows](#common-workflows)
- [Data Persistence](#data-persistence)

## What are Sessions?

A session is an active timer that tracks time spent on a task. Recorded data:
- Task name and optional description
- Start time
- Elapsed time (updated in real-time)
- Pause/resume history
- Final duration when stopped

Start a timer when you begin work, stop it when done. End time is automatically set to the current time.

## TUI Usage

### Starting a Session

In the TUI, you must select an existing work record, then press `S` to start a session:
- Updates that record's end time when stopped
- Use to extend existing entries

To create a new task with a session, use the CLI `session start` command.

### Session Controls

| Key | Action |
|-----|--------|
| `S` | Start/Stop session on selected record |
| `P` | Pause/Resume active session |

### Visual Indicators

Active session displays:

1. **Timer Status Bar** (top of screen):
   ```
   ⏱ Running: Task Name | 1h 23m 45s | Status: Running
   ```
   Shows task name, elapsed time (H:MM:SS), and status (Running/Paused)

2. **Record Highlighting**:
   - Active session records show ⏱ icon

### Session States

- **Running**: Time is actively being tracked
- **Paused**: Timer is paused, paused duration is tracked separately
- **Stopped**: Session has ended, time is saved to the work record

## CLI Usage

Control sessions from the command line without opening the TUI.

### Starting a Session

The CLI can create new tasks with sessions:

```bash
# Start a basic session
work-tuimer session start "My Task"

# Start with a description
work-tuimer session start "Bug Fix" -d "Fixing authentication issue"
```

This creates a new work record and starts tracking immediately.

Output:
```
✓ Session started
  Task: My Task
  Description: Optional description
  Started at: 14:30:45
```

### Checking Session Status

```bash
work-tuimer session status
```

Output:
```
⏱ Session Status
  Task: My Task
  Status: Running
  Elapsed: 1h 23m 45s
  Started at: 14:30:45
  Description: Optional description
```

### Pausing and Resuming

```bash
# Pause the active session
work-tuimer session pause
```

Output:
```
⏸ Session paused
  Task: My Task
  Elapsed: 0m 45s
```

```bash
# Resume the paused session
work-tuimer session resume
```

Output:
```
▶ Session resumed
  Task: My Task
  Total elapsed (before pause): 0m 45s
```

### Stopping a Session

```bash
work-tuimer session stop
```

Output:
```
✓ Session stopped
  Task: My Task
  Duration: 1h 23m 45s
  Started at: 14:30:45
  Ended at: 15:54:30
```

### Error Handling

If you try to control a session when none is running:
```bash
$ work-tuimer session stop
Error: No session is running
```

If you try to start a session when one is already running:
```bash
$ work-tuimer session start "Another Task"
Error: A timer is already running
```

## Session Features

### Automatic Time Updates

End time is automatically set to current time when stopped.

### Pause Support

Pause and resume sessions:
- **Elapsed time**: Only counts active time (excludes paused duration)
- **Paused duration**: Tracked separately
- **Multiple pauses**: Pause/resume as needed

### Persistence Across Restarts

Sessions survive application restarts. State is saved in a SQLite database under the application data directory returned by `dirs::data_local_dir()`, for example:
- Linux: `~/.local/share/work-tuimer/work-tuimer.db`
- macOS: `~/Library/Application Support/work-tuimer/work-tuimer.db`
- Windows: `%APPDATA%\work-tuimer\work-tuimer.db`

### Cross-Date Support

Start a session on a record from any date:
- Navigate to any day in the TUI
- Start a session on that day's record
- End time updates correctly when stopped

### CLI and TUI Integration

Sessions share state across both interfaces:
- Start in CLI, pause in TUI
- Start in TUI, check status in CLI
- Changes sync automatically

TUI auto-reloads every 500ms to reflect external changes.

## Common Workflows

### Workflow 1: Simple Session

```bash
# Start working
work-tuimer session start "Write documentation"

# ... work on your task ...

# Stop when done
work-tuimer session stop
```

### Workflow 2: Session with Breaks

```bash
# Start working
work-tuimer session start "Code review"

# ... work for a while ...

# Take a break
work-tuimer session pause

# ... break time ...

# Resume work
work-tuimer session resume

# ... finish up ...

# Stop when done
work-tuimer session stop
```

### Workflow 3: TUI + CLI Hybrid

```bash
# Start in CLI before opening TUI
work-tuimer session start "Morning standup prep"

# Open TUI to view full day
work-tuimer

# Continue working in TUI, see session status at top
# Press P to pause, S to stop, or let it run

# Later, check status from CLI
work-tuimer session status

# Stop from CLI when done
work-tuimer session stop
```

### Workflow 4: Updating Existing Records

In the TUI:
1. Navigate to a work record to extend
2. Press `S` to start a session
3. Work continues...
4. Press `S` again to stop

Record's end time and duration update automatically.

### Workflow 5: Quick Status Checks

```bash
# Quick check if anything is running
work-tuimer session status

# If nothing running, start new task
work-tuimer session start "New task"
```

## Data Persistence

### Active Session Storage

Active sessions are stored in SQLite table `active_timer` (single-row state):
- **Linux/macOS**: `~/.local/share/work-tuimer/work-tuimer.db`
- **Windows**: `%APPDATA%\work-tuimer\work-tuimer.db`
- **Fallback**: `./data/work-tuimer.db`

### Session State Fields

The `active_timer` row stores:
- `task_name`, `description`
- `start_time`, `end_time`, `date`
- `status`, `paused_duration_secs`, `paused_at`
- `source_record_id`, `source_record_date`

### Work Record Integration

When a session stops, it creates or updates a work record in SQLite (`work_records` table) for the target day.

### Database Location Priority

Daily work records are saved to (checked in order):
1. `~/.local/share/work-tuimer/work-tuimer.db`
2. `./data/work-tuimer.db` (fallback)

## Tips

1. **Use descriptive task names**: Easier to identify work later
2. **Add descriptions for context**: Useful when reviewing time logs
3. **Pause during interruptions**: Accurate tracking excludes breaks
4. **Check status regularly**: `work-tuimer session status` shows active sessions
5. **Stop sessions promptly**: Remember to stop when switching tasks
6. **Use CLI for quick starts**: Start sessions without opening TUI
7. **Work in both interfaces**: Auto-reload syncs changes every 500ms

## Troubleshooting

### Session not showing in TUI

- The TUI auto-reloads every 500ms, wait a moment
- If still not visible, restart the TUI

### Lost session after restart

- Sessions are saved to `work-tuimer.db` (table `active_timer`) - check if the DB file exists
- If the DB file was deleted, the session cannot be recovered

### Wrong end time on stopped session

- End time is set to when you stopped
- If stopped late, manually edit the end time in the TUI

### CLI and TUI showing different data

- The TUI caches data and auto-reloads every 500ms
- Wait a moment or restart the TUI to see latest changes
