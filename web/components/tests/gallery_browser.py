"""Render the complete catalog in isolated Chrome and exercise built-in editors."""
import argparse, io, json
from pathlib import Path
from PIL import Image, ImageChops
from playwright.sync_api import sync_playwright

parser = argparse.ArgumentParser()
parser.add_argument('--url', default='http://127.0.0.1:8094')
args = parser.parse_args()
root = Path(__file__).resolve().parents[3]
catalog = json.loads((root/'web/components/catalog.json').read_text())
out = root/'target/component-browser/gallery'; out.mkdir(parents=True, exist_ok=True)
with sync_playwright() as p:
    browser = p.chromium.launch(channel='chrome', headless=True, args=['--enable-unsafe-webgpu'])
    context = browser.new_context(viewport={'width':1440, 'height':1100}, device_scale_factor=1)
    page = context.new_page(); errors=[]
    page.on('pageerror', lambda error: errors.append(str(error)))
    page.add_init_script('''window.__workers=0;window.__state={};const OriginalWorker=Worker;
    window.Worker=class extends OriginalWorker {constructor(...args){super(...args);window.__workers++;
      this.addEventListener('message',({data})=>{if(['state','ready'].includes(data.type))window.__state=data.payload;});}};''')
    def snapshot(name):
        raw = page.locator('#live-canvas canvas').screenshot(path=str(out/f'{name}.png'))
        return Image.open(io.BytesIO(raw)).convert('RGB')
    for c in catalog['components']:
        slug=c['slug']; route=f"/components/{c['library']}/{slug}/"
        response=page.goto(args.url+route, wait_until='domcontentloaded')
        assert response.status==200, route
        live='worker-canvas' in c['capabilities']
        expected='ready' if live else 'native'
        page.wait_for_function('(expected)=>document.querySelector("#live-canvas").dataset.status===expected', arg=expected, timeout=90000)
        page.get_by_role('tab', name='Live', exact=True).first.click()
        if not live:
            assert page.evaluate('window.__workers')==0
            assert page.locator('#live-fallback').is_visible()
            assert 'Live playback is not available' in page.locator('#live-status').inner_text()
            print('PASS native preview and source', slug, flush=True)
            continue
        canvas=page.locator('#live-canvas canvas')
        page.wait_for_function('(()=>{const c=document.querySelector("#live-canvas canvas");return c.width===Math.round(c.getBoundingClientRect().width*devicePixelRatio)})()')
        page.wait_for_timeout(180)
        assert page.evaluate('window.__workers')==1, slug
        shot=snapshot(slug)
        assert len(shot.getcolors(shot.width*shot.height))>10, f'{slug}: blank canvas'
        page.evaluate('window.__canvas=document.querySelector("#live-canvas canvas")')
        # Every recipe must apply through the current worker, without losing its canvas.
        if c.get('custom_css', True):
            page.locator('#apply-css').click()
            page.wait_for_function('document.querySelector("#css-status").textContent==="Applied"', timeout=15000)
        else:
            assert not page.locator('.css-editor').is_visible()
        assert page.evaluate('window.__canvas===document.querySelector("#live-canvas canvas")')
        assert page.evaluate('window.__workers')==1
        if slug in ['textarea','code-editor','rich-text-editor']:
            page.locator('#clear-css').click()
            page.wait_for_function('document.querySelector("#css-status").textContent==="Default styles"')
            canvas.click()
            page.wait_for_timeout(100)
            before=snapshot(slug+'-before-typing')
            page.keyboard.type('BrowserCheck', delay=30)
            field={'textarea':'name','code-editor':'code','rich-text-editor':'document'}[slug]
            page.wait_for_function('(field)=>window.__state[field]?.includes("BrowserCheck")', arg=field)
            page.wait_for_timeout(100)
            after=snapshot(slug+'-typed')
            # Exclude the feedback caption; check actual editor pixels.
            w,h=before.size
            box=(max(0,w//2-210),max(0,h//2-105),min(w,w//2+210),min(h,h//2+65))
            diff=ImageChops.difference(before.crop(box),after.crop(box))
            assert sum(sum(pixel)>60 for pixel in diff.getdata())>100, f'{slug}: editor text did not repaint'
            for family in ['restrained','expressive','hybrid']:
                page.locator(f'[data-theme-family="{family}"]').click()
                page.wait_for_function('(family)=>window.__state.family===family', arg=family)
                assert 'BrowserCheck' in page.evaluate('(field)=>window.__state[field]',field)
            assert page.evaluate('window.__workers')==1
            assert page.evaluate('window.__canvas===document.querySelector("#live-canvas canvas")')
        assert not errors, (slug, errors)
        print('PASS worker, viewport, recipe and pixels',slug,flush=True)
    context.close()
    mobile=browser.new_context(viewport={'width':390,'height':844}, device_scale_factor=2)
    page=mobile.new_page()
    for library,slug in [('cn','calendar'),('cn','time-picker'),('cn','color-picker'),('layout','code-editor'),('layout','rich-text-editor')]:
        page.goto(f'{args.url}/components/{library}/{slug}/',wait_until='domcontentloaded')
        page.wait_for_function('document.querySelector("#live-canvas").dataset.status==="ready"',timeout=90000)
        page.get_by_role('tab',name='Live',exact=True).click()
        page.wait_for_timeout(150)
        assert page.evaluate('document.documentElement.scrollWidth<=innerWidth'),slug
        page.locator('#live-canvas canvas').screenshot(path=str(out/f'{slug}-mobile.png'))
        print('PASS mobile viewport',slug,flush=True)
    mobile.close();browser.close()
print(f'Validated {len(catalog["components"])} pages and all Live recipes in isolated Chrome.',flush=True)
