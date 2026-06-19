from __future__ import annotations

from .diff import compare_snapshots, latest_two
from .models import Snapshot
from .report import format_bytes


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


def render_explanation(before: Snapshot, after: Snapshot) -> str:
    old_percent = before.filesystem.used_percent
    new_percent = after.filesystem.used_percent
    filesystem_delta = after.filesystem.used_bytes - before.filesystem.used_bytes
    verb = "increased" if new_percent > old_percent else "decreased" if new_percent < old_percent else "remained"
    first = (
        f"Disk usage {verb} at {new_percent}% (used space changed by {format_bytes(filesystem_delta, signed=True)})."
        if verb == "remained"
        else f"Disk usage {verb} from {old_percent}% to {new_percent}%."
    )
    growth = [change for change in compare_snapshots(before, after) if change.bytes > 0]
    lines = [first, "", "Largest contributors:", ""]
    lines.extend(f"{format_bytes(change.bytes, signed=True)} {change.path}" for change in growth[:5])
    if not growth:
        lines.append("No significant directory growth.")

    old_podman, new_podman = _podman_total(before), _podman_total(after)
    lines.append("")
    if old_podman is None or new_podman is None:
        lines.append("Podman comparison unavailable.")
    elif abs(new_podman - old_podman) < 50 * 1024 * 1024:
        lines.append("Podman unchanged.")
    else:
        lines.append(f"Podman changed by {format_bytes(new_podman - old_podman, signed=True)}.")

    largest_growth = growth[0].bytes if growth else 0
    unusual = new_percent - old_percent >= 5 or filesystem_delta >= 5 * 1024**3 or largest_growth >= 5 * 1024**3
    lines.extend(["", "Assessment:"])
    if new_percent >= 90:
        assessment = "Growth appears unusual because disk usage is critical."
        recommendation = "Review the largest contributor and decide manually whether its contents are still needed."
    elif unusual:
        assessment = "Growth appears unusual because the change is large for one snapshot interval."
        recommendation = "Review the largest contributor to confirm the growth is expected."
    elif growth:
        assessment = "Growth appears normal and available capacity is not currently constrained."
        recommendation = "No action required."
    else:
        assessment = "No unusual growth was detected."
        recommendation = "No action required."
    lines.extend([assessment, "", "Recommendation:", recommendation])
    return "\n".join(lines)


def explain_command() -> str:
    return render_explanation(*latest_two())
