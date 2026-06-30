# Rust Migration

The migration keeps a reviewable Rust binary named `disk-agent-rs` beside the
existing Python implementation until host parity is approved.

Implemented in Rust:

- Rust crate scaffold
- CLI skeleton
- JSON-compatible data models
- snapshot loading
- snapshot saving with temporary file rename
- report command
- saved-snapshot diff and explain commands
- live command execution with timeout and `LC_ALL=C`
- live `df` collection
- live `du` collection with partial-data warning handling
- `podman system df` parsing
- Podman rootless storage fallback inspection
- live `snapshot` command
- live `investigate` command
- rule parser compatibility for embedded rule files
- tests for Python snapshot fixtures
- tests for live collection edge cases

Still pending before replacing the installed `disk-agent` command:

- Review live Rust output on Ubuntu 24.04 and Arch against the Python behavior
  oracle.
- Rename the Rust binary from `disk-agent-rs` to `disk-agent`.
- Install with `cargo install --path . --locked` or an equivalent explicit
  install step.
- Remove the legacy Python launcher after the installed command is confirmed to
  resolve to the Rust binary.
- Archive or delete Python implementation files after the Rust binary is the
  operational interface.
