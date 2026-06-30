# Rust Migration

The Rust implementation is now the installed `disk-agent` command. Host parity
has been validated in the Arch development environment, and the final Ubuntu
migration replaces the obsolete Python launcher with Cargo's installed binary.

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

Final migration steps:

- Rename the Rust package and binary to `disk-agent`.
- Install with `cargo install --path . --locked`.
- Remove `~/.local/bin/disk-agent`, the obsolete Python launcher that masked
  Cargo's binary earlier in PATH.
- Verify `disk-agent` resolves to `~/.cargo/bin/disk-agent` on Ubuntu.

The Python implementation files remain in the repository as legacy reference
material until a separate cleanup explicitly removes them.
