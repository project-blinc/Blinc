"""Check actual audio playback and waveform pixels in isolated Chrome."""
import argparse
import io
from pathlib import Path
from PIL import Image
from playwright.sync_api import sync_playwright

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument('--url', default='http://127.0.0.1:8094')
args = parser.parse_args()
out = Path(__file__).resolve().parents[3] / 'target/component-browser/audio-waveform'
out.mkdir(parents=True, exist_ok=True)

with sync_playwright() as p:
    browser = p.chromium.launch(channel='chrome', headless=True, args=['--enable-unsafe-webgpu'])
    page = browser.new_page(viewport={'width': 1440, 'height': 1100}, device_scale_factor=2)
    errors = []
    page.on('pageerror', lambda error: errors.append(str(error)))
    page.add_init_script('''window.__media = {}; const OriginalWorker = Worker;
    window.Worker = class extends OriginalWorker {
      constructor(...args) { super(...args); this.addEventListener('message', ({data}) => {
        if (['state', 'ready'].includes(data.type)) window.__media = data.payload;
      }); }
    };''')
    page.goto(args.url + '/components/layout/audio-player/', wait_until='domcontentloaded')
    page.get_by_role('tab', name='Live', exact=True).click()
    canvas = page.locator('#live-canvas canvas')
    canvas.scroll_into_view_if_needed()
    page.wait_for_function('document.querySelector("#live-canvas").dataset.status === "ready"', timeout=90000)
    page.wait_for_function('window.__media.audio?.duration > 10000', timeout=20000)
    page.evaluate('document.querySelector("video").muted = true')

    def capture(name):
        image = Image.open(io.BytesIO(canvas.screenshot(path=str(out / f'{name}.png')))).convert('RGB')
        box = canvas.bounding_box()
        scale = image.width / box['width']
        width = min(420, box['width'] - 48)
        left = (box['width'] - width) / 2
        return image.crop(tuple(round(v * scale) for v in (
            left, box['height'] / 2 - 75, left + width, box['height'] / 2 - 15)))

    def played(image):
        # Hybrid dark's primary bars, excluding the transport row and caption.
        return sum(b > 200 and 90 < r < 170 and 135 < g < 205 for r, g, b in image.get_flattened_data())

    def toggle():
        box = canvas.bounding_box()
        width = min(420, box['width'] - 48)
        page.mouse.click(box['x'] + (box['width'] - width) / 2 + 20,
                         box['y'] + box['height'] / 2 + 16)

    initial = capture('initial')
    toggle()
    page.wait_for_function('window.__media.audio.playing && window.__media.audio.position > 300')
    early = capture('playing-early')
    position = page.evaluate('window.__media.audio.position')
    page.wait_for_function('(position) => window.__media.audio.position > position + 1200', arg=position)
    later = capture('playing-later')
    assert played(later) > played(early) + 100, (played(initial), played(early), played(later))
    print('PASS waveform pixels advance with actual playback', played(early), played(later), flush=True)
    toggle()
    page.wait_for_function('!window.__media.audio.playing')
    paused = capture('paused')
    page.wait_for_timeout(450)
    assert capture('paused-stable').tobytes() == paused.tobytes()
    box = canvas.bounding_box()
    width = min(420, box['width'] - 48)
    page.mouse.click(box['x'] + (box['width'] - width) / 2 + width * 0.55,
                     box['y'] + box['height'] / 2 + 16)
    page.wait_for_function('window.__media.audio.position > window.__media.audio.duration * 0.25')
    sought = capture('sought')
    assert played(sought) > played(paused) + 500
    assert not page.evaluate('window.__media.audio.playing')
    page.wait_for_timeout(300)
    assert capture('sought-stable').tobytes() == sought.tobytes()
    toggle()
    page.wait_for_function('window.__media.audio.playing')
    position = page.evaluate('window.__media.audio.position')
    page.wait_for_function('(position) => window.__media.audio.position > position + 1000', arg=position)
    assert played(capture('resumed')) > played(sought) + 100
    assert not errors, errors
    print('PASS pause, seek while paused, and resume', flush=True)
    browser.close()
