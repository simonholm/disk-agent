from __future__ import annotations

from dataclasses import dataclass
from pathlib import Path
from typing import Dict, List, Tuple

from .models import DirectoryUsage, Snapshot, load_snapshot
from .report import format_bytes, snapshot_paths
from .snapshot import SNAPSHOT_DIR

SIGNIFICANT_BYTES = 50 * 1024 * 1024


@dataclass
class UsageChange:
    path: str
    bytes: int


def latest_two(directory: Path = SNAPSHOT_DIR) -> Tuple[Snapshot, Snapshot]:
    paths = snapshot_paths(directory)
    if len(paths) < 2:
        raise RuntimeError("two snapshots are required; snapshots are stored once per day")
    return load_snapshot(paths[-2]), load_snapshot(paths[-1])


def _usage_map(snapshot: Snapshot) -> Dict[str, int]:
    result: Dict[str, int] = {}
    for items in (snapshot.home_usage, snapshot.local_share_usage, snapshot.copilot_usage):
        for item in items:
            result[item.path] = item.bytes
    return result


def compare_snapshots(before: Snapshot, after: Snapshot) -> List[UsageChange]:
    old = _usage_map(before)
    new = _usage_map(after)
    changes = [
        UsageChange(path, new.get(path, 0) - old.get(path, 0))
        for path in old.keys() | new.keys()
    ]
    return sorted(
        (change for change in changes if abs(change.bytes) >= SIGNIFICANT_BYTES),
        key=lambda change: abs(change.bytes),
        reverse=True,
    )


def render_diff(before: Snapshot, after: Snapshot) -> str:
    changes = compare_snapshots(before, after)
    growth = [change for change in changes if change.bytes > 0]
    shrinkage = [change for change in changes if change.bytes < 0]
    lines = ["Compared:", f"{before.timestamp[:10]} → {after.timestamp[:10]}", "", "Growth:", ""]
    lines.extend(f"{format_bytes(change.bytes, signed=True)} {change.path}" for change in growth)
    if not growth:
        lines.append("No significant growth.")
    lines.extend(["", "Shrinkage:", ""])
    if shrinkage:
        lines.extend(f"{format_bytes(change.bytes, signed=True)} {change.path}" for change in shrinkage)
    else:
        lines.append("No significant shrinkage.")
    lines.extend(["", "No other significant changes."])
    return "\n".join(lines)


def diff_command() -> str:
    return render_diff(*latest_two())
