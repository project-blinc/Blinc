"""Memory telemetry must not prevent the requested simulator checks."""

import contextlib
import importlib.util
import io
import json
import os
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

if os.name != "posix":
    raise unittest.SkipTest("Simulator runner uses POSIX process groups and file locks.")

SCRIPT = Path(__file__).resolve().parents[1] / "test_ios_accessibility.py"
spec = importlib.util.spec_from_file_location("ios_accessibility_runner", SCRIPT)
runner = importlib.util.module_from_spec(spec)
spec.loader.exec_module(runner)


class MemoryTelemetryTests(unittest.TestCase):
    def assert_telemetry_allows_builds(self, sample):
        baseline = {"pressure": 2, "swap_used_mb": 1000.0, "free_percent": 30}
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory)
            with patch.object(runner, "OUT", path), \
                 patch.object(runner, "prepare", return_value=path / "Test.xcodeproj"), \
                 patch.object(runner, "memory", side_effect=[baseline] + [sample] * 10), \
                 patch.object(runner, "output", return_value='{"devices": {}}'), \
                 patch.object(runner.subprocess, "Popen") as spawn, \
                 patch("sys.argv", [str(SCRIPT)]), \
                 contextlib.redirect_stderr(io.StringIO()), \
                 contextlib.redirect_stdout(io.StringIO()):
                spawn.return_value.poll.return_value = 0
                spawn.return_value.returncode = 0
                self.assertEqual(runner.main(), 1)  # No installed runtime in this fixture.
                self.assertEqual(spawn.call_count, 2)  # Rust and Xcode builds both ran.
            report = json.loads((path / "memory.json").read_text())
            self.assertEqual(report["outcome"], "setup-failed")
            self.assertIn("iOS17.5 runtime", report["reason"])
            self.assertEqual(report["peak_swap_mb"], sample["swap_used_mb"])

    def test_pressure_is_recorded_without_stopping_builds(self):
        self.assert_telemetry_allows_builds({"pressure": 4, "swap_used_mb": 1000.0, "free_percent": 1})

    def test_swap_growth_is_recorded_without_stopping_builds(self):
        self.assert_telemetry_allows_builds({"pressure": 2, "swap_used_mb": 1600.0, "free_percent": 30})


if __name__ == "__main__":
    unittest.main()
