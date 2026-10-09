import { expect, test, type Page } from '@playwright/test';
import { openOrientationCard, placedTiles, playMove, startLocalGame } from './play-move';

/**
 * A spot where the tile fits two ways opens a card of the two options at the
 * spot; pressing one previews it on the board, lifting plays it. (Where the
 * card sits is covered by the sweep in `svg/card_placement.rs`.)
 */
test.use({ viewport: { width: 1280, height: 720 } });

async function startWithCard(page: Page) {
  // A dealt game that is the same every time, played out the same way.
  await startLocalGame(page, 2, 3);
  // A few moves in, so spots have neighbours and the card has tiles to avoid.
  for (let move = 0; move < 4; move++) await playMove(page);
  return openOrientationCard(page);
}

test('pressing an option previews it on the board and lifting elsewhere plays nothing', async ({ page }) => {
  const card = await startWithCard(page);
  const options = card.getByRole('button', { name: 'Choose rotation' });
  const ghost = page.locator('.tile-preview');
  const placed = await placedTiles(page).count();

  // Nothing is previewed until an option is pointed at.
  await expect(ghost).toHaveCount(0);

  const o = (await options.nth(1).boundingBox())!;
  await page.mouse.move(o.x + o.width / 2, o.y + o.height / 2);
  await page.mouse.down();
  await expect(ghost).toHaveCount(1);
  const pressed = await ghost.innerHTML();

  // Slide to the other option: the ghost turns.
  const other = (await options.nth(0).boundingBox())!;
  await page.mouse.move(other.x + other.width / 2, other.y + other.height / 2, { steps: 4 });
  await expect(ghost).toHaveCount(1);
  expect(await ghost.innerHTML()).not.toBe(pressed);

  // Lift away from both: no click, no ghost, the card stays.
  await page.mouse.move(o.x + o.width / 2, o.y - 200, { steps: 4 });
  await page.mouse.up();
  await expect(ghost).toHaveCount(0);
  await expect(placedTiles(page)).toHaveCount(placed);
  await expect(card).toHaveCount(1);
});

test('tapping an option plays the tile that way', async ({ page }) => {
  const card = await startWithCard(page);
  const placed = await placedTiles(page).count();
  const at = await page.locator('.board-empty.picked').getAttribute('data-position');

  await card.getByRole('button', { name: 'Choose rotation' }).nth(1).click();

  await expect(placedTiles(page)).toHaveCount(placed + 1);
  await expect(page.locator(`.board-tile:not(.tile-preview)[data-position="${at}"]`)).toBeVisible();
  await expect(page.locator('.orientation-card')).toHaveCount(0);
});

test('tapping the empty board closes the card, back to choosing a spot', async ({ page }) => {
  const card = await startWithCard(page);
  const stage = (await page.locator('.board-stage').boundingBox())!;

  // The corner of the stage, far from any tile or spot.
  await page.mouse.click(stage.x + 6, stage.y + stage.height - 6);

  await expect(card).toHaveCount(0);
  await expect(page.locator('.board-empty.picked')).toHaveCount(0);
  await expect(page.locator('.board-empty:not(.faint)').first()).toBeAttached();
});
