"""Check release archive reuse and upload boundaries without GitHub or Cargo writes."""

import importlib.util
import os
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

ROOT = Path(__file__).resolve().parents[2]
SPEC = importlib.util.spec_from_file_location("prepare_books", ROOT / ".github/scripts/prepare-book-releases.py")
prepare = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(prepare)


class RestoreTests(unittest.TestCase):
    def test_cached_release_must_match_its_tag_commit(self):
        with tempfile.TemporaryDirectory() as temporary:
            archives = Path(temporary)
            release = [{"tagName": "v0.6.0"}]
            assets = {"assets": [{"name": "blinc-book-0.6.0.tar.gz"}]}
            metadata = {"id": "0.6.0", "commit": "a" * 40}
            with (patch.object(prepare, "gh_json", side_effect=[release, assets]),
                  patch.object(prepare, "run") as run,
                  patch.object(prepare.books, "unpack", return_value=metadata),
                  patch.object(prepare.subprocess, "check_output", return_value="a" * 40)):
                self.assertEqual(prepare.restore("project-blinc/Blinc", archives), ("v0.6.0", "0.6.0", True))
                self.assertIn("--skip-existing", run.call_args.args[0])
            with (patch.object(prepare, "gh_json", side_effect=[release, assets]),
                  patch.object(prepare, "run"),
                  patch.object(prepare.books, "unpack", return_value=metadata),
                  patch.object(prepare.subprocess, "check_output", return_value="b" * 40)):
                with self.assertRaisesRegex(RuntimeError, "does not match"):
                    prepare.restore("project-blinc/Blinc", archives)

    def test_preview_does_not_upload_and_publisher_never_clobbers(self):
        for upload in ["false", "true"]:
            with self.subTest(upload=upload), tempfile.TemporaryDirectory() as temporary:
                root = Path(temporary)
                archive = root / "snapshot.tar.gz"
                with (patch.object(prepare, "ROOT", root),
                      patch.object(prepare, "restore", return_value=("v0.6.0", "0.6.0", False)),
                      patch.object(prepare, "snapshot", return_value=archive) as snapshot,
                      patch.object(prepare, "validate_archive"),
                      patch.object(prepare, "run") as run,
                      patch.dict(os.environ, {"GITHUB_REPOSITORY": "project-blinc/Blinc", "BLINC_BOOK_UPLOAD": upload})):
                    prepare.main()
                    snapshot.assert_called_once()
                    if upload == "false":
                        run.assert_not_called()
                    else:
                        self.assertEqual(run.call_args.args[0][:3], ["gh", "release", "upload"])
                        self.assertNotIn("--clobber", run.call_args.args[0])

    def test_existing_archive_is_not_rebuilt_or_uploaded(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            archives = root / "target/book-releases"
            archives.mkdir(parents=True)
            (archives / "blinc-book-0.6.0.tar.gz").write_bytes(b"restored")
            with (patch.object(prepare, "ROOT", root),
                  patch.object(prepare, "restore", return_value=("v0.6.0", "0.6.0", True)),
                  patch.object(prepare, "snapshot") as snapshot,
                  patch.object(prepare, "run") as run,
                  patch.dict(os.environ, {"GITHUB_REPOSITORY": "project-blinc/Blinc", "BLINC_BOOK_UPLOAD": "true"})):
                prepare.main()
                snapshot.assert_not_called()
                run.assert_not_called()

    def test_local_snapshot_is_uploaded_after_an_interrupted_publisher(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            archives = root / "target/book-releases"
            archives.mkdir(parents=True)
            archive = archives / "blinc-book-0.6.0.tar.gz"
            archive.write_bytes(b"previous local build")
            with (patch.object(prepare, "ROOT", root),
                  patch.object(prepare, "restore", return_value=("v0.6.0", "0.6.0", False)),
                  patch.object(prepare, "snapshot") as snapshot,
                  patch.object(prepare, "validate_archive") as validate,
                  patch.object(prepare, "run") as run,
                  patch.dict(os.environ, {"GITHUB_REPOSITORY": "project-blinc/Blinc", "BLINC_BOOK_UPLOAD": "true"})):
                prepare.main()
                snapshot.assert_not_called()
                validate.assert_called_once_with(archive, "v0.6.0", "0.6.0")
                self.assertEqual(run.call_args.args[0][:3], ["gh", "release", "upload"])

    def test_snapshot_removes_the_worktree_when_the_book_build_fails(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            with (patch.object(prepare, "ROOT", root),
                  patch.object(prepare, "run") as run,
                  patch.object(prepare.subprocess, "check_output", return_value="a" * 40),
                  patch.object(prepare.books, "build", side_effect=RuntimeError("Build failed"))):
                with self.assertRaisesRegex(RuntimeError, "Build failed"):
                    prepare.snapshot("v0.6.0", "0.6.0", root / "archives")
                self.assertEqual(run.call_args.args[0][:4], ["git", "worktree", "remove", "--force"])


if __name__ == "__main__":
    unittest.main()
