from __future__ import annotations

import json
from dataclasses import asdict, dataclass, field
from pathlib import Path
from typing import Any, Dict, List, Optional


@dataclass
class DirectoryUsage:
    path: str
    bytes: int


@dataclass
class FilesystemUsage:
    filesystem: str
    mountpoint: str
    total_bytes: int
    used_bytes: int
    available_bytes: int
    used_percent: int


@dataclass
class PodmanUsage:
    available: bool = False
    images_bytes: Optional[int] = None
    containers_bytes: Optional[int] = None
    volumes_bytes: Optional[int] = None
    error: Optional[str] = None


@dataclass
class Snapshot:
    timestamp: str
    filesystem: FilesystemUsage
    home_usage: List[DirectoryUsage] = field(default_factory=list)
    local_share_usage: List[DirectoryUsage] = field(default_factory=list)
    copilot_usage: List[DirectoryUsage] = field(default_factory=list)
    podman: PodmanUsage = field(default_factory=PodmanUsage)
    largest_directories: List[DirectoryUsage] = field(default_factory=list)
    warnings: List[str] = field(default_factory=list)
    schema_version: int = 1

    def to_dict(self) -> Dict[str, Any]:
        return asdict(self)

    @classmethod
    def from_dict(cls, data: Dict[str, Any]) -> "Snapshot":
        def usages(key: str) -> List[DirectoryUsage]:
            return [DirectoryUsage(**item) for item in data.get(key, [])]

        return cls(
            timestamp=data["timestamp"],
            filesystem=FilesystemUsage(**data["filesystem"]),
            home_usage=usages("home_usage"),
            local_share_usage=usages("local_share_usage"),
            copilot_usage=usages("copilot_usage"),
            podman=PodmanUsage(**data.get("podman", {})),
            largest_directories=usages("largest_directories"),
            warnings=data.get("warnings", []),
            schema_version=data.get("schema_version", 1),
        )


def load_snapshot(path: Path) -> Snapshot:
    with path.open(encoding="utf-8") as handle:
        return Snapshot.from_dict(json.load(handle))

