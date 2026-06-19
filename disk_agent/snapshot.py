from __future__ import annotations

import json
import os
import shutil
import subprocess
from datetime import datetime, timezone
from pathlib import Path
from typing import List, Sequence, Tuple

from .models import DirectoryUsage, FilesystemUsage, PodmanUsage, Snapshot

SNAPSHOT_DIR = Path.home() / ".disk-agent" / "snapshots"
COMMAND_TIMEOUT_SECONDS = 120
TOP_DIRECTORY_DEPTH = 3


def _display_path(path: Path) -> str:
    home = Path.home()
    try:
        relative = path.resolve().relative_to(home.resolve())
    except (OSError, ValueError):
        return str(path)
    return "~" if str(relative) == "." else f"~/{relative}"


def _run(command: Sequence[str]) -> Tuple[str, str, int]:
    try:
        result = subprocess.run(
            command,
            text=True,
            capture_output=True,
            timeout=COMMAND_TIMEOUT_SECONDS,
            check=False,
            env={**os.environ, "LC_ALL": "C"},
        )
        return result.stdout, result.stderr, result.returncode
    except subprocess.TimeoutExpired as exc:
        return exc.stdout or "", f"command timed out after {COMMAND_TIMEOUT_SECONDS}s", 124
    except OSError as exc:
        return "", str(exc), 127


def _collect_filesystem() -> FilesystemUsage:
    stdout, stderr, code = _run(["df", "-B1", "--output=source,size,used,avail,pcent,target", "/"])
    lines = [line.split() for line in stdout.splitlines()[1:] if line.strip()]
    if code != 0 or not lines or len(lines[-1]) < 6:
        raise RuntimeError(f"df failed: {stderr.strip() or 'invalid output'}")
    filesystem, total, used, available, percent = lines[-1][:5]
    mountpoint = " ".join(lines[-1][5:])
    return FilesystemUsage(
        filesystem=filesystem,
        mountpoint=mountpoint,
        total_bytes=int(total),
        used_bytes=int(used),
        available_bytes=int(available),
        used_percent=int(percent.rstrip("%")),
    )


def _collect_du(path: Path, max_depth: int) -> Tuple[List[DirectoryUsage], List[str]]:
    if not path.exists():
        return [], [f"path not found: {_display_path(path)}"]
    stdout, stderr, code = _run(
        ["du", "-x", "-B1", f"--max-depth={max_depth}", str(path)]
    )
    values: List[DirectoryUsage] = []
    for line in stdout.splitlines():
        size, separator, raw_path = line.partition("\t")
        if not separator:
            continue
        try:
            values.append(DirectoryUsage(_display_path(Path(raw_path)), int(size)))
        except ValueError:
            continue
    warnings = []
    if code != 0 and stderr.strip():
        # du commonly returns 1 for unreadable paths. Keep useful partial data.
        warnings.append(f"du {_display_path(path)}: permission or read errors ignored")
    return values, warnings


def _parse_size(value: str) -> int:
    value = value.strip()
    if not value or value == "0B":
        return 0
    units = {"B": 1, "kB": 1000, "KB": 1000, "MB": 1000**2, "GB": 1000**3, "TB": 1000**4}
    for suffix in sorted(units, key=len, reverse=True):
        if value.endswith(suffix):
            try:
                return int(float(value[: -len(suffix)]) * units[suffix])
            except ValueError:
                return 0
    return 0


def _collect_podman() -> PodmanUsage:
    if shutil.which("podman") is None:
        return PodmanUsage(error="podman is not installed")
    stdout, stderr, code = _run(["podman", "system", "df", "--format", "{{json .}}"])
    if code != 0:
        return PodmanUsage(error=stderr.strip() or "podman system df failed")
    totals = {"Images": 0, "Containers": 0, "Local Volumes": 0}
    try:
        for line in stdout.splitlines():
            row = json.loads(line)
            kind = row.get("Type")
            if kind in totals:
                totals[kind] = _parse_size(str(row.get("Size", "0B")))
    except (json.JSONDecodeError, TypeError):
        return PodmanUsage(error="could not parse podman system df output")
    return PodmanUsage(
        available=True,
        images_bytes=totals["Images"],
        containers_bytes=totals["Containers"],
        volumes_bytes=totals["Local Volumes"],
    )


def collect_snapshot(now: datetime | None = None) -> Snapshot:
    now = now or datetime.now(timezone.utc).astimezone()
    home = Path.home()
    home_usage, warnings = _collect_du(home, 2)
    local_usage, local_warnings = _collect_du(home / ".local" / "share", 2)
    copilot_usage, copilot_warnings = _collect_du(home / ".copilot", 2)
    top_usage, top_warnings = _collect_du(home, TOP_DIRECTORY_DEPTH)
    warnings.extend(local_warnings + copilot_warnings + top_warnings)

    # Dedupe paths and retain the most precise observed value.
    by_path = {}
    for usage in top_usage + local_usage + copilot_usage:
        by_path[usage.path] = usage
    largest = sorted(by_path.values(), key=lambda item: item.bytes, reverse=True)
    largest = [item for item in largest if item.path != "~"][:100]

    return Snapshot(
        timestamp=now.isoformat(timespec="seconds"),
        filesystem=_collect_filesystem(),
        home_usage=home_usage,
        local_share_usage=local_usage,
        copilot_usage=copilot_usage,
        podman=_collect_podman(),
        largest_directories=largest,
        warnings=warnings,
    )


def save_snapshot(snapshot: Snapshot, directory: Path = SNAPSHOT_DIR) -> Path:
    directory.mkdir(parents=True, exist_ok=True)
    day = snapshot.timestamp[:10]
    destination = directory / f"{day}.json"
    temporary = destination.with_suffix(".json.tmp")
    with temporary.open("w", encoding="utf-8") as handle:
        json.dump(snapshot.to_dict(), handle, indent=2, sort_keys=True)
        handle.write("\n")
    temporary.replace(destination)
    return destination


def snapshot_command() -> str:
    snapshot = collect_snapshot()
    path = save_snapshot(snapshot)
    message = f"Snapshot stored: {path}"
    if snapshot.warnings:
        message += f"\nCompleted with {len(snapshot.warnings)} ignored warning(s)."
    return message
