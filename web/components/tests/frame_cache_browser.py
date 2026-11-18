"""Exercise WebApp retained frames in Chrome using a browser-tests WASM bundle."""

import argparse
import io
import json
import statistics
import threading
from http.server import SimpleHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path
from urllib.parse import unquote, urlparse

from PIL import Image, ImageChops
from playwright.sync_api import sync_playwright

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument("--runtime", type=Path, required=True)
parser.add_argument("--out", type=Path, required=True)
parser.add_argument("--mode", choices=["baseline", "candidate"], required=True)
parser.add_argument("--dpr", type=int, choices=[1, 2], default=1)
parser.add_argument("--backend", choices=["webgpu", "webgl2"], default="webgpu")
args = parser.parse_args()
runtime = args.runtime.resolve()
args.out.mkdir(parents=True, exist_ok=True)

INDEX = """<!doctype html><html><body style="margin:0;background:#101318">
<canvas id="probe" style="width:640px;height:480px"></canvas>
<script>
const worker = new Worker('/worker.js', {type:'module'});
const pending = new Map(); let seq = 0;
worker.onmessage = ({data}) => {
  const [resolve,reject] = pending.get(data.seq); pending.delete(data.seq);
  data.error ? reject(new Error(data.error)) : resolve(data.value);
};
window.command = (cmd, args=[], transfer=[]) => new Promise((resolve,reject) => {
  pending.set(++seq,[resolve,reject]); worker.postMessage({seq,cmd,args},transfer);
});
const canvas = document.querySelector('canvas').transferControlToOffscreen();
window.ready = command('init', [canvas, DPR], [canvas]);
</script></body></html>"""

WORKER = """import init, {create_cache_probe} from '/pkg/blinc_component_demos.js';
let probe; let wasm;
self.onmessage = async ({data}) => {
  const {seq,cmd,args} = data;
  try {
    let value;
    if (cmd === 'init') {
      wasm = await init(); probe = await create_cache_probe(args[0],args[1],256);
      for (let i=0;i<60;i++) {
        await new Promise(requestAnimationFrame); probe.frame(false);
      }
      value = JSON.parse(probe.frame(false));
    } else if (cmd === 'settle') {
      for (let i=0;i<30;i++) {
        await new Promise(requestAnimationFrame); probe.frame(false);
      }
    } else if (cmd === 'bench') {
      const before = JSON.parse(probe.frame(false)), costs=[];
      for (let i=0;i<120;i++) {
        await new Promise(requestAnimationFrame);
        if (args[0]) probe.change('translate', i % 50);
        const start = performance.now(); probe.frame(false);
        costs.push(performance.now()-start);
      }
      value = {before,after:JSON.parse(probe.frame(false)),costs,
        linear_memory_bytes:wasm.memory.buffer.byteLength};
    } else {
      value = probe[cmd](...args);
      if (cmd === 'frame') value = JSON.parse(value);
    }
    self.postMessage({seq,value});
  } catch (error) {self.postMessage({seq,error:String(error.stack || error)});}
};"""


class Handler(SimpleHTTPRequestHandler):
    def log_message(self, *_):
        pass

    def end_headers(self):
        self.send_header("Cross-Origin-Opener-Policy", "same-origin")
        self.send_header("Cross-Origin-Embedder-Policy", "require-corp")
        super().end_headers()

    def do_GET(self):
        path = urlparse(self.path).path
        body = {
            "/": INDEX.replace("DPR", str(args.dpr)).encode(),
            "/worker.js": WORKER.replace(
                "let probe;",
                ("Object.defineProperty(navigator, 'gpu', {value: undefined}); "
                 if args.backend == "webgl2" else "") + "let probe;",
            ).encode(),
        }.get(path)
        if body is not None:
            self.send_response(200)
            self.send_header("Content-Type", "text/javascript" if path.endswith(".js") else "text/html")
            self.send_header("Content-Length", str(len(body)))
            self.end_headers()
            self.wfile.write(body)
            return
        name = Path(unquote(path)).name
        file = runtime / name
        if not path.startswith("/pkg/") or not file.is_file():
            self.send_error(404)
            return
        body = file.read_bytes()
        self.send_response(200)
        self.send_header("Content-Type", "application/wasm" if name.endswith(".wasm") else "text/javascript")
        self.send_header("Content-Length", str(len(body)))
        self.end_headers()
        self.wfile.write(body)


server = ThreadingHTTPServer(("127.0.0.1", 0), Handler)
thread = threading.Thread(target=server.serve_forever, daemon=True)
thread.start()
report = {"mode": args.mode, "dpr": args.dpr, "backend": args.backend, "benchmarks": {}, "pixel_checks": {}}
try:
    with sync_playwright() as playwright:
        browser = playwright.chromium.launch(
            channel="chrome", headless=True, args=["--enable-unsafe-webgpu"]
        )
        context = browser.new_context(
            viewport={"width": 700, "height": 540}, device_scale_factor=args.dpr
        )
        page = context.new_page()
        errors = []
        page.on("pageerror", lambda error: errors.append(str(error)))
        page.goto(f"http://127.0.0.1:{server.server_port}/", wait_until="domcontentloaded")
        initial = page.evaluate("window.ready")
        assert initial["gpu"] == (args.backend == "webgpu"), initial
        report["initial"] = initial

        def command(name, *values):
            return page.evaluate("([name,args])=>window.command(name,args)", [name, list(values)])

        def shot(name):
            page.wait_for_timeout(35)
            return Image.open(io.BytesIO(page.locator("canvas").screenshot(
                path=str(args.out / f"{name}.png")
            ))).convert("RGB")

        def difference(a, b):
            return sum(max(pixel) > 3 for pixel in ImageChops.difference(a, b).get_flattened_data())

        def same_as_full(name, tolerance=0):
            state = command("frame", False)
            cached = shot(name + "-retained")
            command("frame", True)
            fresh = shot(name + "-full")
            changed = difference(cached, fresh)
            report["pixel_checks"][name] = changed
            assert changed <= tolerance, (name, changed, state)
            return cached

        for name, moving in [("idle", False), ("translation", True)]:
            result = command("bench", moving)
            paints = result["after"]["paints"] - result["before"]["paints"]
            costs = sorted(result["costs"])
            report["benchmarks"][name] = {
                "tree_paints": paints,
                "frames": len(costs) + 1,
                "root_builds": result["after"]["builds"] - result["before"]["builds"],
                "wasm_linear_memory_bytes": result["linear_memory_bytes"],
                "median_cpu_frame_ms": statistics.median(costs),
                "p95_cpu_frame_ms": costs[int(len(costs) * 0.95) - 1],
                "raw_cpu_frame_ms": result["costs"],
            }
            if args.mode == "candidate":
                assert paints <= (2 if moving else 0), (name, paints)
            else:
                assert paints >= len(costs), (name, paints)
            print("PASS", name, report["benchmarks"][name] | {"raw_cpu_frame_ms": "saved"}, flush=True)

        command("change", "translate", 37.25)
        same_as_full("fractional-translation")
        command("change", "color", 0.1)
        same_as_full("bound-color")
        command("change", "width", 110.0)
        same_as_full("bound-layout")
        command("change", "rebuild", 0.0)
        same_as_full("explicit-rebuild")
        command("css", "#hover-target { background: #8844cc; }")
        before_hover = same_as_full("stylesheet")
        command("input", json.dumps({"kind": "pointer-move", "x": 350, "y": 50}))
        hover = same_as_full("hover")
        assert difference(before_hover, hover) > 1000, "hover did not repaint"
        command("change", "input", 1.0)
        command("frame", False)
        command("input", json.dumps({"kind": "pointer-down", "x": 70, "y": 155, "button": 0}))
        command("input", json.dumps({"kind": "pointer-up", "x": 70, "y": 155, "button": 0}))
        command("frame", False)
        command("settle")
        before_typing = shot("input-empty")
        command("input", json.dumps({"kind": "text", "text": "Retained text"}))
        typed = same_as_full("input-typing", tolerance=100 * args.dpr * args.dpr)
        assert difference(before_typing, typed) > 100, "typed characters did not render"
        command("input", json.dumps({"kind": "focus", "focused": False}))
        command("frame", False)
        command("settle")
        before_resize = command("frame", False)["paints"]
        command("resize", 600.0, 440.0, float(args.dpr))
        assert command("frame", False)["paints"] > before_resize
        command("settle")
        same_as_full("resize", tolerance=100 * args.dpr * args.dpr)
        assert not errors, errors
        report["page_errors"] = errors
        report["passed"] = True
        context.close()
        browser.close()
finally:
    (args.out / "report.json").write_text(json.dumps(report, indent=2) + "\n")
    server.shutdown()
    server.server_close()
