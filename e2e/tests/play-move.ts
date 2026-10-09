import { expect, type Page } from '@playwright/test';

export function placedTiles(page: Page) {
  return page.locator('.board-tile:not(.tile-preview)');
}

/** Play the first legal move offered by the current player's hand. */
export async function playMove(page: Page) {
  const placed = placedTiles(page);
  const previousCount = await placed.count();
  const tile = page.locator('.player-hand.active .hand-tile:not(.disabled)').first();
  const positions = page.locator('.board-empty:not(.faint)');
  const rotation = page.getByRole('button', { name: 'Choose rotation' }).first();

  await expect(tile).toBeVisible();
  await tile.click();

  await expect
    .poll(async () =>
      (await placed.count()) > previousCount || (await positions.count()) > 0,
    )
    .toBe(true);

  if ((await placed.count()) === previousCount) {
    await clickPlayablePosition(page);
    await expect
      .poll(async () =>
        (await placed.count()) > previousCount || (await rotation.count()) > 0,
      )
      .toBe(true);

    if ((await placed.count()) === previousCount) {
      await rotation.click();
    }
  }

  await expect(placed).toHaveCount(previousCount + 1);
  return previousCount + 1;
}

/**
 * Click the first legal position that is actually on screen and not covered.
 *
 * The camera frames the legal positions when a tile is picked, so they are
 * normally all on screen. If one is covered anyway the helper zooms out (through
 * the on-screen zoom control, where it is visible) and, as a last resort, activates
 * a position with the keyboard, which needs no hit target.
 */
async function clickPlayablePosition(page: Page) {
  const positions = page.locator('.board-empty:not(.faint)');
  const zoomOut = page.getByRole('button', { name: /^Zoom out$|^−$/ }).or(page.getByTitle('Zoom out'));

  for (let zoomSteps = 0; zoomSteps <= 6; zoomSteps++) {
    const count = await positions.count();
    for (let index = 0; index < count; index++) {
      const position = positions.nth(index);
      try {
        await position.click({ trial: true, timeout: 750 });
      } catch {
        continue;
      }
      await position.click();
      return;
    }
    if (zoomSteps === 6 || !(await zoomOut.first().isVisible().catch(() => false))) break;
    await zoomOut.first().click();
  }

  const count = await positions.count();
  if (count === 0) throw new Error('There is no legal board position to play');
  await positions.first().focus();
  await page.keyboard.press('Enter');
}

/**
 * Play on until a tile is picked, then a spot, that leaves the player a choice
 * of orientation, so the card of options is open at the spot. A tile or a spot
 * with a single legal placement is played at once, and the turn goes on.
 */
export async function openOrientationCard(page: Page, maxMoves = 40) {
  const card = page.locator('.orientation-card');
  const spots = page.locator('.board-empty:not(.faint)');

  for (let move = 0; move < maxMoves; move++) {
    const placed = await placedTiles(page).count();
    const tiles = page.locator('.player-hand.active .hand-tile:not(.disabled)');
    const count = await tiles.count();
    for (let tile = 0; tile < count; tile++) {
      await tiles.nth(tile).click();
      await page.waitForTimeout(100);
      if ((await placedTiles(page).count()) > placed) break; // played at once
      const hexes = await spots.count();
      for (let hex = 0; hex < hexes; hex++) {
        // The keyboard reaches a hex wherever the camera has left it.
        await spots.nth(hex).press('Enter');
        await page.waitForTimeout(100);
        if ((await card.count()) > 0) return card;
        if ((await placedTiles(page).count()) > placed) break;
      }
      if ((await placedTiles(page).count()) > placed) break;
    }
    if ((await placedTiles(page).count()) === placed) await playMove(page);
  }
  throw new Error('no tile in the game left a choice of orientation');
}

/**
 * Seats `players` humans at the setup screen (Red and Blue play by default) and
 * starts. With a `seed` the game is dealt the same every time, and `playMove`
 * and friends (which always take the first choice) play it out the same way.
 */
export async function startLocalGame(page: Page, players = 2, seed?: number) {
  if (seed === undefined) {
    await page.getByRole('button', { name: 'Local Game' }).click();
  } else {
    await page.goto(`/game?seed=${seed}`);
    // Server-rendered controls appear before their client-side handlers are attached.
    await expect(page.getByRole('button', { name: 'Start Game' })).toHaveAttribute('data-dioxus-id', /\d+/);
  }
  for (const color of ['Yellow', 'Green'].slice(0, Math.max(0, players - 2))) {
    const seat = page.locator('.player-slot', { hasText: color });
    await seat.locator('.player-toggle').click();
    await expect(seat.locator('.player-toggle')).toHaveText('Player');
  }
  await page.getByRole('button', { name: 'Start Game' }).click();
  await expect(page.locator('.game-layout')).toBeVisible();
}

/**
 * Like `openOrientationCard`, with a finger: tap a tile, then its spots one by
 * one until a spot that takes two orientations opens the card.
 */
export async function tapOrientationCard(page: Page, maxMoves = 40) {
  const card = page.locator('.orientation-card');
  const spots = page.locator('.board-empty:not(.faint)');

  for (let move = 0; move < maxMoves; move++) {
    const placed = await placedTiles(page).count();
    const tiles = page.locator('.player-hand.active .hand-tile:not(.disabled)');
    const count = await tiles.count();
    for (let tile = 0; tile < count; tile++) {
      await tiles.nth(tile).tap();
      await page.waitForTimeout(150);
      if ((await placedTiles(page).count()) > placed) break; // played at once
      const hexes = await spots.count();
      for (let hex = 0; hex < hexes; hex++) {
        await spots.nth(hex).tap();
        await page.waitForTimeout(150);
        if ((await card.count()) > 0) return card;
        if ((await placedTiles(page).count()) > placed) break;
      }
      if ((await placedTiles(page).count()) > placed) break;
    }
    if ((await placedTiles(page).count()) === placed) await playMove(page);
  }
  throw new Error('no tile in the game left a choice of orientation');
}
