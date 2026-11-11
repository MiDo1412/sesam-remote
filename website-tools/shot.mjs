import { createRequire } from 'module'; const { chromium } = createRequire(import.meta.url)('playwright');
const [,, name, scheme='light', ua='mac', w='1280', h='800', extra=''] = process.argv;
const b = await chromium.launch();
const UA = ua==='ipad' ? 'Mozilla/5.0 (iPad; CPU OS 18_0 like Mac OS X) AppleWebKit/605.1.15 (KHTML, like Gecko) Mobile/15E148' : ua==='mac' ? 'Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/605.1.15 (KHTML, like Gecko)' : 'Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/130 Safari/537.36 Edg/130';
const ctx = await b.newContext({ viewport:{width:+w,height:+h}, deviceScaleFactor:2, colorScheme:scheme, userAgent:UA, hasTouch: ua==='ipad' });
const p = await ctx.newPage();
p.on('console', m => m.type()==='error' && console.log('ERR', m.text()));
p.on('pageerror', e => console.log('PAGEERR', e.message));
if (ua==='ipad') await p.addInitScript(() => { Object.defineProperty(Navigator.prototype, 'maxTouchPoints', { get: () => 5 }); window.__deskW = 2360; window.__deskH = 1560; });
await p.addInitScript({ path: './mock.js' });
if (extra) for (const e of extra.split(',')) await p.addInitScript({ path: e });
await p.addInitScript(() => addEventListener('DOMContentLoaded', () => { const s = document.createElement('style'); s.textContent = '.win-controls{display:none!important}'; document.head.append(s); }));

await p.goto('http://localhost:1420/');
await p.waitForTimeout(1500);
if (process.env.ACT) { const f = (await import(process.env.ACT)).default; await f(p); }
await p.screenshot({ path: `./${name}.png` });
console.log(await p.evaluate(()=>[...new Set(window.__calls)].join(',')));
await b.close();
