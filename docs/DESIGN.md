# disk-agent Rust Design

The Rust implementation is a side-by-side rewrite of the existing Python
utility. During migration the Python implementation remains the behavior oracle.

The temporary binary is `disk-agent-rs`. It does not import, execute, or depend
on Python at runtime.

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
`investigate`. The temporary binary name remains until the installed Python
launcher can be replaced deliberately.
