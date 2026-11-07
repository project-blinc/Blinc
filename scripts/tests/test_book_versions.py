"""Release identity, assets and stable/dev routing for versioned book publication."""

import importlib.util
import io
import json
from pathlib import Path
import tarfile
import tempfile
import unittest

SPEC = importlib.util.spec_from_file_location("book_versions", Path(__file__).resolve().parents[1] / "book_versions.py")
books = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(books)


class BookVersionsTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name)
        self.archives = self.root / "archives"
        self.archives.mkdir()

    def book(self, name, content):
        directory = self.root / name
        directory.mkdir()
        (directory / "index.html").write_text(content)
        (directory / "chapter.html").write_text(content + " chapter")
        (directory / books.META).write_text(json.dumps({
            "schema": 1, "id": name, "commit": "a" * 40,
            "source_ref": "main" if name == "dev" else "v" + name,
            "pages": ["index.html", "chapter.html"],
        }))
        return directory

    def release(self, name):
        book = self.book(name, "Released " + name)
        examples = self.root / (name + "-examples")
        examples.mkdir()
        (examples / "demo.wasm").write_bytes(b"versioned wasm " + name.encode())
        archive = self.archives / f"blinc-book-{name}.tar.gz"
        books.freeze(book, examples, archive)
        return book, examples, archive

    def test_stable_uses_the_newest_release_and_preserves_older_assets(self):
        self.release("0.6.0")
        self.release("0.6.1")
        dev = self.book("dev", "Development facade APIs")
        output = self.root / "site/docs"
        manifest = books.assemble(dev, self.archives, output)
        self.assertEqual(manifest["stable"], "0.6.1")
        self.assertEqual((output / "index.html").read_text(), "Released 0.6.1")
        self.assertEqual((output / "dev/index.html").read_text(), "Development facade APIs")
        self.assertEqual((output / "0.6.0/examples/demo.wasm").read_bytes(), b"versioned wasm 0.6.0")
        self.assertEqual((output / "examples/demo.wasm").read_bytes(), b"versioned wasm 0.6.1")
        self.assertEqual([item["id"] for item in manifest["editions"]], ["0.6.1", "0.6.0", "dev"])

    def test_missing_release_keeps_existing_stable_site(self):
        dev = self.book("dev", "Development")
        output = self.root / "site/docs"
        output.mkdir(parents=True)
        (output / "index.html").write_text("Existing stable")
        with self.assertRaisesRegex(ValueError, "No released book"):
            books.assemble(dev, self.archives, output)
        self.assertEqual((output / "index.html").read_text(), "Existing stable")

    def test_corrupt_snapshot_is_rejected(self):
        book, _, _ = self.release("0.6.0")
        (book / "chapter.html").write_text("Wrong version")
        with self.assertRaisesRegex(ValueError, "checksums"):
            books.verify(book)

    def test_release_requires_assets_and_cannot_be_overwritten(self):
        book, examples, archive = self.release("0.6.0")
        with self.assertRaisesRegex(ValueError, "overwrite"):
            books.freeze(book, examples, archive)
        with self.assertRaisesRegex(ValueError, "built example assets"):
            books.freeze(book, self.root / "missing", self.root / "missing.tar.gz")

    def test_development_cannot_be_frozen_as_a_release(self):
        with self.assertRaisesRegex(ValueError, "Development"):
            books.freeze(self.book("dev", "Unreleased"), self.root, self.root / "dev.tar.gz")

    def test_embedded_demos_must_exist_in_the_release(self):
        book = self.book("0.6.0", '<iframe src="examples/missing/index.html"></iframe>')
        examples = self.root / "examples"
        examples.mkdir()
        (examples / "other.wasm").write_bytes(b"example")
        with self.assertRaisesRegex(ValueError, "Missing release demo"):
            books.freeze(book, examples, self.root / "release.tar.gz")

    def test_inline_demos_must_exist_in_the_release(self):
        book = self.book("0.6.0", '<div class="blinc-example" data-example="missing"></div>')
        examples = self.root / "examples"
        (examples / "present").mkdir(parents=True)
        (examples / "present" / "index.html").write_text("demo")
        with self.assertRaisesRegex(ValueError, "Missing release demo"):
            books.freeze(book, examples, self.root / "release.tar.gz")
        book = self.book("0.6.1", '<div class="blinc-example" data-example="present"></div>')
        books.freeze(book, examples, self.root / "ok.tar.gz")

    def test_print_demos_use_the_snapshot_root_and_remain_validated(self):
        book = self.book("0.6.0", "Released")
        chapter = book / "chapter.html"
        chapter.write_text('<iframe src="examples/canvas/index.html?theme=dark#demo"></iframe>')
        page = book / "print.html"
        page.write_text('''<iframe\n src="../../examples/canvas/index.html?theme=dark#demo"></iframe>
<iframe src='../examples/canvas/index.html'></iframe>
<iframe src="https://example.com/demo"></iframe>
<code>&lt;iframe src="../../examples/canvas/index.html"&gt;</code>''')
        metadata = json.loads((book / books.META).read_text())
        metadata["pages"].append("print.html")
        (book / books.META).write_text(json.dumps(metadata))
        examples = self.root / "examples"
        demo = examples / "canvas/index.html"
        demo.parent.mkdir(parents=True)
        demo.write_text("Canvas demo")

        books.rebase_print_demos(book)
        self.assertIn('src="examples/canvas/index.html?theme=dark#demo"', page.read_text())
        self.assertIn("src='examples/canvas/index.html'", page.read_text())
        self.assertIn('src="https://example.com/demo"', page.read_text())
        self.assertIn('&lt;iframe src="../../examples/canvas/index.html"', page.read_text())
        self.assertEqual(chapter.read_text(), '<iframe src="examples/canvas/index.html?theme=dark#demo"></iframe>')
        books.freeze(book, examples, self.root / "release.tar.gz")
        self.assertIn("print.html", books.verify(book)["sha256"])

        page.write_text('<iframe src="../../examples/missing/index.html"></iframe>')
        books.rebase_print_demos(book)
        with self.assertRaisesRegex(ValueError, "Missing release demo: print.html"):
            books.freeze(book, examples, self.root / "missing.tar.gz")

    def test_failed_archive_creation_leaves_no_partial_release(self):
        book = self.book("0.6.0", "Released")
        examples = self.root / "examples"
        examples.mkdir()
        (examples / "demo.wasm").write_bytes(b"example")
        (book / "linked.html").symlink_to(book / "chapter.html")
        archive = self.root / "release.tar.gz"
        with self.assertRaisesRegex(ValueError, "symlink"):
            books.freeze(book, examples, archive)
        self.assertFalse(archive.exists())

    def test_archive_paths_cannot_escape_the_snapshot(self):
        archive = self.root / "unsafe.tar.gz"
        with tarfile.open(archive, "w:gz") as target:
            info = tarfile.TarInfo("../outside")
            info.size = 4
            target.addfile(info, io.BytesIO(b"oops"))
        with self.assertRaisesRegex(ValueError, "Invalid snapshot path"):
            books.unpack(archive, self.root / "unpacked")
        self.assertFalse((self.root / "outside").exists())

    def test_config_preserves_existing_assets_and_search_options(self):
        source = '[book]\ntitle = "Blinc"\n[output.html]\nadditional-js = ["custom.js"]\n[output.html.search]\nenable = true\n'
        updated = books.set_config(source, "output.html", "additional-js", ["custom.js", "versions.js"])
        updated = books.set_config(updated, "output.html", "site-url", "/docs/dev/")
        config = books.tomllib.loads(updated)
        self.assertEqual(config["output"]["html"]["additional-js"], ["custom.js", "versions.js"])
        self.assertTrue(config["output"]["html"]["search"]["enable"])
        self.assertEqual(config["book"]["title"], "Blinc")


if __name__ == "__main__":
    unittest.main()
