import { expect, test, type Page } from '@playwright/test';
import { placedTiles, playMove, startLocalGame, tapOrientationCard } from './play-move';
import { centreOf, holdTouch } from './touch';

/**
 * Playing on a phone with real touch input (taps, and presses through the
 * DevTools protocol): every other player's tiles are one tap away, a move is
 * a tile, a spot and — when the tile fits the spot two ways — one of two
 * options, and End Game, in the menu, asks first.
 */
test.use({ viewport: { width: 390, height: 844 }, isMobile: true, hasTouch: true, deviceScaleFactor: 2 });

/** The same dealt game every time, so what the tests look for is always there. */
const SEED = 3;

async function openGame(page: Page, players: number) {
  await startLocalGame(page, players, SEED);
  // A few moves in, so spots have neighbours and the card has tiles to avoid.
  for (let move = 0; move < 4; move++) await playMove(page);
}

test('a tap on another tab shows that player\'s tiles, which are not for playing', async ({ page }) => {
  await openGame(page, 3);
  const tabs = page.locator('.hand-header');
  await expect(tabs).toHaveCount(3);
  await expect(page.locator('.player-hand.shown')).toHaveClass(/active/);

  const other = page.locator('.player-hand:not(.active)').first();
  await other.locator('.hand-header').tap();
  await expect(other).toHaveClass(/shown/);
  await expect(page.locator('.player-hand.shown')).toHaveCount(1);

  const placed = await placedTiles(page).count();
  await other.locator('.hand-tile').first().tap({ force: true });
  await expect(placedTiles(page)).toHaveCount(placed);
  await expect(page.locator('.board-empty:not(.faint)')).toHaveCount(0);

  // And back to the mover's.
  await page.locator('.player-hand.active .hand-header').tap();
  await expect(page.locator('.player-hand.active')).toHaveClass(/shown/);
});

test('a spot that takes two orientations opens a card, a press previews and a tap plays', async ({ page }) => {
  await openGame(page, 2);
  const card = await tapOrientationCard(page);
  // The mouse that played the first moves is still somewhere over the board; a phone has none.
  await page.mouse.move(1, 1);
  const options = card.getByRole('button', { name: 'Choose rotation' });
  await expect(options).toHaveCount(2);

  // A finger pressed on an option shows it on the board; sliding off and lifting plays nothing.
  const stage = (await page.locator('.board-stage').boundingBox())!;
  const ghost = page.locator('.tile-preview');
  const placed = await placedTiles(page).count();
  await expect(ghost).toHaveCount(0);
  const finger = await holdTouch(page, centreOf((await options.nth(1).boundingBox())!));
  await expect(ghost).toHaveCount(1);
  await finger.moveTo({ x: stage.x + 4, y: stage.y + 4 });
  await finger.lift();
  await expect(ghost).toHaveCount(0);
  await expect(placedTiles(page)).toHaveCount(placed);
  await expect(card).toHaveCount(1);

  // A tap on an option plays it: the move took a tile, a spot and a tap.
  await options.nth(1).tap();
  await expect(placedTiles(page)).toHaveCount(placed + 1);
  await expect(card).toHaveCount(0);
});

test('the menu holds End Game, which asks first', async ({ page }) => {
  await openGame(page, 2);
  await expect(page.locator('.end-game-btn')).toBeHidden();
  await page.getByRole('button', { name: 'Menu' }).tap();
  await expect(page.locator('.end-game-btn')).toBeVisible();
  await page.locator('.end-game-btn').tap();
  const dialog = page.getByRole('dialog');
  await expect(dialog).toBeVisible();
  await dialog.getByRole('button', { name: 'Cancel' }).tap();
  await expect(dialog).toHaveCount(0);
  await expect(page.locator('.game-layout')).toBeVisible();

  // Confirmed, it ends the game.
  await page.getByRole('button', { name: 'Menu' }).tap();
  await page.locator('.end-game-btn').tap();
  await page.getByRole('dialog').getByRole('button', { name: 'Yes, end it' }).tap();
  await expect(page.locator('.results-row')).toHaveCount(2);
});
