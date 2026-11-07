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
    assert page.locator('.chart-live canvas').count() == 0, 'extra live hosts must load on demand'
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
        canvas = section.locator('.chart-live canvas')
        canvas.wait_for()
        section.locator('.chart-live-status').wait_for(state='detached', timeout=90000)
        page.wait_for_function("window.__chartMessages.at(-1)!=='pause'", timeout=10000)
        page.wait_for_timeout(600)
        assert page.locator('.chart-live canvas').count() == 1
        if previous:
            assert previous.evaluate('(node)=>!node.isConnected'), 'the previous extra host must be released'
        previous = canvas.element_handle()
        raw = canvas.screenshot(path=str(output / f'{slug}.png'))
        image = Image.open(io.BytesIO(raw)).convert('RGB')
        pixels = image.tobytes()
        coloured = sum(max(pixels[i:i+3])-min(pixels[i:i+3]) > 35 and max(pixels[i:i+3]) > 80 for i in range(0, len(pixels), 3))
        assert coloured > 100, f'{slug}: no coloured plot pixels'
        print('PASS', slug, page.evaluate('window.__chartState.backend'), coloured, flush=True)
        if slug == 'area-chart':
            page.evaluate("window.__chartMessages.length=0")
            page.locator('label:has(#page-scheme)').click()
            page.wait_for_function("window.__chartMessages.includes('theme')")
        section.get_by_role('tab', name='Code', exact=True).click()
        page.wait_for_function("window.__chartMessages.at(-1)==='pause'")
    page.locator('#tab-preview').focus()
    page.keyboard.press('ArrowRight')
    assert page.locator('#tab-live').get_attribute('aria-selected') == 'true'
    assert page.locator('.chart-live canvas').count() == 0
    page.set_viewport_size({'width': 390, 'height': 844})
    page.locator('#tab-preview').click()
    page.evaluate('window.scrollTo(0,0)')
    page.screenshot(path=str(output / 'page-mobile.png'))
    assert page.evaluate('document.documentElement.scrollWidth<=innerWidth'), 'mobile page overflows'
    section = page.locator('#area-chart')
    section.get_by_role('tab', name='Live', exact=True).click()
    section.locator('.chart-live canvas').wait_for()
    section.locator('.chart-live-status').wait_for(state='detached', timeout=90000)
    assert page.evaluate('document.documentElement.scrollWidth<=innerWidth'), 'live chart overflows on mobile'
    page.wait_for_timeout(600)
    section.screenshot(path=str(output / 'area-mobile.png'))
    page.goto(f'{args.url}/components/cn/line-chart/', wait_until='domcontentloaded')
    page.wait_for_url('**/components/cn/charts/#line-chart')
    assert page.locator('h1').inner_text() == 'Charts'
    assert not errors, errors
    print('PASS chart hosts, pause/disposal, scheme propagation, source and legacy URL', flush=True)
    browser.close()
