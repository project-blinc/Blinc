"""Check selected Calendar colours through the real Chrome worker and CSS editor."""

import argparse
import io
from pathlib import Path

from PIL import Image
from playwright.sync_api import sync_playwright


parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument("--url", default="http://127.0.0.1:8094")
parser.add_argument("--dpr", type=int, choices=[1, 2], default=2)
args = parser.parse_args()
out = Path(__file__).resolve().parents[3] / "target/component-browser/selection-styles"
out.mkdir(parents=True, exist_ok=True)

with sync_playwright() as playwright:
    browser = playwright.chromium.launch(
        channel="chrome", headless=True, args=["--enable-unsafe-webgpu"]
    )
    page = browser.new_page(
        viewport={"width": 1440, "height": 1100}, device_scale_factor=args.dpr
    )
    errors = []
    page.on("pageerror", lambda error: errors.append(str(error)))
    page.add_init_script("""
        window.__workerCount = 0;
        const NativeWorker = Worker;
        window.Worker = class extends NativeWorker {
            constructor(...args) { super(...args); window.__workerCount++; }
        };
    """)
    page.goto(args.url + "/components/cn/calendar/?tab=live", wait_until="domcontentloaded")
    page.wait_for_function(
        "document.querySelector('#live-canvas').dataset.status === 'ready'", timeout=90000
    )
    canvas = page.locator("#live-canvas canvas")
    page.evaluate("window.__selectionCanvas = document.querySelector('#live-canvas canvas')")
    workers = page.evaluate("window.__workerCount")
    app_css = ".demo-calendar .cn-calendar-event { background: #ef4444; }"

    def apply(css):
        page.locator("#custom-css").fill(css)
        page.locator("#apply-css").click()
        page.wait_for_function("document.querySelector('#css-status').textContent === 'Applied'")
        page.wait_for_timeout(450)
        assert page.evaluate("window.__workerCount") == workers
        assert page.evaluate(
            "window.__selectionCanvas === document.querySelector('#live-canvas canvas')"
        )

    def picture(name):
        raw = canvas.screenshot(path=str(out / f"dpr-{args.dpr}-{name}.png"))
        return Image.open(io.BytesIO(raw)).convert("RGB")

    def points(image, colour):
        return [
            (x, y) for y in range(image.height) for x in range(image.width)
            if max(abs(a - b) for a, b in zip(image.getpixel((x, y)), colour)) <= 3
        ]

    def click(x, y):
        canvas.click(position={"x": x / args.dpr, "y": y / args.dpr})
        page.mouse.move(8, 8)
        page.wait_for_timeout(450)

    for dark, inverse, scheme in [(True, (15, 19, 32), "dark"), (False, (255, 255, 255), "light")]:
        page.locator("#demo-dark").set_checked(dark, force=True)
        page.wait_for_timeout(350)
        apply(app_css)
        image = picture(scheme + "-ordinary")
        red = points(image, (239, 68, 68))
        assert red, "the app event colour did not render"
        x = round(sum(x for x, _ in red) / len(red))
        y = round(sum(y for _, y in red) / len(red))
        click(x, y - 10 * args.dpr)
        selected = picture(scheme + "-selected")
        assert max(abs(a - b) for a, b in zip(selected.getpixel((x, y)), inverse)) <= 3, (
            scheme, "selected event dot", selected.getpixel((x, y)), inverse
        )
        apply(app_css + "\n:root { --cn-calendar-selected-color: #facc15; }")
        themed = picture(scheme + "-selected-token")
        assert themed.getpixel((x, y)) == (250, 204, 21), "selected colour token did not update"
        apply(app_css + """
            @layer components {
                .demo-calendar .cn-calendar-day[data-selected] .cn-calendar-event {
                    background: #22c55e !important;
                }
            }
        """)
        variant = picture(scheme + "-selected-variant")
        assert variant.getpixel((x, y)) == (34, 197, 94), "selected variant did not update"
        page.locator("#custom-css").fill(".cn-calendar { opacity: .5 !important; }")
        page.locator("#apply-css").click()
        page.wait_for_function("document.querySelector('#css-status').dataset.error === 'true'")
        assert picture(scheme + "-invalid-css").getpixel((x, y)) == (34, 197, 94)
        click(x + 36 * args.dpr, y - 10 * args.dpr)
        assert picture(scheme + "-deselected").getpixel((x, y)) == (239, 68, 68)
        assert page.evaluate("window.__workerCount") == workers
        print("PASS", scheme, "selection, deselection, token, variant and invalid CSS rollback", flush=True)
    assert not errors, errors
    browser.close()
