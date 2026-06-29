# Snapshot Format

Rust reads the current Python-generated JSON snapshot schema.

Required fields:

- `timestamp`
- `filesystem`

Optional fields default like the Python `Snapshot.from_dict` implementation:

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

Rust writes pretty JSON with deterministic struct field order. Exact key sorting
is not required for compatibility.
