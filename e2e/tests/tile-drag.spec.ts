import { expect, test, type Page } from '@playwright/test';
import { openApp } from './open-app';
import { openOrientationCard, placedTiles, startLocalGame } from './play-move';
import { camera, centreOf, holdTouch } from './touch';

async function liftTile(page: Page, tile = page.locator('.player-hand.active .hand-tile:not(.disabled)').first()) {
  const origin = centreOf((await tile.boundingBox())!);
  await page.mouse.move(origin.x, origin.y);
  await page.mouse.down();
  await page.mouse.move(origin.x, origin.y - 25, { steps: 3 });
  await expect(page.locator('.dragged-tile')).toBeVisible();
  await expect(tile).toHaveClass(/lifted/);
  return origin;
}

test('a game starts and drags under the production script policy', async ({ page }) => {
  const errors: string[] = [];
  page.on('pageerror', error => errors.push(error.message));
  await page.route('**/*', async route => {
    if (!route.request().isNavigationRequest()) return route.continue();
    const response = await route.fetch();
    await route.fulfill({
      response,
      headers: {
        ...response.headers(),
        'content-security-policy': "script-src 'self' 'unsafe-inline' 'wasm-unsafe-eval'",
      },
    });
  });
  await startLocalGame(page, 2, 3);
  expect(errors).toEqual([]);
  await liftTile(page);
  // Chromium's mouse pointer is 1; capture must belong to the hand tile itself.
  await expect.poll(() => page.locator('.hand-tile.lifted').evaluate(tile => tile.hasPointerCapture(1))).toBe(true);
  const at = centreOf((await page.locator('.board-empty').boundingBox())!);
  await page.mouse.move(at.x, at.y, { steps: 8 });
  await page.mouse.up();
  await expect(placedTiles(page)).toHaveCount(1);
  expect(errors).toEqual([]);
});

test('dragging the opening tile snaps it and plays only on release', async ({ page }, testInfo) => {
  await startLocalGame(page, 2, 3);
  const before = await camera(page);
  await liftTile(page);
  await expect(placedTiles(page)).toHaveCount(0);
  const spot = page.locator('.board-empty[data-position="0,0"]');
  const at = centreOf((await spot.boundingBox())!);
  await page.mouse.move(at.x + 8, at.y + 5, { steps: 8 });
  await expect(page.locator('.dragged-tile.snapped')).toBeVisible();
  await expect(spot).toHaveClass(/drag-target/);
  await expect.poll(async () => {
    const floating = centreOf((await page.locator('.dragged-tile').boundingBox())!);
    return Math.hypot(floating.x - at.x, floating.y - at.y);
  }).toBeLessThan(2);
  expect(await camera(page)).toEqual(before);
  await expect(placedTiles(page)).toHaveCount(0);
  await page.screenshot({ path: testInfo.outputPath('tile-snap.png') });
  await page.mouse.up();
  await expect(placedTiles(page)).toHaveCount(1);
  await expect(page.locator('.dragged-tile')).toHaveCount(0);
  await expect(page.locator('.hud-chip-name')).toHaveText('Blue');
});

test('an invalid drop returns to the rack and a following tap still plays', async ({ page }) => {
  await startLocalGame(page, 2, 3);
  const origin = await liftTile(page);
  await page.mouse.move(origin.x + 45, origin.y, { steps: 4 });
  await page.mouse.up();
  await expect(page.locator('.dragged-tile.returning')).toBeVisible();
  await expect(page.locator('.dragged-tile')).toHaveCount(0);
  await expect(placedTiles(page)).toHaveCount(0);
  await page.locator('.player-hand.active .hand-tile:not(.disabled)').first().click();
  await expect(placedTiles(page)).toHaveCount(1);
});

test('a drop with several orientations opens the picker without playing', async ({ page }) => {
  await startLocalGame(page, 2, 3);
  await openOrientationCard(page);
  const tileId = await page.locator('.hand-tile.selected').getAttribute('data-tile');
  const position = await page.locator('.board-empty.picked').getAttribute('data-position');
  await page.keyboard.press('Escape');
  await page.keyboard.press('Escape');
  const count = await placedTiles(page).count();
  const tile = page.locator(`.player-hand.active .hand-tile[data-tile="${tileId}"]`);
  await liftTile(page, tile);
  const spot = page.locator(`.board-empty[data-position="${position}"]`);
  const at = centreOf((await spot.boundingBox())!);
  await page.mouse.move(at.x, at.y, { steps: 8 });
  await expect(page.locator('.dragged-tile.snapped')).toBeVisible();
  await page.mouse.up();
  await expect(page.locator('.orientation-card')).toBeVisible();
  await expect(placedTiles(page)).toHaveCount(count);
  await expect(page.locator('.tile-preview')).toHaveCount(1);
  await page.getByRole('button', { name: 'Choose rotation' }).first().click();
  await expect(placedTiles(page)).toHaveCount(count + 1);
});

test('Escape cancels a captured mouse drag', async ({ page }) => {
  await startLocalGame(page, 2, 3);
  await liftTile(page);
  await page.keyboard.press('Escape');
  await page.mouse.up();
  await expect(page.locator('.dragged-tile')).toHaveCount(0);
  await expect(placedTiles(page)).toHaveCount(0);
  await expect(page.locator('.hand-tile.selected')).toHaveCount(0);
});

test.describe('phone', () => {
  test.use({ viewport: { width: 390, height: 844 }, hasTouch: true });

  test('a finger drags from the rack to the board without moving the camera', async ({ page }) => {
    await startLocalGame(page, 2, 3);
    const tile = page.locator('.player-hand.active .hand-tile').first();
    const origin = centreOf((await tile.boundingBox())!);
    const before = await camera(page);
    const finger = await holdTouch(page, origin);
    await finger.moveTo({ x: origin.x, y: origin.y - 30 });
    await expect(page.locator('.dragged-tile')).toBeVisible();
    const at = centreOf((await page.locator('.board-empty').boundingBox())!);
    await finger.moveTo(at);
    await expect(page.locator('.dragged-tile.snapped')).toBeVisible();
    expect(await camera(page)).toEqual(before);
    await finger.lift();
    await expect(placedTiles(page)).toHaveCount(1);
    await expect(page.locator('.dragged-tile')).toHaveCount(0);
  });

  test('a cancelled touch and reduced motion leave no floating tile', async ({ page }) => {
    await page.emulateMedia({ reducedMotion: 'reduce' });
    await startLocalGame(page, 2, 3);
    const tile = page.locator('.player-hand.active .hand-tile').first();
    const origin = centreOf((await tile.boundingBox())!);
    const finger = await holdTouch(page, origin);
    await finger.moveTo({ x: origin.x, y: origin.y - 30 });
    await expect(page.locator('.dragged-tile')).toBeVisible();
    await tile.dispatchEvent('pointercancel');
    await finger.lift();
    await expect(page.locator('.dragged-tile')).toHaveCount(0);
    await expect(placedTiles(page)).toHaveCount(0);
    await tile.tap();
    await expect(placedTiles(page)).toHaveCount(1);
  });
});

test('an online drop sends one move and both players see the room result', async ({ page, browser }) => {
  const guestContext = await browser.newContext();
  const guest = await guestContext.newPage();
  try {
    await openApp(page);
    await page.getByRole('button', { name: 'Online' }).click();
    await page.getByRole('button', { name: 'Create a room' }).click();
    await expect(page.getByRole('heading', { name: 'LOBBY' })).toBeVisible();
    const code = new URL(page.url()).pathname.split('/').pop();
    await guest.goto(`/join/${code}`);
    await expect(guest.getByRole('heading', { name: 'LOBBY' })).toBeVisible();
    await page.getByRole('button', { name: /Red.*Open/ }).click();
    await guest.getByRole('button', { name: /Blue.*Open/ }).click();
    await page.getByRole('button', { name: 'Start game' }).click();
    await expect(guest.locator('.hud-chip-name')).toBeVisible();
    const mover = (await page.locator('.hud-chip-name').textContent())?.trim() === 'Red' ? page : guest;
    // The socket is already open: observe the browser's send method directly.
    await mover.evaluate(() => {
      const send = WebSocket.prototype.send;
      (window as typeof window & { dragCommands: number }).dragCommands = 0;
      WebSocket.prototype.send = function (data) {
        if (String(data).includes('PlayMove')) (window as typeof window & { dragCommands: number }).dragCommands++;
        return send.call(this, data);
      };
    });
    await liftTile(mover);
    const at = centreOf((await mover.locator('.board-empty').boundingBox())!);
    await mover.mouse.move(at.x, at.y, { steps: 8 });
    await mover.mouse.up();
    await expect(placedTiles(page)).toHaveCount(1);
    await expect(placedTiles(guest)).toHaveCount(1);
    expect(await mover.evaluate(() => (window as typeof window & { dragCommands: number }).dragCommands)).toBe(1);
  } finally {
    await guestContext.close();
  }
});
