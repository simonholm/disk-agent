# disk-agent

`disk-agent` is a bounded, read-only Linux disk usage observer. It records daily
disk usage snapshots, compares recent snapshots, explains significant changes,
and runs live diagnostics using deterministic rules.

It exists for routine disk triage on a VPS or workstation: show what changed,
classify likely causes, and provide enough context for a human to decide what to
inspect next. It observes and explains; it does not clean up, delete, prune, or
modify the system.

## Philosophy

- Bounded: collection uses finite, local checks and bounded-depth directory
  scans.
- Read-only: commands observe filesystem and Podman usage without changing user
  data.
- Deterministic: reports and explanations come from saved snapshots and explicit
  rules, not LLMs or remote services.
- Daily snapshots: snapshots are stored as one JSON file per day in
  `~/.disk-agent/snapshots/YYYY-MM-DD.json`.
- Separate history from diagnosis: use saved snapshots for historical changes
  and fresh read-only scans for live investigation.
- No automatic cleanup: recommendations are informational only.

## Install

```sh
cargo install --path . --locked
```

The installed command should resolve to Cargo's `~/.cargo/bin/disk-agent`:

```sh
which -a disk-agent
```

## Commands

```sh
disk-agent snapshot            # collect and store today's snapshot
disk-agent snapshot --verbose  # show all ignored collection warnings
disk-agent report              # summarize the latest saved snapshot
disk-agent report --refresh    # collect a fresh snapshot, then summarize it
disk-agent diff                # compare the latest two daily snapshots
disk-agent explain             # explain significant changes between snapshots
disk-agent investigate         # inspect current disk usage and diagnostic signals
```

## Example workflow

```sh
disk-agent snapshot
disk-agent report
disk-agent diff
disk-agent explain
disk-agent investigate
```

`snapshot` saves today's baseline. `report` summarizes the latest saved state.
`diff` shows what changed between the latest two daily snapshots. `explain`
classifies significant changes when the snapshot data supports it.
`investigate` collects fresh read-only evidence and prints a current-state
operational assessment. If today's snapshot already exists, it may show
same-day activity as "Changes since today's snapshot"; historical comparison
remains the job of `diff`.

## Safety

`disk-agent` never deletes files, never prunes Podman data, never clears caches,
and never modifies the filesystem as a cleanup action. It performs bounded
observation only. Snapshot commands write JSON snapshot files under
`~/.disk-agent/snapshots/`; diagnostic recommendations remain informational.

## What's new in v0.3.0

- Quieter `snapshot` output: expected `du` permission/read warnings are
  suppressed by default.
- `disk-agent snapshot --verbose` still exposes all ignored warning details.
- Day-to-day use is less noisy while preserving the same read-only collection
  behavior.

## Implementation

`disk-agent` is the Rust implementation. The retired Python implementation is
kept under `legacy/python/` for historical reference only.

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

`disk-agent investigate` collects a fresh read-only snapshot, reports current
filesystem usage, largest consumers, same-day activity when detectable, Podman
status, an assessment, and informational recommendations. It may use today's
saved snapshot as a same-day baseline, but it does not present itself as a
historical snapshot comparison.

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
