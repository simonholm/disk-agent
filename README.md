# disk-agent

A small, bounded, read-only Linux disk usage observer. It takes one snapshot per
invocation, stores at most one JSON snapshot per day, and never deletes or
modifies user data.

## Install

```sh
python3 -m pip install --user -e .
```

## Use

```sh
disk-agent snapshot
disk-agent report
disk-agent diff
disk-agent explain
disk-agent investigate
```

Snapshots are stored in `~/.disk-agent/snapshots/YYYY-MM-DD.json`.

Collection is finite and local: filesystem statistics, bounded-depth `du`
scans, and Podman usage from `podman system df` or rootless Podman storage when
the binary is unavailable. Permission errors and unavailable optional paths or
Podman are recorded without failing the snapshot.

`disk-agent investigate` loads the latest snapshot, collects a fresh read-only
snapshot, compares the two, classifies significant growth with explicit rules,
and prints an operational assessment with informational recommendations only.
It never deletes files, prunes Podman, clears caches, or runs cleanup commands.

## Retired Bash scripts

The previous standalone Bash commands, `disk-snapshot` and `disk-report`, have
been retired in favor of the installed Python `disk-agent` command. On this VPS
they are temporarily preserved as `~/.local/bin/disk-snapshot.bak` and
`~/.local/bin/disk-report.bak` for comparison and rollback while the Python
implementation is confirmed as the sole operational version.
