"""Verify keyboard cancellation in a real Chrome WebGPU worker."""

import argparse
import json
import threading
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path
from urllib.parse import urlparse

from PIL import Image, ImageChops
from playwright.sync_api import sync_playwright

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument("--runtime", type=Path, required=True)
parser.add_argument("--out", type=Path, required=True)
args = parser.parse_args()
runtime = args.runtime.resolve()
args.out.mkdir(parents=True, exist_ok=True)

INDEX = """<!doctype html><canvas id="canvas" width="640" height="480"></canvas>
<script>
const worker = new Worker('/worker.js', {type:'module'}), pending = new Map(); let seq = 0;
window.call = (cmd, args=[]) => new Promise((resolve,reject) => {
  const id = seq++; pending.set(id, [resolve,reject]); worker.postMessage({id,cmd,args});
});
worker.onmessage = ({data}) => {
  const [resolve,reject] = pending.get(data.id); pending.delete(data.id);
  data.error ? reject(new Error(data.error)) : resolve(data.value);
};
window.keyEvents = [];
addEventListener('keydown', async e => {
  e.preventDefault();
  const text = Array.from(e.key).length === 1 && !e.metaKey && (!e.ctrlKey || e.altKey) ? e.key : '';
  const value = await call('key', [e.keyCode,text,e.ctrlKey,e.altKey,e.metaKey]);
  keyEvents.push({...value, code:e.keyCode, text});
});
window.ready = new Promise((resolve,reject) => {
  const id = seq++; pending.set(id, [resolve,reject]);
  const canvas = document.querySelector('canvas').transferControlToOffscreen();
  worker.postMessage({id,cmd:'init',canvas}, [canvas]);
});
</script>"""

WORKER = """import init, {create_cache_probe} from '/runtime/blinc_component_demos.js';
let probe;
onmessage = async ({data:{id,cmd,args,canvas}}) => {
  try {
    if (cmd === 'init') {
      await init(); probe = await create_cache_probe(canvas, 1, 0);
      probe.change('input', 1);
      if (!JSON.parse(probe.frame(true)).gpu) throw new Error('WebGPU adapter required');
      probe.input(JSON.stringify({kind:'pointer-down',x:60,y:155,button:0}));
      probe.input(JSON.stringify({kind:'pointer-up',x:60,y:155,button:0}));
    } else if (cmd === 'key') {
      const prevented = probe.key(...args); probe.frame(false);
      postMessage({id,value:{prevented,value:probe.input_value()}}); return;
    } else if (cmd === 'text') {
      probe.input(JSON.stringify({kind:'text',text:args[0]}));
    }
    probe.frame(false);
    postMessage({id,value:probe.input_value()});
  } catch (error) {postMessage({id,error:String(error.stack || error)});}
};"""


class Handler(BaseHTTPRequestHandler):
    def log_message(self, *_):
        pass

    def do_GET(self):
        path = urlparse(self.path).path
        if path in {"/", "/worker.js"}:
            body = (INDEX if path == "/" else WORKER).encode()
            content_type = "text/html" if path == "/" else "text/javascript"
        elif path.startswith("/runtime/"):
            file = (runtime / path.removeprefix("/runtime/")).resolve()
            if not file.is_relative_to(runtime) or not file.is_file():
                self.send_error(404)
                return
            body = file.read_bytes()
            content_type = "application/wasm" if file.suffix == ".wasm" else "text/javascript"
        else:
            self.send_error(404)
            return
        self.send_response(200)
        self.send_header("Content-Type", content_type)
        self.send_header("Content-Length", str(len(body)))
        self.send_header("Cross-Origin-Opener-Policy", "same-origin")
        self.send_header("Cross-Origin-Embedder-Policy", "require-corp")
        self.end_headers()
        self.wfile.write(body)


server = ThreadingHTTPServer(("127.0.0.1", 0), Handler)
threading.Thread(target=server.serve_forever, daemon=True).start()
try:
    with sync_playwright() as p:
        browser = p.chromium.launch(channel="chrome", headless=True, args=["--enable-unsafe-webgpu"])
        page = browser.new_page(viewport={"width": 700, "height": 540})
        errors = []
        page.on("pageerror", lambda e: errors.append(str(e)))
        page.goto(f"http://127.0.0.1:{server.server_port}/")
        page.evaluate("window.ready")
        canvas = page.locator("canvas")
        canvas.screenshot(path=str(args.out / "empty.png"))

        def press(chord, code):
            page.evaluate("keyEvents.length = 0")
            page.keyboard.press(chord)
            page.wait_for_function("code => keyEvents.some(e => e.code === code)", arg=code)
            return page.evaluate("code => keyEvents.find(e => e.code === code)", code)

        typed = press("f", 70)
        assert typed == {"prevented": False, "value": "f", "code": 70, "text": "f"}, typed
        canvas.screenshot(path=str(args.out / "typed.png"))
        before = Image.open(args.out / "empty.png").convert("RGB").crop((35, 145, 270, 178))
        after = Image.open(args.out / "typed.png").convert("RGB").crop((35, 145, 270, 178))
        assert ImageChops.difference(before, after).getbbox(), "field pixels did not follow typed text"
        pixels = after.tobytes()
        ink = sum(min(pixels[i:i + 3]) > 130 for i in range(0, len(pixels), 3))
        assert ink > 16, f"field shows no typed glyph beyond its caret: {ink} ink pixels"

        cancelled = press("Alt+f", 70)
        assert cancelled["prevented"] and cancelled["value"] == "f", cancelled
        option = press("Alt+b", 66)
        assert option["text"] and not option["prevented"], option
        expected = "f" + option["text"]
        assert option["value"] == expected, option
        altgr = press("Control+Alt+a", 65)
        expected += altgr["text"]
        assert altgr["text"] and altgr["value"] == expected, altgr
        assert press("Control+f", 70)["value"] == expected
        assert press("Meta+f", 70)["value"] == expected
        for key, code in [("F1", 112), ("F12", 123), ("Insert", 45)]:
            assert press(key, code)["value"] == expected
        assert page.evaluate("call('key', [70,'f',false,true,false])")["prevented"]
        committed = page.evaluate("call('text', ['é'])")
        assert committed == expected + "é", committed
        canvas.screenshot(path=str(args.out / "committed.png"))
        assert not errors, errors
        result = {"plain": typed, "cancelled": cancelled, "option": option, "altgr": altgr, "committed": committed, "errors": errors}
        (args.out / "report.json").write_text(json.dumps(result, indent=2) + "\n")
        print("PASS Chrome WebGPU: key order, cancellation, Option text, Ctrl/Meta, function keys, committed text and field pixels", flush=True)
        browser.close()
finally:
    server.shutdown()
