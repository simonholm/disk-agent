from __future__ import annotations

from typing import Callable

import typer
from rich.console import Console

from .diff import diff_command
from .explain import explain_command
from .investigate import investigate_command
from .report import report_command
from .snapshot import snapshot_command

COMMANDS: dict[str, Callable[[], str]] = {
    "snapshot": snapshot_command,
    "report": report_command,
    "diff": diff_command,
    "explain": explain_command,
    "investigate": investigate_command,
}

app = typer.Typer(
    add_completion=False,
    help="Bounded, read-only disk usage observer.",
    no_args_is_help=True,
)
console = Console()
error_console = Console(stderr=True)


def _run(command: str) -> int:
    try:
        console.print(COMMANDS[command](), markup=False)
        return 0
    except (RuntimeError, OSError, ValueError) as exc:
        error_console.print(f"disk-agent: {exc}", markup=False)
        return 1


@app.command("snapshot")
def snapshot_cli() -> None:
    """Collect and store today's disk usage snapshot."""
    raise typer.Exit(_run("snapshot"))


@app.command("report")
def report_cli() -> None:
    """Show the latest snapshot."""
    raise typer.Exit(_run("report"))


@app.command("diff")
def diff_cli() -> None:
    """Compare the latest two daily snapshots."""
    raise typer.Exit(_run("diff"))


@app.command("explain")
def explain_cli() -> None:
    """Explain the latest significant changes."""
    raise typer.Exit(_run("explain"))


@app.command("investigate")
def investigate_cli() -> None:
    """Collect evidence and produce a bounded diagnostic report."""
    raise typer.Exit(_run("investigate"))


def main() -> None:
    app()


def snapshot_main() -> int:
    return _run("snapshot")


def report_main() -> int:
    return _run("report")


def diff_main() -> int:
    return _run("diff")


def explain_main() -> int:
    return _run("explain")


def investigate_main() -> int:
    return _run("investigate")


if __name__ == "__main__":
    main()
