# disk-agent Rust Design

The Rust implementation is the operational implementation of the disk-agent
utility.

The binary is `disk-agent`. It does not import, execute, or depend on Python at
runtime.

## Layers

`main.rs` and `cli.rs` parse arguments and dispatch commands.

Business logic lives in `snapshot.rs`, `report.rs`, `diff.rs`, `explain.rs`,
`investigate.rs`, and `classify.rs`.

Linux-specific and runtime infrastructure lives in `command.rs`,
`filesystem.rs`, `podman.rs`, `json.rs`, `paths.rs`, `output.rs`, and
`errors.rs`.

The Rust implementation includes JSON compatibility, snapshot loading/saving,
report rendering, saved-snapshot diff/explain logic, live command execution,
filesystem collection, Podman collection, live `snapshot`, and live
`investigate`.
