import tempfile
import unittest
from datetime import datetime, timezone
from pathlib import Path

from disk_agent.diff import SIGNIFICANT_BYTES, compare_snapshots, latest_two, render_diff
from disk_agent.explain import render_explanation
from disk_agent.models import DirectoryUsage, FilesystemUsage, PodmanUsage, Snapshot, load_snapshot
from disk_agent.report import render_report
from disk_agent.snapshot import save_snapshot


def sample(day: int, used_percent: int, cache_bytes: int) -> Snapshot:
    return Snapshot(
        timestamp=datetime(2026, 6, day, tzinfo=timezone.utc).isoformat(),
        filesystem=FilesystemUsage("/dev/vda", "/", 1000, used_percent * 10, 1000 - used_percent * 10, used_percent),
        home_usage=[DirectoryUsage("~", 500), DirectoryUsage("~/.cache", cache_bytes)],
        podman=PodmanUsage(True, 100, 200, 300),
        largest_directories=[DirectoryUsage("~/.cache", cache_bytes)],
    )


class DiskAgentTests(unittest.TestCase):
    def test_snapshot_round_trip(self):
        with tempfile.TemporaryDirectory() as temporary:
            original = sample(18, 60, 100)
            path = save_snapshot(original, Path(temporary))
            self.assertEqual(load_snapshot(path), original)

    def test_diff_ignores_changes_under_50_mib(self):
        before = sample(18, 60, 10)
        after = sample(19, 61, 49 * 1024 * 1024)
        self.assertEqual(compare_snapshots(before, after), [])

    def test_diff_includes_threshold_and_sorts_by_absolute_size(self):
        before = sample(18, 60, 0)
        before.home_usage.extend(
            [DirectoryUsage("~/.removed", 200 * 1024 * 1024), DirectoryUsage("~/.exact", 0)]
        )
        after = sample(19, 61, 100 * 1024 * 1024)
        after.home_usage.append(DirectoryUsage("~/.exact", SIGNIFICANT_BYTES))

        changes = compare_snapshots(before, after)

        self.assertEqual(
            [(change.path, change.bytes) for change in changes],
            [
                ("~/.removed", -200 * 1024 * 1024),
                ("~/.cache", 100 * 1024 * 1024),
                ("~/.exact", SIGNIFICANT_BYTES),
            ],
        )

    def test_diff_renders_increases_and_decreases(self):
        before = sample(18, 60, 200 * 1024 * 1024)
        before.home_usage.append(DirectoryUsage("~/.growing", 0))
        after = sample(19, 61, 100 * 1024 * 1024)
        after.home_usage.append(DirectoryUsage("~/.growing", 75 * 1024 * 1024))

        output = render_diff(before, after)

        self.assertIn("Growth:", output)
        self.assertIn("+75M ~/.growing", output)
        self.assertIn("Shrinkage:", output)
        self.assertIn("-100M ~/.cache", output)

    def test_latest_two_requires_two_snapshots(self):
        with tempfile.TemporaryDirectory() as temporary:
            save_snapshot(sample(19, 61, 100), Path(temporary))
            with self.assertRaisesRegex(RuntimeError, "two snapshots are required"):
                latest_two(Path(temporary))

    def test_outputs_are_human_readable_and_one_recommendation(self):
        before = sample(18, 60, 10)
        after = sample(19, 61, 100 * 1024 * 1024)
        self.assertIn("Filesystem usage: 61%", render_report(after))
        self.assertIn("Growth:", render_diff(before, after))
        explanation = render_explanation(before, after)
        self.assertEqual(explanation.count("Recommendation:"), 1)
        self.assertIn("~/.cache", explanation)

    def test_explain_is_concise_with_exactly_one_recommendation(self):
        before = sample(18, 60, 0)
        after = sample(19, 61, 100 * 1024 * 1024)

        output = render_explanation(before, after)

        self.assertIn("Disk usage increased from 60% to 61%.", output)
        self.assertIn("+100M ~/.cache", output)
        self.assertIn("Growth appears normal", output)
        self.assertEqual(output.count("Recommendation:"), 1)

    def test_explain_marks_large_growth_unusual(self):
        before = sample(18, 60, 0)
        after = sample(19, 66, 6 * 1024**3)

        output = render_explanation(before, after)

        self.assertIn("Growth appears unusual", output)
        self.assertIn("Review the largest contributor", output)
        self.assertEqual(output.count("Recommendation:"), 1)

    def test_explain_handles_missing_optional_data(self):
        before = sample(18, 60, 0)
        after = sample(19, 60, 0)
        before.home_usage = []
        after.home_usage = []
        before.podman = PodmanUsage(error="not installed")
        after.podman = PodmanUsage(error="not installed")

        output = render_explanation(before, after)

        self.assertIn("used space changed by +0B", output)
        self.assertIn("No significant directory growth.", output)
        self.assertIn("Podman comparison unavailable.", output)
        self.assertIn("No unusual growth was detected.", output)
        self.assertEqual(output.count("Recommendation:"), 1)


if __name__ == "__main__":
    unittest.main()
