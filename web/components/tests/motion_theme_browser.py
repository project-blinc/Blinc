"""The Motion demo plays its entrance each time the feed is shown, before and after theme switches."""
import argparse
import io
import time

from PIL import Image, ImageStat
from playwright.sync_api import sync_playwright

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument("--url", default="http://127.0.0.1:8094")
args = parser.parse_args()

TOGGLE = (446, 73)  # "Toggle feed" inside the live canvas
FEED = (40, 110, 540, 360)  # the activity feed region, in canvas pixels


def feed_brightness(canvas):
    image = Image.open(io.BytesIO(canvas.screenshot())).convert("L").crop(FEED)
    return ImageStat.Stat(image).mean[0]


def show_then_hide(page, canvas):
    """Show the feed and report whether an early frame was still mid-entrance."""
    box = canvas.bounding_box()
    page.mouse.click(box["x"] + TOGGLE[0], box["y"] + TOGGLE[1])
    time.sleep(0.15)
    early = feed_brightness(canvas)
    time.sleep(1.5)
    settled = feed_brightness(canvas)
    page.mouse.click(box["x"] + TOGGLE[0], box["y"] + TOGGLE[1])
    time.sleep(1.0)
    return early, settled


with sync_playwright() as playwright:
    browser = playwright.chromium.launch(
        channel="chrome", headless=True, args=["--enable-unsafe-webgpu"]
    )
    page = browser.new_page(viewport={"width": 1200, "height": 1000})
    errors = []
    page.on("pageerror", lambda error: errors.append(str(error)))
    page.goto(f"{args.url}/components/layout/motion/?tab=live", wait_until="domcontentloaded")
    page.locator("#tab-live").click()
    page.wait_for_function(
        "document.querySelector('#live-canvas').dataset.status === 'ready'", timeout=120000
    )
    canvas = page.locator("#live-canvas")
    time.sleep(2)
    for label in [None, "Expressive", "Restrained", "Hybrid"]:
        if label:
            page.get_by_role("button", name=label).click()
            time.sleep(3)
        for attempt in (1, 2):
            early, settled = show_then_hide(page, canvas)
            # A finished entrance is at the settled brightness at the first sample.
            assert settled - early > 3, (label, attempt, "no entrance animation", early, settled)
    assert not errors, errors
    browser.close()
print("motion entrance plays on every show, across theme switches")
