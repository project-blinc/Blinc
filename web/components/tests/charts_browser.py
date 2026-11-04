"""Exercise every Charts section in installed Chrome with an isolated profile."""
import argparse
import io
import json
from pathlib import Path

from PIL import Image
from playwright.sync_api import sync_playwright

parser = argparse.ArgumentParser()
parser.add_argument('--url', default='http://127.0.0.1:8094')
args = parser.parse_args()
root = Path(__file__).resolve().parents[3]
output = root / 'target/component-browser/charts'
output.mkdir(parents=True, exist_ok=True)
catalog = json.loads((root / 'web/components/catalog.json').read_text())
charts = next(c for c in catalog['components'] if c['slug'] == 'charts')

with sync_playwright() as p:
    browser = p.chromium.launch(channel='chrome', headless=True, args=['--enable-unsafe-webgpu'])
    page = browser.new_page(viewport={'width': 1440, 'height': 1000}, device_scale_factor=2)
    errors = []
    page.on('pageerror', lambda error: errors.append(str(error)))
    page.add_init_script('''window.__chartMessages=[];window.__chartState={};const NativeWorker=Worker;window.Worker=class extends NativeWorker{constructor(...args){super(...args);this.addEventListener('message',({data})=>{if(data.type==='state'||data.type==='ready')window.__chartState=data.payload;});}postMessage(data,...rest){window.__chartMessages.push(data.type);return super.postMessage(data,...rest);}};''')
    page.goto(f'{args.url}/components/cn/charts/', wait_until='domcontentloaded')
    page.wait_for_function("document.querySelector('#live-canvas')?.dataset.status==='ready'", timeout=90000)
    assert page.locator('h1').inner_text() == 'Charts'
    assert page.locator('.sidebar h2').all_text_contents() == ['Get started', 'Components', 'Blocks']
    assert page.locator('.sidebar a[aria-current]').evaluate('(node)=>getComputedStyle(node).boxShadow') == 'none'
    assert page.locator('[data-chart-section]').count() == len(charts['examples'])
    assert page.locator('iframe').count() == 0, 'extra live hosts must load on demand'
    page.screenshot(path=str(output / 'page-dark.png'))
    previous = None
    for example in charts['examples']:
        slug = example['slug']
        section = page.locator(f'#{slug}')
        if slug == 'sparkline':
            assert section.locator('[data-chart-animation]').is_hidden()
            assert '/poster.png' in section.locator('[data-chart-image]').get_attribute('src')
        section.get_by_role('tab', name='Code', exact=True).click()
        assert f"pub fn {example['entry']}(" in section.locator('pre code').inner_text()
        assert section.locator('.hljs-call').count() > 0
        section.get_by_role('tab', name='Live', exact=True).click()
        host = section.locator('iframe')
        host.wait_for()
        frame = host.element_handle().content_frame()
        frame.wait_for_function("document.querySelector('#live-canvas')?.dataset.status==='ready'", timeout=90000)
        frame.wait_for_function("window.__chartMessages.at(-1)!=='pause'", timeout=10000)
        page.wait_for_timeout(600)
        assert page.locator('iframe').count() == 1
        if previous:
            assert previous.is_detached(), 'the previous extra host must be released'
        previous = frame
        raw = frame.locator('#live-canvas canvas').screenshot(path=str(output / f'{slug}.png'))
        image = Image.open(io.BytesIO(raw)).convert('RGB')
        pixels = image.tobytes()
        coloured = sum(max(pixels[i:i+3])-min(pixels[i:i+3]) > 35 and max(pixels[i:i+3]) > 80 for i in range(0, len(pixels), 3))
        assert coloured > 100, f'{slug}: no coloured plot pixels'
        print('PASS', slug, frame.evaluate('window.__chartState.backend'), coloured, flush=True)
        if slug == 'area-chart':
            frame.evaluate("window.__originalCanvas=document.querySelector('#live-canvas canvas')")
            assert '--chart-1: var(--primary)' in frame.locator('#custom-css').input_value()
            frame.locator('#apply-css').click()
            frame.wait_for_function("document.querySelector('#css-status').textContent==='Applied'")
            assert frame.evaluate("window.__originalCanvas===document.querySelector('#live-canvas canvas')")
            frame.locator('#live-canvas canvas').screenshot(path=str(output / 'area-custom-css.png'))
            frame.locator('[data-theme-family="expressive"]').click()
            frame.wait_for_function("window.__chartState.family==='expressive'")
            assert frame.evaluate("window.__originalCanvas===document.querySelector('#live-canvas canvas')")
            page.locator('label:has(#page-scheme)').click()
            frame.wait_for_function("document.documentElement.dataset.scheme==='light'")
            assert frame.evaluate("window.__originalCanvas===document.querySelector('#live-canvas canvas')")
        section.get_by_role('tab', name='Code', exact=True).click()
        frame.wait_for_function("window.__chartMessages.at(-1)==='pause'")
    page.locator('#tab-preview').focus()
    page.keyboard.press('ArrowRight')
    assert page.locator('#tab-live').get_attribute('aria-selected') == 'true'
    assert page.locator('iframe').count() == 0
    page.set_viewport_size({'width': 390, 'height': 844})
    page.locator('#tab-preview').click()
    page.evaluate('window.scrollTo(0,0)')
    page.screenshot(path=str(output / 'page-mobile.png'))
    assert page.evaluate('document.documentElement.scrollWidth<=innerWidth'), 'mobile page overflows'
    section = page.locator('#area-chart')
    section.get_by_role('tab', name='Live', exact=True).click()
    frame = section.locator('iframe').element_handle().content_frame()
    frame.wait_for_function("document.querySelector('#live-canvas')?.dataset.status==='ready'", timeout=90000)
    assert frame.evaluate('document.documentElement.scrollWidth<=innerWidth'), 'embedded mobile controls overflow'
    page.wait_for_timeout(600)
    section.screenshot(path=str(output / 'area-mobile.png'))
    page.goto(f'{args.url}/components/cn/line-chart/', wait_until='domcontentloaded')
    page.wait_for_url('**/components/cn/charts/#line-chart')
    assert page.locator('h1').inner_text() == 'Charts'
    assert not errors, errors
    print('PASS chart hosts, pause/disposal, scheme propagation, source and legacy URL', flush=True)
    browser.close()
