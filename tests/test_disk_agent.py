import tempfile
import unittest
from datetime import datetime, timezone
from pathlib import Path
from unittest.mock import patch

from disk_agent.classify import classify_path, load_rules
from disk_agent.diff import SIGNIFICANT_BYTES, compare_snapshots, latest_two, render_diff
from disk_agent.explain import render_explanation
from disk_agent.investigate import assess, render_investigation
from disk_agent.models import DirectoryUsage, FilesystemUsage, PodmanUsage, Snapshot, load_snapshot
from disk_agent.report import render_report
from disk_agent.snapshot import _collect_podman, save_snapshot


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

    def test_rules_classify_known_growth(self):
        classification = classify_path("~/.codex/packages/0.142.3", load_rules())

        self.assertEqual(classification.classification, "Application releases")
        self.assertEqual(classification.risk, "Low")
        self.assertTrue(classification.known)

    def test_rules_report_unknown_growth(self):
        classification = classify_path("~/mystery-growth", load_rules())

        self.assertEqual(classification.classification, "Unknown growth")
        self.assertEqual(classification.risk, "Unknown")
        self.assertFalse(classification.known)

    def test_explain_is_concise_with_exactly_one_recommendation(self):
        before = sample(18, 60, 0)
        after = sample(19, 61, 100 * 1024 * 1024)

        output = render_explanation(before, after)

        self.assertIn("Disk usage increased from 60% to 61%.", output)
        self.assertIn("+100M ~/.cache", output)
        self.assertIn("Cause:", output)
        self.assertIn("Risk:", output)
        self.assertIn("Action:", output)
        self.assertIn("Growth appears normal", output)
        self.assertEqual(output.count("Recommendation:"), 1)

    def test_explain_marks_large_growth_unusual(self):
        before = sample(18, 60, 0)
        after = sample(19, 66, 6 * 1024**3)

        output = render_explanation(before, after)

        self.assertIn("Growth appears unusual", output)
        self.assertIn("Review the largest contributor", output)
        self.assertEqual(output.count("Recommendation:"), 1)

    def test_investigation_reads_like_operational_report(self):
        before = sample(18, 60, 0)
        before.home_usage.append(DirectoryUsage("~/.codex", 100 * 1024 * 1024))
        after = sample(19, 62, 0)
        after.home_usage.extend(
            [
                DirectoryUsage("~/.codex", 950 * 1024 * 1024),
                DirectoryUsage("~/.codex/packages", 838 * 1024 * 1024),
            ]
        )
        after.largest_directories.extend(
            [
                DirectoryUsage("~/.codex/packages", 838 * 1024 * 1024),
                DirectoryUsage("~/.codex/packages/0.142.0", 250 * 1024 * 1024),
                DirectoryUsage("~/.codex/packages/0.142.2", 280 * 1024 * 1024),
                DirectoryUsage("~/.codex/packages/0.142.3", 308 * 1024 * 1024),
            ]
        )

        output = render_investigation(before, after)

        self.assertIn("Filesystem usage: 62%", output)
        self.assertIn("+838M ~/.codex/packages", output)
        self.assertIn("Application releases", output)
        self.assertIn("3 retained Codex releases", output)
        self.assertIn("Risk: Low", output)
        self.assertIn("Assessment", output)
        self.assertIn("Healthy", output)
        self.assertIn("Review retained Codex releases.", output)

    def test_assessment_escalates_unknown_large_growth(self):
        before = sample(18, 60, 0)
        after = sample(19, 61, 0)
        after.filesystem.used_bytes = before.filesystem.used_bytes + 2 * 1024**3
        growth = [type("Change", (), {"path": "~/unknown", "bytes": 2 * 1024**3})()]
        classifications = {"~/unknown": classify_path("~/unknown", load_rules())}

        self.assertEqual(assess(before, after, growth, classifications, set()), "Attention Recommended")

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

    def test_podman_uses_rootless_storage_when_binary_is_absent(self):
        with tempfile.TemporaryDirectory() as temporary:
            home = Path(temporary)
            storage = home / ".local" / "share" / "containers" / "storage"
            (storage / "overlay-images").mkdir(parents=True)
            (storage / "overlay-containers").mkdir()
            (storage / "volumes").mkdir()
            storage_calls = {
                str(storage / "overlay-images"): ("100\timages\n", "", 0),
                str(storage / "overlay-containers"): ("200\tcontainers\n", "", 0),
                str(storage / "volumes"): ("300\tvolumes\n", "", 0),
            }

            def fake_run(command):
                return storage_calls[str(command[-1])]

            with patch("disk_agent.snapshot.shutil.which", return_value=None):
                with patch("disk_agent.snapshot.Path.home", return_value=home):
                    with patch("disk_agent.snapshot._run", side_effect=fake_run):
                        usage = _collect_podman()

            self.assertTrue(usage.available)
            self.assertEqual(usage.images_bytes, 100)
            self.assertEqual(usage.containers_bytes, 200)
            self.assertEqual(usage.volumes_bytes, 300)

    def test_podman_reports_unavailable_when_binary_and_storage_are_absent(self):
        with tempfile.TemporaryDirectory() as temporary:
            with patch("disk_agent.snapshot.shutil.which", return_value=None):
                with patch("disk_agent.snapshot.Path.home", return_value=Path(temporary)):
                    usage = _collect_podman()

        self.assertFalse(usage.available)
        self.assertEqual(usage.error, "podman is not installed")


if __name__ == "__main__":
    unittest.main()
