from __future__ import annotations

from pathlib import Path
from typing import Iterable, List

from .classify import Classification, classify_path, is_child_path, load_rules
from .diff import UsageChange, compare_snapshots
from .models import Snapshot, load_snapshot
from .report import format_bytes, snapshot_paths
from .snapshot import SNAPSHOT_DIR, collect_snapshot, save_snapshot

LARGE_CACHE_BYTES = 2 * 1024**3
LARGE_GROWTH_BYTES = 5 * 1024**3
MODERATE_GROWTH_BYTES = 1024**3


def latest_snapshot_path(directory: Path = SNAPSHOT_DIR) -> Path:
    paths = snapshot_paths(directory)
    if not paths:
        raise RuntimeError("no snapshots found; run 'disk-agent snapshot' first")
    return paths[-1]


def _save_if_new_day(snapshot: Snapshot, directory: Path = SNAPSHOT_DIR) -> Path | None:
    destination = directory / f"{snapshot.timestamp[:10]}.json"
    if destination.exists():
        return None
    return save_snapshot(snapshot, directory)


def _non_overlapping(changes: Iterable[UsageChange]) -> List[UsageChange]:
    rules = load_rules()
    changes = list(changes)
    result: list[UsageChange] = []
    for change in sorted(changes, key=lambda item: abs(item.bytes), reverse=True):
        known_children = [
            child
            for child in changes
            if is_child_path(child.path, change.path)
            and child.bytes > 0
            and classify_path(child.path, rules).known
            and child.bytes >= abs(change.bytes) * 0.5
        ]
        if not classify_path(change.path, rules).known and known_children:
            continue
        if any(is_child_path(change.path, kept.path) for kept in result):
            continue
        result.append(change)
    return result


def _podman_total(snapshot: Snapshot) -> int | None:
    if not snapshot.podman.available:
        return None
    return sum(
        value or 0
        for value in (
            snapshot.podman.images_bytes,
            snapshot.podman.containers_bytes,
            snapshot.podman.volumes_bytes,
        )
    )


def _child_names(snapshot: Snapshot, parent: str) -> list[str]:
    prefix = f"{parent}/"
    names: set[str] = set()
    for usage in snapshot.largest_directories + snapshot.home_usage + snapshot.local_share_usage + snapshot.copilot_usage:
        if not usage.path.startswith(prefix):
            continue
        remainder = usage.path[len(prefix) :]
        if remainder and "/" not in remainder:
            names.add(remainder)
    return sorted(names)


def _codex_releases() -> list[str]:
    releases = Path.home() / ".codex" / "packages" / "standalone" / "releases"
    if not releases.is_dir():
        return []
    return sorted(path.name for path in releases.iterdir() if path.is_dir())


def _cause(change: UsageChange, classification: Classification, snapshot: Snapshot) -> str:
    if change.path == "~/.codex/packages" or change.path.startswith("~/.codex/packages/"):
        versions = _codex_releases() or _child_names(snapshot, "~/.codex/packages")
        if versions:
            count = len(versions)
            noun = "release" if count == 1 else "releases"
            return f"{count} retained Codex {noun}: {', '.join(versions)}."
    return classification.explanation


def _repeated_growth(current: list[UsageChange], previous: list[UsageChange]) -> set[str]:
    previous_growth = {change.path for change in previous if change.bytes > 0}
    repeated = set()
    for change in current:
        if change.bytes <= 0:
            continue
        if change.path in previous_growth:
            repeated.add(change.path)
    return repeated


def _large_known_cache(snapshot: Snapshot) -> bool:
    cache_patterns = ("~/.cargo/registry", "~/.npm/_cacache", "~/.local/share/containers")
    for usage in snapshot.largest_directories + snapshot.home_usage + snapshot.local_share_usage:
        if any(usage.path == pattern or usage.path.startswith(f"{pattern}/") for pattern in cache_patterns):
            if usage.bytes >= LARGE_CACHE_BYTES:
                return True
    return False


def assess(
    before: Snapshot,
    after: Snapshot,
    growth: list[UsageChange],
    classifications: dict[str, Classification],
    repeated: set[str],
) -> str:
    fs = after.filesystem
    total_growth = after.filesystem.used_bytes - before.filesystem.used_bytes
    unknown_growth = any(not classifications[change.path].known for change in growth)
    podman_growth = any(classifications[change.path].classification == "Podman storage" for change in growth)
    largest_growth = max((change.bytes for change in growth), default=0)

    if (
        fs.used_percent >= 90
        or total_growth >= LARGE_GROWTH_BYTES
        or largest_growth >= LARGE_GROWTH_BYTES
        or (unknown_growth and total_growth >= MODERATE_GROWTH_BYTES)
        or (podman_growth and total_growth >= MODERATE_GROWTH_BYTES)
    ):
        return "Attention Recommended"
    if fs.used_percent >= 80 or repeated or unknown_growth or _large_known_cache(after):
        return "Monitor"
    return "Healthy"


def recommendations(
    assessment: str,
    growth: list[UsageChange],
    classifications: dict[str, Classification],
) -> list[str]:
    if assessment == "Healthy" and not growth:
        return ["No action required."]
    seen: set[str] = set()
    items: list[str] = []
    for change in growth:
        recommendation = classifications[change.path].recommendation
        if recommendation not in seen:
            seen.add(recommendation)
            items.append(recommendation)
    if not items:
        items.append("No action required.")
    return items


def render_investigation(
    before: Snapshot,
    after: Snapshot,
    previous_before: Snapshot | None = None,
) -> str:
    rules = load_rules()
    all_changes = compare_snapshots(before, after)
    growth = _non_overlapping(change for change in all_changes if change.bytes > 0)
    previous_changes = compare_snapshots(previous_before, before) if previous_before else []
    repeated = _repeated_growth(growth, previous_changes)
    classifications = {change.path: classify_path(change.path, rules) for change in growth}
    assessment = assess(before, after, growth, classifications, repeated)

    fs = after.filesystem
    lines = [
        f"Filesystem usage: {fs.used_percent}% ({format_bytes(fs.used_bytes)} of {format_bytes(fs.total_bytes)})",
        f"Snapshot interval: {before.timestamp[:10]} to {after.timestamp[:10]}",
        "",
        "Growth:",
        "",
    ]
    if not growth:
        lines.append("No significant growth.")
    for change in growth:
        classification = classifications[change.path]
        lines.extend(
            [
                f"{format_bytes(change.bytes, signed=True)} {change.path}",
                classification.classification,
                _cause(change, classification, after),
                f"Risk: {classification.risk}",
            ]
        )
        if change.path in repeated:
            lines.append("Repeated growth: yes")
        lines.append("")

    old_podman, new_podman = _podman_total(before), _podman_total(after)
    lines.append("Podman:")
    if old_podman is None or new_podman is None:
        lines.append("Comparison unavailable.")
    else:
        lines.append(f"Changed by {format_bytes(new_podman - old_podman, signed=True)}.")

    lines.extend(["", "Assessment", "", assessment, "", "Recommendations", ""])
    lines.extend(recommendations(assessment, growth, classifications))
    return "\n".join(lines).rstrip()


def investigate_command() -> str:
    baseline_path = latest_snapshot_path()
    paths = snapshot_paths()
    previous_before = load_snapshot(paths[-2]) if len(paths) >= 2 else None
    before = load_snapshot(baseline_path)
    after = collect_snapshot()
    saved = _save_if_new_day(after)

    report = render_investigation(before, after, previous_before)
    if saved is None:
        return f"{report}\n\nFresh snapshot was collected in memory; today's snapshot file already exists."
    return f"{report}\n\nSnapshot stored: {saved}"
