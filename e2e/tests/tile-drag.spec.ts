import { expect, test } from '@playwright/test';
import { placedTiles, startLocalGame } from './play-move';
import { centreOf } from './touch';

test('desktop drag places only on release under the production script policy', async ({ page }) => {
  const errors: string[] = [];
  page.on('pageerror', error => errors.push(error.message));
  await page.route('**/*', async route => {
    if (!route.request().isNavigationRequest()) return route.continue();
    const response = await route.fetch();
    await route.fulfill({ response, headers: {
      ...response.headers(),
      'content-security-policy': "script-src 'self' 'unsafe-inline' 'wasm-unsafe-eval'",
    } });
  });
  await startLocalGame(page);
  const tile = page.locator('.player-hand.active .hand-tile').first();
  const origin = centreOf((await tile.boundingBox())!);

  // Dropping back by the rack leaves the board alone.
  await page.mouse.move(origin.x, origin.y);
  await page.mouse.down();
  await page.mouse.move(origin.x + 45, origin.y, { steps: 4 });
  await expect(page.locator('.dragged-tile')).toBeVisible();
  await page.mouse.up();
  await expect(page.locator('.dragged-tile')).toHaveCount(0);
  await expect(placedTiles(page)).toHaveCount(0);

  await page.mouse.move(origin.x, origin.y);
  await page.mouse.down();
  await page.mouse.move(origin.x, origin.y - 25, { steps: 3 });
  await expect(page.locator('.dragged-tile')).toBeVisible();
  const spot = centreOf((await page.locator('.board-empty[data-position="0,0"]').boundingBox())!);
  await page.mouse.move(spot.x, spot.y, { steps: 8 });
  await expect(page.locator('.dragged-tile.snapped')).toBeVisible();
  await expect(placedTiles(page)).toHaveCount(0);
  await page.mouse.up();
  await expect(placedTiles(page)).toHaveCount(1);
  await expect(page.locator('.dragged-tile')).toHaveCount(0);
  expect(errors).toEqual([]);
});
