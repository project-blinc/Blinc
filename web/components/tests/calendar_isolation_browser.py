"""Check two Calendar instances through the production DOM/worker transport.

Build blinc_component_demos with browser-tests into --runtime first. Only the
WASM module is substituted in this isolated browser; the served gallery stays
unchanged. Worker pause/resume makes input arriving between frames deterministic.
"""

import argparse
import io
import re
import shutil
import subprocess
from pathlib import Path
from urllib.parse import urlparse

from PIL import Image, ImageOps
from playwright.sync_api import sync_playwright

CASES = [
    ("queued-before-crossing", ["November", "November"]),
    ("cancelled-before-crossing", ["November", "November"]),
    ("completed-before-crossing", ["December", "November"]),
    ("new-stroke-on-second", ["November", "December"]),
    ("cross-and-return-before-frame", ["November", "November"]),
]

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument("--url", default="http://127.0.0.1:8094")
parser.add_argument("--dpr", type=int, choices=[1, 2], default=2)
parser.add_argument("--runtime", type=Path, default=Path("target/calendar-isolation-runtime"))
parser.add_argument("--case", choices=[name for name, _ in CASES])
args = parser.parse_args()
root = Path(__file__).resolve().parents[3]
runtime = args.runtime.resolve()
ocr = shutil.which("tesseract")
if not ocr:
    raise SystemExit("This check requires tesseract on PATH.")
for file in ["blinc_component_demos.js", "blinc_component_demos_bg.wasm"]:
    if not (runtime / file).is_file():
        raise SystemExit(f"Missing {file}; build the browser-tests worker fixture first.")
out = root / "target/component-browser/calendar-isolation" / f"dpr-{args.dpr}"
out.mkdir(parents=True, exist_ok=True)

with sync_playwright() as playwright:
    browser = playwright.chromium.launch(
        channel="chrome", headless=True, args=["--enable-unsafe-webgpu"]
    )
    context = browser.new_context(
        viewport={"width": 1600, "height": 1100}, device_scale_factor=args.dpr
    )

    def module(route):
        name = Path(urlparse(route.request.url).path).name
        route.fulfill(
            path=str(runtime / name),
            content_type="application/wasm" if name.endswith(".wasm") else "text/javascript",
        )

    context.route("**/runtime/pkg/blinc_component_demos*", module)
    page = context.new_page()
    errors = []
    page.on("pageerror", lambda error: errors.append(str(error)))
    page.add_init_script("""
        window.__calendarWorkers = [];
        const NativeWorker = Worker;
        window.Worker = class extends NativeWorker {
            constructor(...args) {
                super(...args);
                this.offset = 0; this.last = -1;
                window.__calendarWorkers.push(this);
            }
            postMessage(data, transfer) {
                if (data?.version === 1 && Number.isSafeInteger(data.seq)) {
                    data = {...data, seq: data.seq + this.offset};
                    this.last = data.seq; this.instance = data.instance;
                }
                return super.postMessage(data, transfer);
            }
            command(type) {
                this.offset++;
                super.postMessage({version:1, instance:this.instance,
                    seq:++this.last, type, payload:{}});
            }
        };
    """)

    def months(name):
        image = Image.open(io.BytesIO(canvas.screenshot(path=str(out / f"{name}.png")))).convert("RGB")
        image = ImageOps.invert(image)
        image = image.resize((image.width * 2, image.height * 2))
        filename = out / f"{name}-ocr.png"
        image.save(filename)
        text = subprocess.check_output(
            [ocr, str(filename), "stdout", "--psm", "6"], stderr=subprocess.DEVNULL, text=True
        )
        found = re.findall(r"November|December|January|February|March", text)
        assert len(found) == 2, (name, "expected two visible month captions", text)
        return found

    for name, expected in CASES:
        if args.case and args.case != name:
            continue
        page.goto(args.url + "/components/cn/calendar/?tab=live", wait_until="domcontentloaded")
        page.wait_for_function(
            "document.querySelector('#live-canvas').dataset.status === 'ready'", timeout=90000
        )
        canvas = page.locator("#live-canvas canvas")
        canvas.scroll_into_view_if_needed()
        page.wait_for_timeout(400)
        assert months(name + "-start") == ["November", "November"]
        page.evaluate("""async ({name, dpr}) => {
            const canvas = document.querySelector('#live-canvas canvas');
            const bounds = canvas.getBoundingClientRect();
            const worker = window.__calendarWorkers[0];
            const wait = ms => new Promise(resolve => setTimeout(resolve, ms));
            const positions = [
                {clientX:bounds.x+bounds.width/2-152, clientY:bounds.y+180},
                {clientX:bounds.x+bounds.width/2+152, clientY:bounds.y+180},
            ];
            let at = 0;
            const move = index => {
                at = index;
                canvas.dispatchEvent(new PointerEvent('pointermove', positions[at]));
            };
            const wheel = delta => canvas.dispatchEvent(new WheelEvent('wheel', {
                ...positions[at], deltaX:delta*dpr, deltaY:0, deltaMode:0, cancelable:true,
            }));
            const stroke = async () => {
                for (const delta of [3,8,16,25,35,28,20,14,9,6,4,3,2,1]) {
                    wheel(delta); await wait(16);
                }
            };
            move(0);
            if (name === 'completed-before-crossing') {
                for (const delta of [20,30,35,30,20,15,10,8,6]) {
                    wheel(delta); await wait(16);
                }
            } else if (name === 'cancelled-before-crossing' || name === 'new-stroke-on-second') {
                wheel(15); await wait(35);
            }
            worker.command('pause');
            if (name === 'queued-before-crossing' || name === 'cross-and-return-before-frame') {
                wheel(15);
            }
            move(1);
            if (name === 'completed-before-crossing') {
                for (const delta of [4,3,2,1,1,1,1,1,1,1]) wheel(delta);
            } else {
                for (let i=0;i<8;i++) wheel(20);
            }
            if (name === 'cross-and-return-before-frame') {
                move(0);
                for (let i=0;i<8;i++) wheel(20);
            }
            worker.command('resume');
            if (name === 'new-stroke-on-second') {
                await wait(16);
                await stroke();
            }
        }""", {"name": name, "dpr": args.dpr})
        page.wait_for_timeout(650)
        observed = months(name + "-end")
        assert observed == expected, (name, expected, observed)
        print("PASS", name, observed, flush=True)
    assert not errors, errors
    browser.close()
