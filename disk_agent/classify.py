from __future__ import annotations

from dataclasses import dataclass
from importlib import resources
from typing import Iterable, List


@dataclass(frozen=True)
class Rule:
    pattern: str
    classification: str
    risk: str
    explanation: str
    recommendation: str


@dataclass(frozen=True)
class Classification:
    path: str
    classification: str
    risk: str
    explanation: str
    recommendation: str
    known: bool


UNKNOWN = Classification(
    path="",
    classification="Unknown growth",
    risk="Unknown",
    explanation="No matching rule is available for this location.",
    recommendation="Inspect this location before taking any cleanup action.",
    known=False,
)


def _parse_rule_document(text: str) -> Rule:
    values: dict[str, str] = {}
    current_key: str | None = None
    block_lines: list[str] = []

    def finish_block() -> None:
        nonlocal current_key, block_lines
        if current_key is not None:
            values[current_key] = "\n".join(line.rstrip() for line in block_lines).strip()
        current_key = None
        block_lines = []

    for raw_line in text.splitlines():
        line = raw_line.rstrip()
        if not line.strip() or line.lstrip().startswith("#"):
            continue
        if current_key is not None:
            if line.startswith("  "):
                block_lines.append(line[2:])
                continue
            finish_block()
        key, separator, value = line.partition(":")
        if not separator:
            continue
        key = key.strip()
        value = value.strip()
        if value == "|":
            current_key = key
            block_lines = []
        else:
            values[key] = value
    finish_block()

    missing = {"pattern", "classification", "risk", "explanation"} - values.keys()
    if missing:
        raise ValueError(f"rule is missing required field(s): {', '.join(sorted(missing))}")
    return Rule(
        pattern=values["pattern"],
        classification=values["classification"],
        risk=values["risk"],
        explanation=values["explanation"],
        recommendation=values.get("recommendation", "None."),
    )


def _parse_rules(text: str) -> List[Rule]:
    documents = [part.strip() for part in text.split("\n---") if part.strip()]
    return [_parse_rule_document(document) for document in documents]


def load_rules() -> List[Rule]:
    rule_dir = resources.files("disk_agent").joinpath("rules")
    rules: list[Rule] = []
    for entry in sorted(rule_dir.iterdir()):
        if entry.name.endswith((".yaml", ".yml")):
            rules.extend(_parse_rules(entry.read_text(encoding="utf-8")))
    return sorted(rules, key=lambda rule: len(rule.pattern), reverse=True)


def classify_path(path: str, rules: Iterable[Rule] | None = None) -> Classification:
    rules = list(rules or load_rules())
    for rule in rules:
        if path == rule.pattern or path.startswith(f"{rule.pattern}/"):
            return Classification(
                path=path,
                classification=rule.classification,
                risk=rule.risk,
                explanation=rule.explanation,
                recommendation=rule.recommendation,
                known=True,
            )
    return Classification(
        path=path,
        classification=UNKNOWN.classification,
        risk=UNKNOWN.risk,
        explanation=UNKNOWN.explanation,
        recommendation=UNKNOWN.recommendation,
        known=UNKNOWN.known,
    )


def is_child_path(child: str, parent: str) -> bool:
    return child.startswith(f"{parent}/")
