import { mkdirSync } from 'node:fs';
import path from 'node:path';
import { test } from '@playwright/test';
import { fixture, playFixtureMove, startLocalGame } from './play-move';

// Manual screenshots, excluded from normal runs.
test.skip(!process.env.TANTRIX_SCREENSHOTS, 'set TANTRIX_SCREENSHOTS=1 to take screenshots');
const out = path.join(__dirname, '..', 'screenshots');
for (const viewport of [{ name: 'phone', width: 390, height: 844 }, { name: 'desktop', width: 1280, height: 720 }]) {
  test(`screenshot at ${viewport.name} size`, async ({ page }) => {
    mkdirSync(out, { recursive: true });
    await page.setViewportSize(viewport);
    await page.emulateMedia({ reducedMotion: 'reduce' });
    await startLocalGame(page);
    for (const move of fixture.moves.slice(0, 2)) await playFixtureMove(page, move);
    await page.screenshot({ path: path.join(out, `${viewport.name}.png`), animations: 'disabled' });
  });
}
