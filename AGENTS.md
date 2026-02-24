# Agent Guidelines for WorkTimer TUI

## GitHub Issues Management

**IMPORTANT: GitHub CLI is READ-ONLY**
- **DO NOT** comment on, close, or modify GitHub issues
- **DO NOT** use commands like `gh issue comment`, `gh issue close`, `gh pr comment`, etc.
- **ONLY** use `gh issue list`, `gh issue view`, `gh pr list`, `gh pr view` for reading
- After implementing fixes, inform the user so they can manage the issue themselves

## Task Management

### TASKS.md Usage
When working on any feature or bug fix:
1. **Always read TASKS.md first** to check current status
2. **Add new tasks** when starting work - break down complex work into clear steps
3. **Update status in real-time** as you progress:
   - `[ ]` Pending → `[~]` In Progress → `[x]` Completed
4. **Move completed tasks** to the "Completed Tasks" section with timestamp
5. **Add context notes** for important decisions, blockers, or next steps
6. **Keep tasks specific and actionable** - prefer multiple small tasks over one large task

### Task Workflow Example
```markdown
## Current Sprint

### Feature: Add CSV Export
- [~] Create CSV export module in `src/export/csv.rs`
- [ ] Add export command to CLI
- [ ] Add tests for CSV formatting
- [ ] Update README with export documentation
```

## Build/Test/Lint Commands
- **Build**: `cargo build`
- **Run**: `cargo run`
- **Test**: `cargo test` (run all tests) or `cargo test <test_name>` (single test)
- **Check**: `cargo check` (fast type checking without compilation)
- **Clippy**: `cargo clippy` (linting)

## Code Style
- **Language**: Rust 2024 edition
- **Error Handling**: Use `anyhow::Result` for functions, `anyhow::Context` for error context
- **Imports**: Group by std → external crates → internal modules, separated by blank lines
- **Types**: Prefer explicit types on public APIs; use `pub` fields for simple data structs
- **Naming**: `snake_case` for variables/functions, `PascalCase` for types/enums, `SCREAMING_SNAKE_CASE` for constants
- **String Handling**: Use `.to_string()` for owned strings, `&str` for borrowed; trim user input
- **Time Format**: Use `time` crate (`TimePoint` format: `HH:MM`, validates 0-23h, 0-59m)
- **Serialization**: Use `serde` with `Serialize`/`Deserialize` derives; pretty-print JSON with `serde_json::to_string_pretty`
- **State Management**: Mutable methods on structs (e.g., `&mut self`); validate before mutating
- **UI Pattern**: Separate state (`app_state.rs`) from rendering (`render.rs`); use ratatui widgets
- **Module Structure**: Each module exports types via `pub use` in `mod.rs`
- **Data Storage**: SQLite database (`work-tuimer.db`) in app data dir, keep legacy JSON files on disk after migration
