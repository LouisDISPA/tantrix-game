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

async function clickPlayablePosition(page: Page) {
  const positions = page.locator('.board-empty:not(.faint)');
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

  throw new Error(`None of the ${count} legal board positions can be clicked`);
}
