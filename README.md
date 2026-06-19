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
```

The equivalent standalone commands are `disk-snapshot`, `disk-report`,
`disk-diff`, and `disk-explain`. Snapshots are stored in
`~/.disk-agent/snapshots/YYYY-MM-DD.json`.

Collection is finite and local: filesystem statistics, bounded-depth `du`
scans, and `podman system df`. Permission errors and unavailable optional paths
or Podman are recorded without failing the snapshot.

