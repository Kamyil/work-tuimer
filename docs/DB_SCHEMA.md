# Database Schema Contract

This document describes the SQLite schema contract for external scripts/plugins.

## Goals

- Keep app storage simple and stable.
- Provide a safe read-only integration surface.
- Allow future internal changes without breaking plugin queries.

## Database Location

WorkTUImer stores data in:

1. `<dirs::data_local_dir()>/work-tuimer/work-tuimer.db`
2. `./data/work-tuimer.db` (fallback)

Examples:
- Linux: `~/.local/share/work-tuimer/work-tuimer.db`
- macOS: `~/Library/Application Support/work-tuimer/work-tuimer.db`
- Windows: `%LOCALAPPDATA%\work-tuimer\work-tuimer.db`

## Versioning Metadata

The `meta` table exposes schema compatibility values:

- `schema.version` (current schema version)
- `schema.min_compatible_version` (minimum reader version)
- `schema.contract` (stable integration contract string)
- `migration.json_to_sqlite.v1` (legacy JSON migration marker)

SQLite `PRAGMA user_version` is also set and kept in sync with `schema.version`.

## Base Tables

- `meta(key TEXT PRIMARY KEY, value TEXT NOT NULL)`
- `day_meta(date TEXT PRIMARY KEY, last_id INTEGER, revision INTEGER)`
- `work_records(date TEXT, id INTEGER, name TEXT, start_minutes INTEGER, end_minutes INTEGER, total_minutes INTEGER, description TEXT, PRIMARY KEY(date, id))`
- `active_timer(singleton_id INTEGER PRIMARY KEY CHECK(singleton_id = 1), ...timer state columns...)`

## Stable Read Models (Recommended)

Use these views for integrations when possible:

- `v_work_records`
  - Includes stable per-record fields plus `record_uid` (`<date>:<record_id>`)
  - Includes `start_time` / `end_time` formatted as `HH:MM`
- `v_daily_totals`
  - One row per date with `records_count` and `total_minutes`
- `v_active_timer`
  - Current active timer row (if any)

## Compatibility Policy

- Prefer querying views (`v_*`) over base tables.
- Minor releases may add base-table columns/indexes.
- Existing view columns are treated as stable contract.
- Integrations should default to read-only access.

## Example Queries

```sql
-- Read schema metadata
SELECT key, value
FROM meta
WHERE key IN ('schema.version', 'schema.min_compatible_version', 'schema.contract');
```

```sql
-- Get daily totals (newest first)
SELECT date, records_count, total_minutes
FROM v_daily_totals
ORDER BY date DESC;
```

```sql
-- Get records for one day
SELECT record_uid, name, start_time, end_time, total_minutes, description
FROM v_work_records
WHERE date = '2026-02-24'
ORDER BY record_id;
```

```sql
-- Check active timer
SELECT task_name, status, start_time, paused_duration_secs
FROM v_active_timer;
```
