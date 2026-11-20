// iPhone-Ansicht im Browser: node phone.mjs <name> <light|dark> [act.mjs] [w h]
import { createRequire } from 'module'; const { chromium } = createRequire(import.meta.url)('playwright');
const [,, name, scheme='light', act='', w='402', h='874'] = process.argv;
const b = await chromium.launch();
const ctx = await b.newContext({ viewport:{width:+w,height:+h}, deviceScaleFactor:2, colorScheme:scheme, locale:'de-CH', hasTouch:true, isMobile:true,
  userAgent:'Mozilla/5.0 (iPhone; CPU iPhone OS 26_0 like Mac OS X) AppleWebKit/605.1.15 (KHTML, like Gecko) Mobile/15E148' });
const p = await ctx.newPage();
p.on('pageerror', e => console.log('PAGEERR', e.message));
await p.addInitScript(() => { Object.defineProperty(Navigator.prototype, 'maxTouchPoints', { get: () => 5 }); window.__deskW = 1179; window.__deskH = 2556; });
for (const f of ['./mock.js','./desk.js','./shell.js']) await p.addInitScript({ path: f });
await p.goto('http://localhost:1420/'); await p.waitForTimeout(1500);
if (act) { const f = (await import(act)).default; await f(p); }
await p.screenshot({ path: `${process.env.OUT || '.'}/${name}.png` });
await b.close();
