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
 * The board camera does not follow play, so a greedy strategy grows a long
 * chain that eventually runs past the playfield and every legal position ends
 * up off-screen. A player would zoom out; so does this helper (through the
 * on-screen zoom control). If even the minimum zoom shows nothing, it falls
 * back to activating a position with the keyboard, which needs no hit target.
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
    if (zoomSteps === 6 || !(await zoomOut.first().isEnabled().catch(() => false))) break;
    await zoomOut.first().click();
  }

  const count = await positions.count();
  if (count === 0) throw new Error('There is no legal board position to play');
  await positions.first().focus();
  await page.keyboard.press('Enter');
}
