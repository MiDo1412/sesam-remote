export default async (p) => { await p.locator('.card', { hasText: 'Studio-Mac' }).click(); await p.waitForTimeout(800); };
