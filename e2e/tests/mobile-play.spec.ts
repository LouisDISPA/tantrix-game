import { expect, test } from '@playwright/test';
import { placedTiles, startLocalGame } from './play-move';
import { camera, centreOf, holdTouch, touchDrag, touchPinch } from './touch';

test.use({ viewport: { width: 390, height: 844 }, isMobile: true, hasTouch: true });

test('a phone can tap a tile, position and rotation, then confirm End Game', async ({ page }) => {
  await startLocalGame(page);
  await page.locator('.player-hand.active .hand-tile[data-tile="RGYRYG"]').tap();
  await expect(placedTiles(page)).toHaveCount(1);
  await page.locator('.player-hand.active .hand-tile[data-tile="RBBYRY"]').tap();
  await page.locator('.board-empty:not(.faint)[data-position="-1,0"]').tap();
  await expect(page.locator('.orientation-card')).toBeVisible();
  await page.locator('.orientation-option[data-rotation="R180"]').tap();
  await expect(placedTiles(page)).toHaveCount(2);
  await expect(page.locator('.hud-chip-name')).toHaveText('Red');

  await page.getByRole('button', { name: 'Menu', exact: true }).tap();
  await page.getByRole('button', { name: 'End Game', exact: true }).tap();
  await page.getByRole('dialog').getByRole('button', { name: 'Cancel' }).tap();
  await expect(page.locator('.game-layout')).toBeVisible();
  await page.getByRole('button', { name: 'Menu', exact: true }).tap();
  await page.getByRole('button', { name: 'End Game', exact: true }).tap();
  await page.getByRole('dialog').getByRole('button', { name: 'Yes, end it' }).tap();
  await expect(page.locator('.results-row')).toHaveCount(2);
});

test('a phone can drag a tile and pan and pinch the board', async ({ page }) => {
  await startLocalGame(page);
  const origin = centreOf((await page.locator('.player-hand.active .hand-tile').first().boundingBox())!);
  const finger = await holdTouch(page, origin);
  await finger.moveTo({ x: origin.x, y: origin.y - 30 });
  await expect(page.locator('.dragged-tile')).toBeVisible();
  const spot = centreOf((await page.locator('.board-empty').boundingBox())!);
  await finger.moveTo(spot);
  await expect(page.locator('.dragged-tile.snapped')).toBeVisible();
  await finger.lift();
  await expect(placedTiles(page)).toHaveCount(1);

  const stage = centreOf((await page.locator('.board-stage').boundingBox())!);
  const before = await camera(page);
  const center = { x: stage.x, y: stage.y + 60 };
  await touchDrag(page, { x: center.x - 60, y: center.y }, { x: center.x + 60, y: center.y });
  await expect.poll(async () => (await camera(page)).panX).toBeLessThan(before.panX);
  await touchPinch(page, center, 80, 160);
  await expect.poll(async () => (await camera(page)).zoom).toBeGreaterThan(before.zoom);
  await expect(placedTiles(page)).toHaveCount(1);
});
