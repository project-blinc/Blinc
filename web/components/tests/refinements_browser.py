"""Check gallery refinements using actual worker-rendered Chrome canvases."""
import argparse, io, json
from pathlib import Path
from PIL import Image, ImageChops
from playwright.sync_api import sync_playwright

parser=argparse.ArgumentParser(); parser.add_argument('--url',default='http://127.0.0.1:8094'); args=parser.parse_args()
root=Path(__file__).resolve().parents[3]
out=root/'target/component-browser/refinements';out.mkdir(parents=True,exist_ok=True)
catalog=json.loads((root/'web/components/catalog.json').read_text())
with sync_playwright() as p:
    browser=p.chromium.launch(channel='chrome',headless=True,args=['--enable-unsafe-webgpu'])
    context=browser.new_context(viewport={'width':1440,'height':1100},device_scale_factor=1)
    page=context.new_page(); errors=[];page.on('pageerror',lambda error:errors.append(str(error)))
    page.add_init_script('''window.__workers=0;window.__state={};const OriginalWorker=Worker;
    window.Worker=class extends OriginalWorker {constructor(...args){super(...args);window.__workers++;
    this.addEventListener('message',({data})=>{if(['state','ready'].includes(data.type))window.__state=data.payload;});}};''')
    def open_demo(slug,library='cn'):
        page.goto(f'{args.url}/components/{library}/{slug}/',wait_until='domcontentloaded')
        page.locator('.example').scroll_into_view_if_needed()
        page.get_by_role('tab',name='Live',exact=True).first.click()
        page.locator('#live-canvas').scroll_into_view_if_needed()
        try:
            page.wait_for_function('document.querySelector("#live-canvas").dataset.status==="ready"',timeout=90000)
        except Exception as error:
            detail=page.locator('#live-status').inner_text()
            raise AssertionError(f'{slug}: worker did not become ready: {detail}') from error
        page.wait_for_timeout(250)
        return page.locator('#live-canvas canvas')
    def shot(slug,canvas):
        return Image.open(io.BytesIO(canvas.screenshot(path=str(out/f'{slug}.png')))).convert('RGB')
    def changed(a,b):
        return sum(1 for r,g,b in ImageChops.difference(a,b).get_flattened_data() if r+g+b>45)

    page.goto(args.url+'/components/')
    links=page.locator('.sidebar a[href*="/components/cn/"], .sidebar a[href*="/components/layout/"]').evaluate_all('(els)=>els.map(e=>e.getAttribute("href").split("/").filter(Boolean).at(-1))')
    assert links==[c['slug'] for c in catalog['components']]
    assert links[links.index('select'):links.index('select')+3]==['select','combobox','dropdown-menu']
    page.locator('.sidebar').evaluate('(el)=>el.scrollTop=el.scrollHeight-el.clientHeight-80')
    href=page.locator('.sidebar').evaluate('''el=>[...el.querySelectorAll('a')].find(a=>{
      const r=a.getBoundingClientRect(),n=el.getBoundingClientRect();return r.top>n.top+120&&r.bottom<n.bottom-40;}).getAttribute('href')''')
    before=page.locator('.sidebar').evaluate('(el)=>el.scrollTop')
    page.locator(f'.sidebar a[href="{href}"]').click()
    assert abs(page.locator('.sidebar').evaluate('(el)=>el.scrollTop')-before)<=1
    print('PASS ordering and navigation scroll retention',flush=True)

    canvas=open_demo('button')
    page.locator('#custom-css').evaluate('''el=>{el.value='/* '+ 'very_long_css_token_'.repeat(35)+' */\\n.cn-button { color: var(--primary); }';el.dispatchEvent(new Event('input',{bubbles:true}));}''')
    metrics=page.evaluate('''(()=>{const t=document.querySelector('#custom-css'),p=document.querySelector('#css-highlight').parentElement;
      return {tw:t.scrollWidth,tc:t.clientWidth,pw:p.scrollWidth,pc:p.clientWidth,th:t.scrollHeight,ph:p.scrollHeight};})()''')
    assert metrics['tw']<=metrics['tc']+1 and metrics['pw']<=metrics['pc']+1,metrics
    assert abs(metrics['th']-metrics['ph'])<25,metrics
    page.locator('.css-editor').screenshot(path=str(out/'css-wrap.png'))
    assert page.evaluate('window.__workers')==1
    print('PASS wrapped editor and syntax mirror',metrics,flush=True)
    for slug in ['card','badge','aspect-ratio','kbd','icons','tabs','sidebar','sheet','drawer']:
        canvas=open_demo(slug)
        if slug in ['sheet','drawer']:
            box=canvas.bounding_box();page.mouse.click(box['x']+box['width']/2,box['y']+box['height']/2);page.wait_for_timeout(650)
        shot(slug,canvas)
    canvas=open_demo('canvas','layout')
    assert not page.locator('.css-editor').is_visible()
    widths=page.locator('.live-workbench').evaluate('(e)=>[e.clientWidth,e.querySelector(".live-viewport").clientWidth]')
    assert abs(widths[0]-widths[1])<3,widths
    shot('canvas',canvas)
    print('PASS full-width Canvas without CSS editor',flush=True)

    canvas=open_demo('context-menu')
    assert canvas.evaluate('(c)=>!c.dispatchEvent(new MouseEvent("contextmenu",{bubbles:true,cancelable:true}))')
    initial=shot('context-before',canvas)
    box=canvas.bounding_box();page.mouse.click(box['x']+box['width']/2,box['y']+box['height']/2,button='right');page.wait_for_timeout(400)
    assert changed(initial,shot('context-open',canvas))>1000
    print('PASS browser context default prevented and Blinc menu opens',flush=True)

    canvas=open_demo('calendar');box=canvas.bounding_box()
    page.mouse.move(box['x']+box['width']/2,box['y']+box['height']/2)
    previous=shot('calendar-start',canvas)
    for i in range(4):
        page.mouse.wheel(200,0);page.wait_for_timeout(500)
        current=shot(f'calendar-wheel-{i}',canvas);assert changed(previous,current)>100, f'wheel swipe {i} did not change the calendar'
        previous=current
    for i in range(3):
        x=box['x']+box['width']/2;y=box['y']+box['height']/2
        page.mouse.move(x+65,y);page.mouse.down();page.mouse.move(x-65,y,steps=12);page.mouse.up();page.wait_for_timeout(600)
        current=shot(f'calendar-drag-{i}',canvas);assert changed(previous,current)>100, f'drag swipe {i} did not change the calendar'
        previous=current
    print('PASS repeated Calendar wheel and drag swipes without outside clicks',flush=True)

    for slug,key in [('video-player','video'),('audio-player','audio')]:
        canvas=open_demo(slug,'layout')
        page.wait_for_function('(key)=>window.__state[key]?.duration>10000',arg=key,timeout=20000)
        ranges=page.evaluate('''async()=>{const r=await fetch(document.querySelector('video').currentSrc,{headers:{Range:'bytes=0-1023'}});return {status:r.status,accept:r.headers.get('accept-ranges'),length:r.headers.get('content-length')}}''')
        assert ranges=={'status':206,'accept':'bytes','length':'1024'},ranges
        if key=='video':page.wait_for_function('window.__state.video.frame>0',timeout=20000)
        box=canvas.bounding_box();width=min(420,box['width']-48)
        x=box['x']+(box['width']-width)/2+20
        y=box['y']+(box['height']-240)/2+220 if key=='video' else box['y']+box['height']/2+16
        page.mouse.click(x,y)
        page.wait_for_function('(key)=>window.__state[key]?.playing&&window.__state[key]?.position>250',arg=key,timeout=10000)
        a=shot(slug+'-playing',canvas);page.wait_for_timeout(600);b=shot(slug+'-advanced',canvas)
        if key=='video':assert changed(a,b)>1000,'decoded video did not advance'
        page.mouse.click(x,y);page.wait_for_function('(key)=>!window.__state[key]?.playing',arg=key,timeout=10000)
        paused=page.evaluate('(key)=>window.__state[key].position',key)
        page.wait_for_timeout(500)
        assert abs(page.evaluate('(key)=>window.__state[key].position',key)-paused)<100
        page.mouse.click(box['x']+(box['width']-width)/2+width*0.55,y)
        page.wait_for_function('(key)=>window.__state[key].position>window.__state[key].duration*0.25',arg=key,timeout=10000)
        page.wait_for_function('document.querySelector("video").currentTime>document.querySelector("video").duration*0.25',timeout=10000)
        page.mouse.click(box['x']+(box['width']-width)/2+width-31,y)
        page.wait_for_function('document.querySelector("video").volume>0.05&&document.querySelector("video").volume<0.9',timeout=10000)
        assert page.evaluate('window.__workers')==1
        page.get_by_role('button',name='Expressive',exact=True).click();page.wait_for_timeout(400)
        assert page.evaluate('window.__workers')==1 and page.evaluate('(key)=>window.__state[key].duration',key)>10000
        page.mouse.click(x,y);page.wait_for_function('!document.querySelector("video").paused',timeout=10000)
        page.get_by_role('tab',name='Preview',exact=True).first.click()
        page.wait_for_function('document.querySelector("video").paused',timeout=10000)
        print('PASS real browser playback, pause, seek, volume, visibility and theme retention',slug,flush=True)
    page.set_viewport_size({'width':390,'height':1000})
    for library,slug in [('cn','aspect-ratio'),('layout','svg'),('layout','notch'),('cn','sheet'),('cn','drawer'),('cn','card')]:
        print('CHECK mobile',slug,flush=True)
        canvas=open_demo(slug,library)
        if slug in ['sheet','drawer']:
            box=canvas.bounding_box();page.mouse.click(box['x']+box['width']/2,box['y']+box['height']/2);page.wait_for_timeout(500)
        elif slug=='notch':
            box=canvas.bounding_box();page.mouse.move(box['x']+(box['width']-224)/2+16,box['y']+22);page.wait_for_timeout(600)
        shot(slug+'-mobile',canvas)
    print('PASS mobile render checks',flush=True)
    assert not errors,errors
    context.close();browser.close()
