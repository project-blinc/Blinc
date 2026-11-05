"""Verify canonical component CSS, highlighting and copy actions in Code views."""
import argparse
from pathlib import Path

from playwright.sync_api import sync_playwright

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument("--url", default="http://127.0.0.1:8094")
args = parser.parse_args()
out = Path(__file__).resolve().parents[3] / "target/component-browser/component-styles"
out.mkdir(parents=True, exist_ok=True)

with sync_playwright() as playwright:
    browser = playwright.chromium.launch(
        channel="chrome", headless=True, args=["--enable-unsafe-webgpu"]
    )
    page = browser.new_page(viewport={"width": 1440, "height": 1100}, device_scale_factor=2)
    errors = []
    page.on("pageerror", lambda error: errors.append(str(error)))
    # Capture the clipboard boundary inside the isolated page.
    page.add_init_script("""
        Object.defineProperty(navigator, 'clipboard', {
            value: { writeText: async value => { window.__copiedSource = value; } }
        });
    """)
    for library, slug, required, excluded in [
        ("cn", "checkbox", [".blinc-checkbox-mark", ".cn-checkbox:checked", "!important"], [".cn-radio"]),
        ("cn", "input", [".blinc-input:focus", ".cn-input", ".cn-label-required"], [".cn-input-otp", ".cn-button"]),
        ("cn", "spinner", ["@keyframes cn-spinner-turn", "rotate(360deg)"], ["cn-fade-in"]),
        ("layout", "link", [".blinc-link", "var(--text-link)"], [".cn-button"]),
        ("cn", "charts", [".cn-chart", "--cn-chart-grid"], [".cn-button"]),
    ]:
        page.goto(f"{args.url}/components/{library}/{slug}/?tab=code", wait_until="domcontentloaded")
        page.locator("#tab-code").click()
        code = page.locator("#component-css")
        code.wait_for(state="visible")
        source = code.text_content()
        for value in required:
            assert value in source, (slug, "missing", value)
        for value in excluded + [".demo-", ".catalog-demo"]:
            assert value not in source, (slug, "unrelated", value)
        assert source.startswith("@layer components {\n")
        assert code.locator(".hljs-selector-class").count() > 0
        assert page.locator("#demo-css").count() == 0
        page.locator('[data-copy="component-css"]').click()
        page.wait_for_function("window.__copiedSource !== undefined")
        assert page.evaluate("window.__copiedSource") == source, "Copy CSS changed the snippet"
        if slug == "checkbox":
            code.locator("..").screenshot(path=str(out / "checkbox-css.png"))
        if slug in {"checkbox", "spinner"}:
            page.locator("#tab-live").click()
            page.wait_for_function("document.querySelector('#live-canvas').dataset.status === 'ready'", timeout=90000)
            page.locator("#custom-css").fill(source)
            page.locator("#apply-css").click()
            page.wait_for_function("document.querySelector('#css-status').textContent === 'Applied'")
            page.locator("#tab-code").click()
        if slug == "charts":
            page.locator("#area-chart-code-tab").click()
            child = page.locator("#area-chart-css")
            assert child.is_visible()
            assert child.text_content() == source
            page.locator('[data-copy="area-chart-css"]').click()
            assert page.evaluate("window.__copiedSource") == source
        print("PASS", slug, "component defaults, highlighting and Copy CSS", flush=True)
    page.goto(args.url + "/components/layout/canvas/?tab=code", wait_until="domcontentloaded")
    page.locator("#tab-code").click()
    assert page.locator("#rust-source").is_visible()
    assert page.locator("#component-css").count() == 0
    assert page.locator('#panel-code [data-copy="demo-css"]').count() == 0
    print("PASS Canvas has Rust usage without fabricated stylesheet defaults", flush=True)
    assert not errors, errors
    browser.close()
