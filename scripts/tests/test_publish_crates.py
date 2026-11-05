"""Release selection and upload failure controls; no network or registry mutations."""

import importlib.util
import json
from pathlib import Path
import subprocess
import unittest
from unittest.mock import patch

SPEC = importlib.util.spec_from_file_location(
    "publish_crates", Path(__file__).resolve().parents[1] / "publish_crates.py"
)
publisher = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(publisher)


def package(name, deps=(), publish=None):
    return {
        "id": name, "name": name, "version": "0.6.0", "publish": publish,
        "dependencies": [
            {"name": dep, "kind": None, "path": f"../{dep}", "req": "^0.6.0"}
            for dep in deps
        ],
    }


def metadata(*packages):
    return {"packages": packages, "workspace_members": [p["id"] for p in packages]}


class ReleaseTests(unittest.TestCase):
    def test_facade_follows_all_its_dependencies_in_the_release(self):
        source = metadata(package("blinc", ["blinc_app", "blinc_cn", "blinc_svg", "blinc_icons"]),
                          package("blinc_app"), package("blinc_cn"), package("blinc_svg"), package("blinc_icons"))
        selected = [p["name"] for p in source["packages"]]
        plan = publisher.release_plan(source, selected)
        self.assertEqual(plan[-1]["name"], "blinc")
        self.assertIn("blinc", publisher.RELEASE_CRATES)

    def test_css_precedes_layout_even_if_selection_is_reversed(self):
        source = metadata(package("layout", ["css"]), package("css"))
        plan = publisher.release_plan(source, ["layout", "css"])
        self.assertEqual([p["name"] for p in plan], ["css", "layout"])

    def test_private_excluded_missing_and_cyclic_packages_are_rejected(self):
        cases = [
            (metadata(package("private", publish=[])), ["private"]),
            (metadata(package("local", ["excluded"])), ["local"]),
            (metadata(package("a", ["b"]), package("b", ["a"])), ["a", "b"]),
            (metadata(package("local")), ["absent"]),
            (metadata(package("local", ["excluded"])), ["local", "excluded"]),
        ]
        for source, selected in cases:
            with self.subTest(selected=selected), self.assertRaises(ValueError):
                publisher.release_plan(source, selected)

    def test_dev_dependency_does_not_create_a_runtime_cycle(self):
        core = package("core", ["animation"])
        core["dependencies"][0]["kind"] = "dev"
        source = metadata(core, package("animation", ["core"]))
        plan = publisher.release_plan(source, ["animation", "core"])
        self.assertEqual([p["name"] for p in plan], ["core", "animation"])

    def test_old_internal_version_is_rejected_after_release_bump(self):
        app = package("app", ["core"])
        app["dependencies"][0]["req"] = "^0.5.1"
        with self.assertRaisesRegex(ValueError, "release 0.6.0"):
            publisher.release_plan(metadata(app, package("core")), ["app", "core"])

    def test_failed_upload_stops_even_if_output_says_uploaded(self):
        failed = subprocess.CompletedProcess([], 17, stdout="uploaded")
        with patch.object(publisher.subprocess, "run", return_value=failed) as run:
            with self.assertRaisesRegex(RuntimeError, "exit 17"):
                publisher.execute_plan([package("first"), package("second")], False, 0)
            self.assertEqual(run.call_count, 1)
            self.assertNotIn("--no-verify", run.call_args.args[0])

    def test_dry_run_verifies_without_upload_or_wait(self):
        success = subprocess.CompletedProcess([], 0)
        with (
            patch.object(publisher.subprocess, "run", return_value=success) as run,
            patch.object(publisher.time, "sleep") as sleep,
        ):
            publisher.execute_plan([package("first"), package("second")], True, 20)
            self.assertEqual(run.call_count, 2)
            self.assertTrue(all("--dry-run" in call.args[0] for call in run.call_args_list))
            sleep.assert_not_called()

    def test_default_mode_only_reads_metadata(self):
        source = metadata(*(package(name) for name in publisher.RELEASE_CRATES))
        success = subprocess.CompletedProcess([], 0, stdout=json.dumps(source))
        with patch.object(publisher.subprocess, "run", return_value=success) as run:
            self.assertEqual(publisher.main([]), 0)
            self.assertEqual(run.call_count, 1)
            self.assertEqual(run.call_args.args[0][1], "metadata")

    def test_invalid_release_selection_prevents_all_uploads(self):
        source = metadata(*(package(name) for name in publisher.RELEASE_CRATES))
        source["packages"][0]["publish"] = []
        success = subprocess.CompletedProcess([], 0, stdout=json.dumps(source))
        with patch.object(publisher.subprocess, "run", return_value=success) as run:
            self.assertEqual(publisher.main(["--publish", "--wait", "0"]), 1)
            self.assertEqual(run.call_count, 1)
            self.assertEqual(run.call_args.args[0][1], "metadata")


if __name__ == "__main__":
    unittest.main()
