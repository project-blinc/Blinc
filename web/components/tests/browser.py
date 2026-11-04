from playwright.sync_api import sync_playwright
from PIL import Image, ImageChops
from pathlib import Path
import argparse, io

parser=argparse.ArgumentParser(description="Verify the real Chrome worker UI using an isolated headless profile.")
parser.add_argument("--url", default="http://127.0.0.1:8094")
parser.add_argument("--dpr", type=int, choices=[1,2], default=2)
args=parser.parse_args()
root=Path(__file__).resolve().parents[3]
out=root/'target'/'component-browser'/f'dpr-{args.dpr}';out.mkdir(parents=True,exist_ok=True)
with sync_playwright() as p:
 browser=p.chromium.launch(channel='chrome',headless=True,args=['--enable-unsafe-webgpu'])
 context=browser.new_context(viewport={'width':1440,'height':1100},device_scale_factor=args.dpr)
 page=context.new_page(); errors=[]
 page.on('pageerror',lambda e: errors.append(str(e)))
 page.add_init_script('''window.__blincEvents=[];window.__blincWorkers=0;window.__blincState={};const NativeWorker=Worker;window.Worker=class extends NativeWorker{constructor(...args){super(...args);window.__blincWorkers++;this.addEventListener('message',({data})=>{if(data.type==='state'||data.type==='ready')window.__blincState=data.payload;});}postMessage(data,...rest){if(data.type==='input')window.__blincEvents.push(data.payload.event);return super.postMessage(data,...rest);}};''')
 def ready():
  page.wait_for_function("document.querySelector('#live-canvas').dataset.status === 'ready'",timeout=90000)
 def canvas():return page.locator('#live-canvas canvas')
 def picture(name):
  raw=canvas().screenshot(path=str(out/f'{name}.png'));return Image.open(io.BytesIO(raw)).convert('RGB')
 def type_and_check(value,expected,name):
  node=canvas();b=node.bounding_box();node.click(position={'x':b['width']/2,'y':b['height']/2})
  page.wait_for_timeout(250);before=picture(name+'-before')
  page.keyboard.type(value,delay=50);page.wait_for_function('(name)=>window.__blincState.name===name',arg=expected)
  page.wait_for_timeout(300);after=picture(name+'-after')
  cx=before.width/2;cy=before.height/2;region=(round(cx-113*args.dpr),round(cy-10*args.dpr),round(cx-22*args.dpr),round(cy+10*args.dpr))
  a=before.crop(region);b=after.crop(region)
  raw_a,raw_b=a.tobytes(),b.tobytes()
  changed=sum(sum(abs(raw_a[j]-raw_b[j]) for j in range(i,i+3))>30 for i in range(0,len(raw_a),3))
  assert changed>50, f'Field pixels did not follow the text callback: {name}, changed={changed}'
  print('PASS input pixels',name,'changed',changed,'state',expected,flush=True)
 page.goto(f'{args.url}/components/cn/input/',wait_until='domcontentloaded');ready()
 page.get_by_role('tab',name='Live',exact=True).click()
 page.wait_for_function("(()=>{const c=document.querySelector('#live-canvas canvas');return c.width===Math.round(c.getBoundingClientRect().width*devicePixelRatio)})()")
 type_and_check('Ada','Ada','input-preview-to-live')
 events=page.evaluate('window.__blincEvents.filter(e=>e.kind===\'text\').map(e=>e.text)')
 assert events==['A','d','a'], events
 page.locator('label:has(#demo-dark)').click();page.wait_for_timeout(300)
 type_and_check('More','AdaMore','input-after-scheme')
 page.evaluate("window.__themeCanvas=document.querySelector('#live-canvas canvas')")
 workers=page.evaluate('window.__blincWorkers')
 for family in ['restrained','expressive','hybrid']:
  page.locator(f'[data-theme-family="{family}"]').click()
  page.wait_for_function('(family)=>window.__blincState.family===family',arg=family)
  assert page.evaluate('window.__blincState.name')=='AdaMore'
  assert page.evaluate('window.__blincWorkers')==workers
  assert page.evaluate("window.__themeCanvas===document.querySelector('#live-canvas canvas')")
 print('PASS theme families retain input values, worker and canvas',flush=True)
 page.locator('label:has(#page-scheme)').click();page.wait_for_timeout(200)
 assert page.evaluate('document.documentElement.dataset.scheme')=='light'
 assert page.locator('#demo-dark').is_checked()==False
 assert page.evaluate("localStorage.getItem('blinc-scheme')")=='light'
 assert page.locator('.brand img').evaluate('(img)=>getComputedStyle(img).filter')=='none'
 page.screenshot(path=str(out/'input-page-light.png'),full_page=True)
 page.evaluate("window.__inputCanvas=document.querySelector('#live-canvas canvas')")
 input_workers=page.evaluate('window.__blincWorkers')
 page.locator('#custom-css').fill('.cn-input { background: var(--surface-elevated); border-color: var(--primary); }')
 page.locator('#apply-css').click()
 page.wait_for_function("document.querySelector('#css-status').textContent==='Applied'")
 assert page.evaluate('window.__blincWorkers')==input_workers
 assert page.evaluate("window.__inputCanvas===document.querySelector('#live-canvas canvas')")
 assert page.evaluate('window.__blincState.name')=='AdaMore'
 type_and_check(' CSS','AdaMore CSS','input-after-css')
 print('PASS input value and visible text survive CSS refresh',flush=True)
 page.goto(f'{args.url}/components/cn/button/?tab=live',wait_until='domcontentloaded');ready()
 assert page.evaluate('document.documentElement.dataset.scheme')=='light'
 page.locator('label:has(#page-scheme)').click();page.wait_for_timeout(400)
 # Find the rendered button in the actual GPU pixels.
 image=picture('button-idle');blue=[]
 for y in range(image.height):
  for x in range(image.width):
   r,g,b=image.getpixel((x,y))
   if b>200 and g>80 and b>r*1.4: blue.append((x,y))
 assert blue,'button pixels missing'
 left=min(x for x,y in blue);right=max(x for x,y in blue);top=min(y for x,y in blue);bottom=max(y for x,y in blue)
 b=canvas().bounding_box();x=(left+right)/2;y=(top+bottom)/2
 page.mouse.move(b['x']+x/args.dpr,b['y']+y/args.dpr);page.wait_for_timeout(500)
 page.mouse.click(b['x']+x/args.dpr,b['y']+y/args.dpr);page.wait_for_function('window.__blincState.clicks===1')
 page.wait_for_timeout(650)
 samples=[]
 for i in range(8):
  page.wait_for_timeout(90);im=picture(f'button-hover-{i}');samples.append(im.getpixel((left+6*args.dpr,round(y))))
 assert all(max(p[c] for p in samples)-min(p[c] for p in samples)<=1 for c in range(3)),f'post-click hover blinks: {samples}'
 print('PASS Chrome post-click hover',samples,flush=True)
 css=page.locator('#custom-css')
 assert '.cn-button--primary' in css.input_value()
 assert 'var(--primary)' in css.input_value()
 assert 'transition:' in css.input_value()
 assert page.locator('#css-highlight .hljs-selector-class').count()>0
 css.focus()
 assert css.evaluate('(e)=>getComputedStyle(e).outlineStyle')=='none'
 page.evaluate("window.__cssCanvas=document.querySelector('#live-canvas canvas')")
 css_workers=page.evaluate('window.__blincWorkers')
 css.fill(':root { --primary: #ee9933; }')
 page.locator('#apply-css').click();ready()
 page.wait_for_function("document.querySelector('#css-status').textContent==='Applied'")
 page.wait_for_timeout(400)
 assert page.evaluate('window.__blincWorkers')==css_workers
 assert page.evaluate("window.__cssCanvas===document.querySelector('#live-canvas canvas')")
 assert page.evaluate('window.__blincState.clicks')==1
 image=picture('button-custom-root')
 pixels=image.tobytes()
 orange=sum(pixels[i]>180 and 90<pixels[i+1]<190 and pixels[i+2]<100 for i in range(0,len(pixels),3))
 assert orange>500, 'root override did not repaint the component'
 css.fill('.cn-button { background:')
 page.locator('#apply-css').click()
 page.wait_for_function("document.querySelector('#css-status').dataset.error==='true'")
 assert page.locator('#live-canvas').get_attribute('data-status')=='ready'
 assert page.evaluate('window.__blincWorkers')==css_workers
 page.locator('#clear-css').click()
 page.wait_for_function("document.querySelector('#css-status').textContent==='Default styles'")
 page.wait_for_timeout(350)
 assert page.evaluate("window.__cssCanvas===document.querySelector('#live-canvas canvas')")
 assert page.evaluate('window.__blincState.clicks')==1
 print('PASS CSS Apply/error/reset retains worker, canvas and values; root override repaints',flush=True)
 page.locator('#apply-css').click()
 page.wait_for_function("document.querySelector('#css-status').textContent==='Applied'")
 page.wait_for_timeout(450)
 im=picture('button-recipe')
 blue=[(x,y) for y in range(im.height) for x in range(im.width) if (lambda c:c[2]>200 and c[1]>80 and c[2]>c[0]*1.4)(im.getpixel((x,y)))]
 assert blue, 'the starter recipe did not render a button'
 left=min(x for x,y in blue);right=max(x for x,y in blue);top=min(y for x,y in blue);bottom=max(y for x,y in blue)
 b=canvas().bounding_box();x=(left+right)/2;y=(top+bottom)/2
 page.mouse.click(b['x']+x/args.dpr,b['y']+y/args.dpr)
 page.wait_for_function('window.__blincState.clicks===2');page.wait_for_timeout(650)
 samples=[]
 for i in range(4):
  page.wait_for_timeout(90);im=picture(f'button-recipe-hover-{i}');samples.append(im.getpixel((left+6*args.dpr,round(y))))
 assert all(max(p[c] for p in samples)-min(p[c] for p in samples)<=1 for c in range(3)),f'starter recipe hover blinks: {samples}'
 assert page.evaluate('window.__blincWorkers')==css_workers
 print('PASS starter recipe padding, shadow, transform and post-click hover',flush=True)
 page.locator('.example').screenshot(path=str(out/'button-editor-dark.png'))
 page.locator('.css-editor').screenshot(path=str(out/'css-editor-dark.png'))
 page.get_by_role('tab',name='Code',exact=True).click()
 assert page.locator('#rust-source .hljs-call').count()>3
 assert page.locator('#rust-source .hljs-type').count()>3
 colour=page.locator('#rust-source .hljs-call').first.evaluate('(e)=>getComputedStyle(e).color')
 assert colour=='rgb(0, 229, 255)',colour
 page.locator('#panel-code').screenshot(path=str(out/'button-code-dark.png'))
 page.goto(f'{args.url}/components/cn/select/?tab=live',wait_until='domcontentloaded');ready()
 node=canvas();b=node.bounding_box();node.click(position={'x':b['width']/2,'y':b['height']/2})
 page.wait_for_timeout(350);picture('select-popup')
 assert not errors,errors
 print('PASS schemes, original logo colours, CSS editor, code tokens; browser errors:',errors,flush=True)
 context.close();browser.close()
