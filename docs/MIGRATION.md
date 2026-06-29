# Rust Migration

Phase 1 creates a reviewable Rust binary named `disk-agent-rs` beside the
existing Python implementation.

Completed in phase 1:

- Rust crate scaffold
- CLI skeleton
- JSON-compatible data models
- snapshot loading
- snapshot saving with temporary file rename
- report command
- saved-snapshot diff and explain commands
- rule parser compatibility for embedded rule files
- tests for Python snapshot fixtures

Not completed in phase 1:

- live `df` collection
- live `du` collection
- `podman system df`
- Podman storage fallback inspection
- live `snapshot` command
- live `investigate` command

The next migration step is to port `command.rs`, then `filesystem.rs`, then
`podman.rs`. After that, `snapshot.rs` can orchestrate live collection without
mixing Python and Rust at runtime.
