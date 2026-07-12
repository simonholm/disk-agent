# disk-agent

A small, bounded, read-only Linux disk usage observer. It takes one snapshot per
invocation, stores at most one JSON snapshot per day, and never deletes or
modifies user data.

## Install

```sh
cargo install --path . --locked
```

## Use

```sh
disk-agent snapshot
disk-agent snapshot --verbose
disk-agent report
disk-agent report --refresh
disk-agent diff
disk-agent explain
disk-agent investigate
```

## Implementation

`disk-agent` is now the Rust implementation. The obsolete Python launcher has
been removed from `~/.local/bin`; the installed command should resolve to
Cargo's `~/.cargo/bin/disk-agent`.

The retired Python implementation is kept under `legacy/python/` for historical
reference only.

Validation:

```sh
CARGO_TARGET_DIR=target cargo test
cargo install --path . --locked
which -a disk-agent
disk-agent report
```

The Rust binary supports JSON-compatible snapshot loading and saving, report
rendering, saved-snapshot diff/explain logic, live snapshot collection, Podman
usage collection, and live investigation.

Snapshots are stored in `~/.disk-agent/snapshots/YYYY-MM-DD.json`.

`disk-agent report` reads the latest saved snapshot and identifies its timestamp
and source path in the output. Use `disk-agent report --refresh` to collect and
save a fresh snapshot before reporting. Snapshots continue to use one file per
day, so a refreshed report on the same day overwrites that day's snapshot.

Collection is finite and local: filesystem statistics, bounded-depth `du`
scans, and Podman usage from `podman system df` or rootless Podman storage when
the binary is unavailable. Permission errors and unavailable optional paths or
Podman are recorded without failing the snapshot. Expected `du` permission/read
warnings are quiet by default; if collection ignores unexpected warnings,
`disk-agent snapshot` reports their count. Use `disk-agent snapshot --verbose`
to print all warning details.

`disk-agent investigate` loads the latest snapshot, collects a fresh read-only
snapshot, compares the two, classifies significant growth with explicit rules,
and prints an operational assessment with informational recommendations only.
It never deletes files, prunes Podman, clears caches, or runs cleanup commands.

`disk-agent explain` compares the latest two snapshots and attributes broad
growth to changed child directories when the snapshot data supports it:

```text
Disk usage increased from 66% to 69%.

Top contributors:
  +430M ~/labs/archive
  +180M ~/.cache/pip
  +110M ~/Downloads

Cause:
Growth is primarily due to Development (+430M) and Cache (+180M).

Risk:
Low

Recommendations:
Review recent build artifacts or repository changes if unexpected.
Cache growth is usually safe to inspect later; no cleanup is required now.
Review ~/Downloads if the increase was unexpected.
No cleanup is currently required.

Podman comparison unavailable.
```

Path classification is rule-based and deterministic. Current categories include
Cache, Rust, Node, Trash, Podman, Downloads, Photos, Media, Development, and
System logs. Unmatched paths are reported as unclassified.

## Retired Bash scripts

The previous standalone Bash commands, `disk-snapshot` and `disk-report`, have
been retired in favor of the installed Rust `disk-agent` command. On this VPS
they are preserved as `~/.local/bin/disk-snapshot.bak` and
`~/.local/bin/disk-report.bak` for comparison and rollback.
