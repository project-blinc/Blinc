"""Verify decoded video audio and the real Blinc transport controls in Chrome."""
import argparse
from pathlib import Path

from playwright.sync_api import sync_playwright

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument("--url", default="http://127.0.0.1:8094")
parser.add_argument("--dpr", type=int, choices=[1, 2], default=2)
args = parser.parse_args()
out = Path(__file__).resolve().parents[3] / "target/component-browser/video-audio"
out.mkdir(parents=True, exist_ok=True)

with sync_playwright() as playwright:
    # Keep Chrome's default silent test output; measure the decoded signal directly.
    browser = playwright.chromium.launch(
        channel="chrome", headless=True, args=["--enable-unsafe-webgpu"]
    )
    page = browser.new_page(viewport={"width": 1440, "height": 1100}, device_scale_factor=args.dpr)
    errors = []
    page.on("pageerror", lambda error: errors.append(str(error)))
    page.add_init_script("""
        window.__videoState = {}; window.__videoWorkers = 0;
        const NativeWorker = Worker;
        window.Worker = class extends NativeWorker {
            constructor(...args) {
                super(...args); window.__videoWorkers++;
                this.addEventListener('message', ({data}) => {
                    if (['ready', 'state'].includes(data.type)) window.__videoState = data.payload;
                });
            }
        };
    """)
    page.goto(args.url + "/components/layout/video-player/?tab=live", wait_until="domcontentloaded")
    page.locator("#live-canvas").scroll_into_view_if_needed()
    page.wait_for_function("document.querySelector('#live-canvas').dataset.status === 'ready'", timeout=90000)
    page.wait_for_function("window.__videoState.video?.duration > 10000", timeout=20000)
    initial = page.evaluate("""() => {
        const v = document.querySelector('video');
        return {paused: v.paused, muted: v.muted, volume: v.volume};
    }""")
    assert initial == {"paused": True, "muted": False, "volume": 1}, initial
    initial_frame = page.evaluate("window.__videoState.video.frame")
    canvas = page.locator("#live-canvas canvas")
    page.evaluate("""() => {
        const v = document.querySelector('video');
        const context = new AudioContext();
        const source = context.createMediaElementSource(v);
        const analyser = context.createAnalyser(); analyser.fftSize = 2048;
        const silent = context.createGain(); silent.gain.value = 0;
        source.connect(analyser); analyser.connect(silent); silent.connect(context.destination);
        window.__audioProbe = {context, analyser, samples: new Float32Array(analyser.fftSize)};
        document.querySelector('#live-canvas canvas').addEventListener('pointerdown', () => context.resume(), {once:true});
        window.__audioRms = () => {
            const p = window.__audioProbe; p.analyser.getFloatTimeDomainData(p.samples);
            return Math.sqrt(p.samples.reduce((sum, value) => sum + value * value, 0) / p.samples.length);
        };
        window.__originalVideo = v;
    }""")

    def click_control(offset):
        box = canvas.bounding_box()
        width = min(420, box["width"] - 48)
        left = box["x"] + (box["width"] - width) / 2
        x = left + (20 if offset == "play" else width + offset)
        y = box["y"] + (box["height"] - 240) / 2 + 220
        page.mouse.click(x, y)

    def audible():
        page.wait_for_function("!document.querySelector('video').paused && window.__audioProbe.context.state === 'running' && window.__audioRms() > 0.001", timeout=10000)
        assert page.evaluate("!document.querySelector('video').muted && document.querySelector('video').volume > 0")
        return page.evaluate("({rms: window.__audioRms(), decoded: document.querySelector('video').webkitAudioDecodedByteCount})")

    click_control("play")
    page.wait_for_function("window.__videoState.video.playing && window.__videoState.video.position > 300")
    signal = audible()
    assert signal["decoded"] > 0, signal
    page.wait_for_function("(frame) => window.__videoState.video.frame > frame + 2", arg=initial_frame)
    print("PASS user-initiated video playback has decoded audio", signal, flush=True)
    canvas.screenshot(path=str(out / f"dpr-{args.dpr}-playing.png"))
    click_control(-70)
    page.wait_for_function("document.querySelector('video').volume === 0")
    click_control(-70)
    page.wait_for_function("document.querySelector('video').volume === 1")
    audible()
    click_control(-31)
    page.wait_for_function("document.querySelector('video').volume > .1 && document.querySelector('video').volume < .9")
    click_control("play")
    page.wait_for_function("document.querySelector('video').paused")
    paused = page.evaluate("document.querySelector('video').currentTime")
    page.wait_for_timeout(350)
    assert abs(page.evaluate("document.querySelector('video').currentTime") - paused) < .1
    click_control("play")
    audible()
    page.get_by_role("button", name="Expressive", exact=True).click()
    page.wait_for_timeout(400)
    assert page.evaluate("document.querySelector('video') === window.__originalVideo && window.__videoWorkers === 1")
    audible()
    page.locator("#tab-preview").click()
    page.wait_for_function("document.querySelector('video').paused")
    assert not errors, errors
    print("PASS mute/unmute, volume, pause/resume, theme retention and hidden-preview pause", flush=True)
    browser.close()
