from __future__ import annotations

from pathlib import Path
from typing import Iterable, List

from .models import DirectoryUsage, Snapshot, load_snapshot
from .snapshot import SNAPSHOT_DIR


def format_bytes(value: int | None, signed: bool = False) -> str:
    if value is None:
        return "unavailable"
    sign = ""
    if signed:
        sign = "+" if value >= 0 else "-"
        value = abs(value)
    units = ["B", "K", "M", "G", "T", "P"]
    amount = float(value)
    for unit in units:
        if amount < 1024 or unit == units[-1]:
            rendered = f"{amount:.1f}".rstrip("0").rstrip(".")
            return f"{sign}{rendered}{unit}"
        amount /= 1024
    return f"{sign}{value}B"


def snapshot_paths(directory: Path = SNAPSHOT_DIR) -> List[Path]:
    return sorted(directory.glob("????-??-??.json")) if directory.exists() else []


def latest_snapshot(directory: Path = SNAPSHOT_DIR) -> Snapshot:
    paths = snapshot_paths(directory)
    if not paths:
        raise RuntimeError("no snapshots found; run 'disk-agent snapshot' first")
    return load_snapshot(paths[-1])


def _top_consumers(snapshot: Snapshot, limit: int = 5) -> Iterable[DirectoryUsage]:
    # Immediate children of home provide a stable, non-overlapping summary.
    return sorted(
        (item for item in snapshot.home_usage if item.path != "~" and item.path.count("/") == 1),
        key=lambda item: item.bytes,
        reverse=True,
    )[:limit]


def render_report(snapshot: Snapshot) -> str:
    fs = snapshot.filesystem
    lines = [
        f"Filesystem usage: {fs.used_percent}% ({format_bytes(fs.used_bytes)} of {format_bytes(fs.total_bytes)})",
        "",
        "Top consumers:",
        "",
    ]
    consumers = list(_top_consumers(snapshot))
    lines.extend(f"{format_bytes(item.bytes)} {item.path}" for item in consumers)
    if not consumers:
        lines.append("No directory data available.")

    lines.extend(["", "Podman:", ""])
    if snapshot.podman.available:
        lines.extend(
            [
                f"Images: {format_bytes(snapshot.podman.images_bytes)}",
                f"Containers: {format_bytes(snapshot.podman.containers_bytes)}",
                f"Volumes: {format_bytes(snapshot.podman.volumes_bytes)}",
            ]
        )
    else:
        lines.append(f"Unavailable ({snapshot.podman.error or 'unknown error'}).")

    lines.extend(["", "Largest directories:", ""])
    lines.extend(f"{format_bytes(item.bytes)} {item.path}" for item in snapshot.largest_directories[:10])
    lines.extend(["", "Assessment:"])
    if fs.used_percent >= 90:
        lines.append("Disk usage is critical.")
    elif fs.used_percent >= 80:
        lines.append("Disk usage is elevated.")
    else:
        lines.append("No action required.")
    return "\n".join(lines)


def report_command() -> str:
    return render_report(latest_snapshot())

