# Snapshot Format

`disk-agent` reads and writes a stable JSON snapshot schema.

Required fields:

- `timestamp`
- `filesystem`

Optional fields have stable defaults when absent:

- `home_usage`: `[]`
- `local_share_usage`: `[]`
- `copilot_usage`: `[]`
- `podman`: unavailable/default object
- `largest_directories`: `[]`
- `warnings`: `[]`
- `schema_version`: `1`

Snapshot files are stored as:

```text
~/.disk-agent/snapshots/YYYY-MM-DD.json
```

Rust writes pretty JSON with deterministic struct field order. Consumers should
treat JSON object key order as insignificant.
