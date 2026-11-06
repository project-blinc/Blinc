"""Demo WASM moves to a content-addressed origin and the pages follow it."""

import hashlib
import http.server
import importlib.util
from pathlib import Path
import tempfile
import threading
import unittest

SPEC = importlib.util.spec_from_file_location("book_cdn", Path(__file__).resolve().parents[1] / "book_cdn.py")
cdn = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(cdn)

PAGE = ('<script type="module">const wasmUrl = "pkg/a_bg.wasm";\n'
        'const { default: init } = await import("./pkg/a_bg.js");\nawait init();\n</script>')
BASE = "https://assets.example.org"


class ExternalizeTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name)
        self.docs = self.root / "docs"
        self.objects = self.root / "objects"

    def demo(self, edition, name, wasm):
        directory = self.docs / edition / "examples" / name
        (directory / "pkg").mkdir(parents=True)
        (directory / "index.html").write_text(PAGE)
        (directory / "pkg" / f"{name}_bg.wasm").write_bytes(wasm)
        (directory / "pkg" / f"{name}.js").write_text("export default function init() {}")
        return directory

    def test_pages_point_at_the_hashed_object_and_the_module_leaves_the_site(self):
        directory = self.demo("", "a", b"module a")
        manifest = cdn.externalize(self.docs, BASE, self.objects)
        digest = hashlib.sha256(b"module a").hexdigest()
        self.assertEqual(manifest, {f"wasm/{digest}.wasm": {"sha256": digest, "bytes": 8}})
        self.assertEqual((self.objects / f"wasm/{digest}.wasm").read_bytes(), b"module a")
        self.assertIn(f'init({{ module_or_path: "{BASE}/wasm/{digest}.wasm" }})', (directory / "index.html").read_text())
        self.assertIn(f'const wasmUrl = "{BASE}/wasm/{digest}.wasm";', (directory / "index.html").read_text())
        self.assertFalse((directory / "pkg" / "a_bg.wasm").exists())
        self.assertTrue((directory / "pkg" / "a.js").exists())

    def test_a_page_without_the_worker_literal_still_gets_the_init_rewrite(self):
        directory = self.demo("", "a", b"module a")
        (directory / "index.html").write_text('<script type="module">await init();</script>')
        cdn.externalize(self.docs, BASE, self.objects)
        self.assertIn("module_or_path", (directory / "index.html").read_text())

    def test_identical_modules_in_two_editions_share_one_object(self):
        self.demo("", "a", b"same")
        self.demo("0.6.0", "a", b"same")
        self.demo("dev", "a", b"newer")
        manifest = cdn.externalize(self.docs, BASE, self.objects)
        self.assertEqual(len(manifest), 2)
        self.assertEqual(len(list((self.objects / "wasm").iterdir())), 2)

    def test_large_assets_move_and_the_page_remaps_their_requests(self):
        directory = self.demo("", "a", b"module")
        assets = directory / cdn.ASSETS / "3d"
        assets.mkdir(parents=True)
        (assets / "scene.bin").write_bytes(b"s" * cdn.LARGE)
        (assets / "small.txt").write_text("stays")
        (assets / "clip.mp4").write_bytes(b"v" * cdn.LARGE)
        page = directory / "index.html"
        page.write_text(f'<link rel="preload" as="fetch" crossorigin href="{cdn.ASSETS}/3d/scene.bin" />\n' + PAGE)
        manifest = cdn.externalize(self.docs, BASE, self.objects)
        digest = hashlib.sha256(b"s" * cdn.LARGE).hexdigest()
        url = f"{BASE}/asset/{digest}.bin"
        text = page.read_text()
        self.assertIn(f'href="{url}"', text)
        self.assertIn(f'"{cdn.ASSETS}/3d/scene.bin": "{url}"', text)
        self.assertLess(text.index("window.fetch"), text.index("await init("))
        self.assertIn("window.blincAssetRemap=", text)
        self.assertIn(f"asset/{digest}.bin", manifest)
        self.assertFalse((assets / "scene.bin").exists())
        self.assertTrue((assets / "small.txt").exists())
        self.assertTrue((assets / "clip.mp4").exists())

    def test_a_page_without_large_assets_gets_no_remap(self):
        directory = self.demo("", "a", b"module")
        cdn.externalize(self.docs, BASE, self.objects)
        self.assertNotIn("window.fetch", (directory / "index.html").read_text())

    def test_a_page_that_cannot_be_pointed_at_the_origin_fails(self):
        directory = self.demo("", "a", b"x")
        (directory / "index.html").write_text("<script>init(1)</script>")
        with self.assertRaisesRegex(ValueError, "exactly one"):
            cdn.externalize(self.docs, BASE, self.objects)

    def test_a_demo_without_exactly_one_module_fails(self):
        directory = self.demo("", "a", b"x")
        (directory / "pkg" / "b_bg.wasm").write_bytes(b"y")
        with self.assertRaisesRegex(ValueError, "expected one WASM module"):
            cdn.externalize(self.docs, BASE, self.objects)

    def test_the_base_must_be_an_https_origin(self):
        for value in ("http://assets.example.org", "assets.example.org", 'https://a"b', "https://a/../b"):
            with self.assertRaises(ValueError):
                cdn.base_url(value)
        self.assertEqual(cdn.base_url("https://assets.example.org/blinc/"), "https://assets.example.org/blinc")


class VerifyTests(unittest.TestCase):
    def serve(self, headers):
        class Handler(http.server.BaseHTTPRequestHandler):
            def do_HEAD(self):
                self.send_response(200)
                for name, value in headers.items():
                    self.send_header(name, value)
                self.end_headers()

            def log_message(self, *args):
                pass

        server = http.server.HTTPServer(("127.0.0.1", 0), Handler)
        threading.Thread(target=server.serve_forever, daemon=True).start()
        self.addCleanup(server.server_close)
        self.addCleanup(server.shutdown)
        return f"http://127.0.0.1:{server.server_port}"

    def check(self, headers):
        url = self.serve(headers)
        # The https rule guards what is published; a loopback server stands in for the origin here.
        original = cdn.base_url
        cdn.base_url = lambda value: value
        self.addCleanup(setattr, cdn, "base_url", original)
        return cdn.verify(url, {"wasm/x.wasm": {"sha256": "x", "bytes": 4}}, "https://blinc.rs")

    def test_an_object_served_correctly_has_no_problems(self):
        self.assertEqual(self.check({
            "Content-Type": "application/wasm", "Content-Length": "4",
            "Cache-Control": cdn.IMMUTABLE, "Access-Control-Allow-Origin": "*",
        }), [])

    def test_each_missing_property_is_named(self):
        problems = self.check({"Content-Type": "text/plain", "Content-Length": "9"})
        self.assertEqual(len(problems), 4)


if __name__ == "__main__":
    unittest.main()
