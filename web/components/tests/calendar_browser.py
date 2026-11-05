"""Read the actual month after uninterrupted Magic Mouse style wheel sequences."""
import argparse, io, shutil, subprocess
from pathlib import Path
from PIL import Image, ImageOps
from playwright.sync_api import sync_playwright

parser=argparse.ArgumentParser();parser.add_argument('--url',default='http://127.0.0.1:8094');parser.add_argument('--dpr',type=int,choices=[1,2],default=2);args=parser.parse_args()
ocr=shutil.which('tesseract')
if not ocr:raise SystemExit('This month-label check requires tesseract on PATH.')
out=Path(__file__).resolve().parents[3]/'target/component-browser/calendar-continuous';out.mkdir(parents=True,exist_ok=True)
with sync_playwright() as p:
    browser=p.chromium.launch(channel='chrome',headless=True,args=['--enable-unsafe-webgpu'])
    page=browser.new_page(viewport={'width':1440,'height':1100},device_scale_factor=args.dpr)
    def shown(name):
        im=Image.open(io.BytesIO(canvas.screenshot(path=str(out/f'{name}.png')))).convert('RGB')
        center=im.width/2;scale=im.width/canvas.bounding_box()['width']
        header=im.crop((int(center-94*scale),int(42*scale),int(center+89*scale),int(80*scale)))
        header=ImageOps.invert(header);header=header.resize((header.width*2,header.height*2))
        filename=out/f'{name}-header.png';header.save(filename)
        return subprocess.check_output([ocr,str(filename),'stdout','--psm','7'],stderr=subprocess.DEVNULL,text=True).strip()
    tail=[3,8,16,25,35,28,20,14,9,6,4,3,2,1]
    short=[1,2,4,8,12,14,12,8,4,2,1]
    ramp=[1,1.25,1.6,2,2.5,3.1,3.8,4.7,5.7,7,8.6,10.5,12.8,15.6,19,23,28,34,28,20,14,9,6,4,3,2,1]
    for name,profile,gap,directions,expected in [
        ('gradual-strokes-during-tail',ramp,0,[1]*4,'March'),
        ('cancelled-stroke-then-gradual-swipe',ramp,0,[1],'December'),
        ('flat-strokes',[20]*8,40,[1]*4,'March'),
        ('momentum-strokes',tail,40,[1]*4,'March'),
        ('new-strokes-during-tail',tail,0,[1]*4,'March'),
        ('short-strokes',short,40,[1]*4,'March'),
        ('direction-reversal',tail,40,[1,1,-1,-1],'November'),
        ('one-stroke-with-long-tail',tail+[1]*25,0,[1],'December'),
    ]:
        page.goto(args.url+'/components/cn/calendar/',wait_until='domcontentloaded')
        page.get_by_role('tab',name='Live',exact=True).click()
        page.locator('#live-canvas').scroll_into_view_if_needed()
        page.wait_for_function('document.querySelector("#live-canvas").dataset.status==="ready"',timeout=90000)
        page.wait_for_timeout(400)
        canvas=page.locator('#live-canvas canvas');b=canvas.bounding_box()
        page.mouse.move(b['x']+b['width']/2,b['y']+b['height']/2)
        assert 'November' in shown(name+'-start')
        b=canvas.bounding_box()
        page.mouse.move(b['x']+b['width']/2,b['y']+b['height']/2)
        # Scale injected wheel deltas to the logical viewport at each DPR.
        # No screenshots, OCR, clicks, pointer exits or long waits between strokes.
        # Dispatch through the real host's DOM listeners with browser timestamps.
        # Keeping timing in the browser avoids automation round trips introducing
        # idle gaps that accidentally reset a cancelled/consumed gesture.
        page.evaluate('''async ({profile,gap,directions,dpr,cancel}) => {
            const canvas=document.querySelector('#live-canvas canvas');
            const b=canvas.getBoundingClientRect();
            const center={clientX:b.x+b.width/2,clientY:b.y+b.height/2};
            const wait=ms=>new Promise(resolve=>setTimeout(resolve,ms));
            const move=position=>canvas.dispatchEvent(new PointerEvent('pointermove',position));
            const wheel=delta=>canvas.dispatchEvent(new WheelEvent('wheel',{
                ...center,deltaX:delta*dpr,deltaY:0,deltaMode:0,cancelable:true,
            }));
            if(cancel){
                for(let i=0;i<3;i++){wheel(10);await wait(16);}
                move({clientX:b.x+4,clientY:b.y+4});
                await wait(16);
                move(center);
            }
            for(const direction of directions){
                for(const delta of profile){wheel(delta*direction);await wait(16);}
                if(gap)await wait(gap);
            }
        }''',{'profile':profile,'gap':gap,'directions':directions,'dpr':args.dpr,'cancel':name.startswith('cancelled-')})
        page.wait_for_timeout(600)
        month=shown(name+'-end');assert expected in month,(name,expected,month)
        print('PASS',name,month,flush=True)
    browser.close()
