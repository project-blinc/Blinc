"""Check documentation discovery in isolated Chrome, including source-only previews."""
import argparse
import io
import re
from pathlib import Path
from PIL import Image
from playwright.sync_api import sync_playwright

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument('--url', default='http://127.0.0.1:8094')
parser.add_argument('--source', action='store_true', help='Overlay current frontend sources on the staged site without rebuilding WASM.')
args = parser.parse_args()
root = Path(__file__).resolve().parents[3]
web = root / 'web/components'
out = root / 'target/component-browser/content-nudge'
out.mkdir(parents=True, exist_ok=True)

with sync_playwright() as p:
    browser = p.chromium.launch(channel='chrome', headless=True, args=['--enable-unsafe-webgpu'])
    context = browser.new_context(viewport={'width': 1440, 'height': 900}, device_scale_factor=2)
    errors = []
    context.on('weberror', lambda error: errors.append(str(error.error)))
    if args.source:
        def overlay(route):
            url = route.request.url.split('?')[0]
            name = url.rsplit('/', 1)[-1]
            if name in ['detail.js', 'content-nudge.js', 'catalog.css']:
                route.fulfill(path=str(web / name), content_type='text/css' if name.endswith('.css') else 'text/javascript')
            elif route.request.resource_type == 'document' and '/components/' in url:
                response = route.fetch()
                html = response.text()
                if 'id="component-config"' in html and 'id="content-nudge"' not in html:
                    template = (web / 'templates/detail.html').read_text()
                    link = re.search(r'<a id="content-nudge".*?</a>', template).group()
                    html = html.replace('<section class="content-section"><h2>Use ', link + '\n<section id="component-content" class="content-section" aria-labelledby="component-content-title"><h2 id="component-content-title" tabindex="-1">Use ', 1)
                route.fulfill(response=response, body=html)
            else:
                route.continue_()
        context.route('**/components/**', overlay)
    page = context.new_page()
    page.add_init_script('''const scrollToContent = Element.prototype.scrollIntoView;
    Element.prototype.scrollIntoView = function(options) {
      if (this.id === 'component-content') window.__contentScroll = options;
      return scrollToContent.call(this, options);
    };''')

    def open_page(query=''):
        page.goto(args.url + '/components/cn/button/' + query, wait_until='domcontentloaded')
        page.wait_for_function('document.fonts.status === "loaded"')
        return page.locator('#content-nudge')

    def screenshot(name):
        image = Image.open(io.BytesIO(page.screenshot(path=str(out / f'{name}.png'))))
        assert image.width > 0

    def at_content():
        page.wait_for_function('Math.abs(document.querySelector("#component-content").getBoundingClientRect().top - 110) < 3')
        page.locator('#content-nudge').wait_for(state='hidden')
        assert page.evaluate('document.activeElement.id') == 'component-content-title'

    nudge = open_page()
    page.wait_for_function('document.querySelector("#live-canvas").dataset.status === "ready"', timeout=90000)
    nudge.wait_for(state='visible')
    assert nudge.get_attribute('href') == '#component-content'
    screenshot('desktop-dark')
    nudge.click()
    at_content()
    assert page.evaluate('window.__contentScroll.behavior') == 'smooth'
    screenshot('desktop-documentation')
    page.evaluate('scrollTo({top:0,behavior:"instant"})')
    nudge.wait_for(state='visible')
    nudge.focus()
    page.keyboard.press('Enter')
    at_content()
    page.evaluate('scrollTo({top:0,behavior:"instant"})')
    page.set_viewport_size({'width': 1440, 'height': 1800})
    nudge.wait_for(state='hidden')
    page.set_viewport_size({'width': 1440, 'height': 1000})
    nudge.wait_for(state='hidden')
    page.get_by_role('tab', name='Code', exact=True).click()
    nudge.wait_for(state='visible')
    page.get_by_role('tab', name='Preview', exact=True).click()
    nudge.wait_for(state='hidden')
    print('PASS desktop scroll cue, focus, keyboard, return scroll and tall viewport', flush=True)

    page.set_viewport_size({'width': 390, 'height': 844})
    nudge = open_page()
    nudge.wait_for(state='visible')
    box = nudge.bounding_box()
    assert box['x'] >= 12 and box['x'] + box['width'] <= 378
    assert box['height'] >= 44
    assert page.evaluate('document.documentElement.scrollWidth <= innerWidth')
    screenshot('mobile-dark')
    page.locator('.scheme-toggle-page').click()
    page.wait_for_function('document.documentElement.dataset.scheme === "light"')
    page.wait_for_timeout(300)
    screenshot('mobile-light')
    nudge.click()
    at_content()
    page.emulate_media(reduced_motion='reduce')
    page.evaluate('scrollTo({top:0,behavior:"instant"})')
    nudge.wait_for(state='visible')
    nudge.click()
    at_content()
    assert page.evaluate('window.__contentScroll.behavior') == 'instant'
    print('PASS mobile layout, both schemes and reduced motion', flush=True)

    for query in ['?standalone=1', '?embed=1']:
        nudge = open_page(query)
        assert nudge.is_hidden(), query
    page.goto(args.url + '/components/', wait_until='domcontentloaded')
    assert page.locator('#content-nudge').count() == 0
    assert not errors, errors
    print('PASS standalone, embedded and index exclusions', flush=True)
    browser.close()
