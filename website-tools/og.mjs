import { createRequire } from 'module'; const { chromium } = createRequire(import.meta.url)('playwright');
const b = await chromium.launch(); const p = await b.newPage({ viewport:{width:1200,height:630} });
await p.goto('file://./og.html'); await p.waitForTimeout(500);
await p.screenshot({ path:'/home/user/sesam-remote/website/img/og.png' }); await b.close();
