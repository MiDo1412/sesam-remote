import { createRequire } from 'module'; const { chromium } = createRequire(import.meta.url)('playwright');
const b = await chromium.launch(); const p = await b.newPage({ viewport:{width:1200,height:630} });
await p.goto(new URL('./og.html', import.meta.url).href); await p.waitForTimeout(600);
await p.screenshot({ path: new URL('../website/img/og.png', import.meta.url).pathname }); await b.close();
