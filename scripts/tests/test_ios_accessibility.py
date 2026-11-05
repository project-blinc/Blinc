"""Resource limits must prevent a build or simulator command from starting."""

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


class MemoryGuardTests(unittest.TestCase):
    def assert_preflight_stops(self, sample):
        baseline = {"pressure": 2, "swap_used_mb": 1000.0, "free_percent": 30}
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory)
            with patch.object(runner, "OUT", path), \
                 patch.object(runner, "prepare", return_value=path / "Test.xcodeproj"), \
                 patch.object(runner, "memory", side_effect=[baseline, sample, sample]), \
                 patch.object(runner.subprocess, "Popen") as spawn, \
                 patch("sys.argv", [str(SCRIPT)]), \
                 contextlib.redirect_stderr(io.StringIO()):
                self.assertEqual(runner.main(), 75)
                spawn.assert_not_called()
            report = json.loads((path / "memory.json").read_text())
            self.assertEqual(report["outcome"], "resource-stopped")
            self.assertFalse(report["simulator_started"])
            self.assertFalse(report["tests_started"])

    def test_critical_pressure_does_not_spawn_a_command(self):
        self.assert_preflight_stops({"pressure": 4, "swap_used_mb": 1000.0, "free_percent": 30})

    def test_swap_growth_does_not_spawn_a_command(self):
        self.assert_preflight_stops({"pressure": 2, "swap_used_mb": 1300.0, "free_percent": 30})


if __name__ == "__main__":
    unittest.main()
