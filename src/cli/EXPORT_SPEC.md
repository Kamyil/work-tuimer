# Export CLI Specification

## Goal
Provide a CLI export flow for work records as CSV.

## Command Interface

### Help
- `export -h`
- `export --help`

### Export One Date
- `export date YYYY-MM-DD`

Behavior:
- Exports records for exactly that date.
- If the date has no records, return an error.

### Export a Range
- `export range YYYY-MM-DD YYYY-MM-DD`
- `export range all`

Options:
- `--individual` (optional)

Behavior:
- Default behavior (without `--individual`):
  - Export all records in the selected range into one combined CSV file.
- With `--individual`:
  - Export one CSV file per date.
- `range all` means all available records.
- Range export must only include dates that actually have records.
- If a date in the range has no records, skip it.

### List Available Dates
- `export list-dates`

Behavior:
- Lists dates that are available for export.

### Stdout Mode
For all save/export operations (everything except `list-dates`):
- `--stdout` outputs CSV to stdout instead of saving file(s).

## CSV Data Rules

- Time values use 24-hour format only for now.
- Stored minute-based values must be converted to `HH:MM`.

## CSV Columns and Order
The CSV must use this column order:

```markdown
| date | task name | start time | end time | description | project | total time |
|------|-----------|------------|----------|-------------|---------|------------|
```

## Output UX

When saving files (not using `--stdout`):
- Do not print raw CSV to the terminal.
- Print where export was saved.
- If multiple files were created, printing the directory path and file names is sufficient.

## Notes

- Autocompletion is desired where possible.
