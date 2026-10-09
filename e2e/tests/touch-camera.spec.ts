import { expect, test } from '@playwright/test';
import { placedTiles, playMove, startLocalGame } from './play-move';
import { camera, centreOf, touchDrag, touchPinch } from './touch';

/**
 * The board camera answers to fingers: one pans, two pinch, and the lift that
 * ends a drag is no tap on the hex under it. A desktop-sized viewport with a
 * touch screen, so the camera is tested apart from the phone layout.
 */
test.use({ viewport: { width: 1280, height: 720 }, hasTouch: true });

async function playSeeded(page: import('@playwright/test').Page, moves: number) {
  await startLocalGame(page, 2, 3);
  for (let move = 0; move < moves; move++) await playMove(page);
}

test('a finger drags the board and two fingers pinch it', async ({ page }) => {
  await playSeeded(page, 3);
  const stage = centreOf((await page.locator('.board-stage').boundingBox())!);
  const before = await camera(page);

  // A finger dragging right moves the board right: the pan (where the camera
  // looks) goes left by the distance over the zoom.
  await touchDrag(page, { x: stage.x - 100, y: stage.y + 150 }, { x: stage.x + 100, y: stage.y + 150 });
  await expect.poll(async () => (await camera(page)).panX).toBeLessThan(before.panX - 150 / before.zoom);
  const panned = await camera(page);
  expect(panned.zoom).toBe(before.zoom);

  // Two fingers moving apart zoom in, and the page itself does not zoom.
  await touchPinch(page, { x: stage.x, y: stage.y + 150 }, 100, 220);
  await expect.poll(async () => (await camera(page)).zoom).toBeGreaterThan(panned.zoom * 1.5);
  expect(await page.evaluate(() => window.visualViewport?.scale)).toBe(1);

  // And together, out.
  const zoomedIn = (await camera(page)).zoom;
  await touchPinch(page, { x: stage.x, y: stage.y + 150 }, 220, 100);
  await expect.poll(async () => (await camera(page)).zoom).toBeLessThan(zoomedIn * 0.7);
});

test('the lift that ends a drag over a legal position does not play it', async ({ page }) => {
  await playSeeded(page, 4);

  // Select a tile that leaves the player a choice (a tile with a single legal
  // move is played at once).
  const spots = page.locator('.board-empty:not(.faint)');
  for (let attempt = 0; attempt < 12 && (await spots.count()) === 0; attempt++) {
    await page.locator('.player-hand.active .hand-tile:not(.disabled)').first().click();
    await page.waitForTimeout(150);
    if ((await spots.count()) === 0) await playMove(page);
  }
  await expect(spots.first()).toBeAttached();

  // Let the camera settle on the spots (a 0.4s transition), then find one.
  await expect
    .poll(async () => {
      const box = await spots.first().boundingBox();
      if (!box) return false;
      const c = centreOf(box);
      return spots.first().evaluate((el, p) => el.contains(document.elementFromPoint(p.x, p.y)), c);
    })
    .toBe(true);
  const at = centreOf((await spots.first().boundingBox())!);
  const placed = await placedTiles(page).count();
  const before = await camera(page);

  // Press on the hex, drag well past the slop of a tap, and come back to lift
  // the mouse over the same hex: the browser then clicks it.
  await page.mouse.move(at.x, at.y);
  await page.mouse.down();
  await page.mouse.move(at.x + 40, at.y + 30, { steps: 4 });
  await page.mouse.move(at.x, at.y, { steps: 4 });
  await page.mouse.up();

  // Neither played nor sent to the orientation step.
  await expect(placedTiles(page)).toHaveCount(placed);
  await expect(page.locator('.orientation-option')).toHaveCount(0);
  await expect(spots.first()).toBeAttached();
  // The board moved with the drag: nothing was undone by a stray tap.
  expect((await camera(page)).panX).toBeCloseTo(before.panX, 0);

  // A plain click right after still plays.
  await spots.first().click();
  await expect.poll(async () => (await placedTiles(page).count()) + (await page.locator('.orientation-option').count())).toBeGreaterThan(placed);
});
