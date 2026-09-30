import { expect, test } from '@playwright/test';
import { openApp } from './open-app';
import { placedTiles, playMove } from './play-move';

const tileCount = 56;
const initiallyDealtTiles = 12;

test('completes a local game and shows the final scores', async ({ page }) => {
  test.setTimeout(5 * 60_000);

  // A fixed deal keeps the game, and so this test, the same on every run.
  await page.goto('/game?seed=1');
  const start = page.getByRole('button', { name: 'Start Game' });
  await expect(start).toHaveAttribute('data-dioxus-id', /\d+/);
  await start.click();

  const playAgain = page.getByRole('button', { name: 'Play Again' });
  const bag = page.locator('.hud-chip-bag');
  for (let move = 0; move < tileCount && !(await playAgain.isVisible()); move++) {
    await test.step(`Place local tile ${move + 1}`, async () => {
      const color = (await page.locator('.hud-chip-name').textContent())?.trim().toLowerCase();
      const hand = page.locator(`.player-hand.color-${color} .hand-tile`);
      const tilesBefore = await hand.evaluateAll((tiles) => tiles.map((t) => t.getAttribute('data-tile')));
      const bagWasEmpty = (await bag.textContent())?.trim() === '0 in bag';
      try {
        await playMove(page);
      } catch (error) {
        if (!(await playAgain.isVisible())) throw error;
        return;
      }
      if (!bagWasEmpty || (await playAgain.isVisible())) return;

      // Without refills the hand shrinks by the one tile played, and every
      // remaining tile still shows itself even though it shifted left.
      await expect(hand).toHaveCount(tilesBefore.length - 1);
      const tilesAfter = await hand.evaluateAll((tiles) => tiles.map((t) => t.getAttribute('data-tile')));
      expect(tilesAfter.every((tile) => tilesBefore.includes(tile))).toBe(true);
    });
  }

  await expect(playAgain).toBeVisible();
  await expect(page.locator('.results-row')).toHaveCount(2);
  await expect(page.locator('.results-winner-label')).toHaveText(/^(Winner|Tie)$/);
  expect(await placedTiles(page).count()).toBeGreaterThanOrEqual(tileCount - initiallyDealtTiles);
  await expect(page.getByText('0 in bag')).toBeVisible();

  await playAgain.click();
  await expect(page.getByText('Set up players')).toBeVisible();
});

test('completes an online game with synchronized boards and final scores', async ({ page, browser }) => {
  test.setTimeout(10 * 60_000);

  await openApp(page);
  await page.getByRole('button', { name: 'Online' }).click();
  await page.getByRole('button', { name: 'Create a room' }).click();
  await expect(page.getByRole('heading', { name: 'LOBBY' })).toBeVisible({ timeout: 20_000 });

  const code = new URL(page.url()).pathname.match(/^\/online\/([A-Z]{6})$/)?.[1] ?? '';
  expect(code).toMatch(/^[A-Z]{6}$/);

  const guestContext = await browser.newContext();
  try {
    const guest = await guestContext.newPage();
    await openApp(guest);
    await guest.getByRole('button', { name: 'Online' }).click();
    await guest.getByLabel('Join code').fill(code);
    await guest.getByRole('button', { name: 'Join room' }).click();
    await expect(guest.getByRole('heading', { name: 'LOBBY' })).toBeVisible({ timeout: 20_000 });

    await page.getByRole('button', { name: /Red.*Open/ }).click();
    await guest.getByRole('button', { name: /Blue.*Open/ }).click();
    const startGame = page.getByRole('button', { name: 'Start game' });
    await expect(startGame).toBeEnabled();
    await startGame.click();
    await expect(page.getByText('Select a tile from your hand')).toBeVisible();
    await expect(guest.getByText('Select a tile from your hand')).toBeVisible();

    const onlineMenu = page.getByRole('button', { name: 'Online Menu' });
    const playedColors = new Set<string>();
    for (let move = 0; move < tileCount && !(await onlineMenu.isVisible()); move++) {
      await test.step(`Place online tile ${move + 1}`, async () => {
        const color = (await page.locator('.hud-chip-name').textContent())?.trim() ?? '';
        expect(color).toMatch(/^(Red|Blue)$/);
        const activePage = color === 'Red' ? page : guest;
        let count: number;
        try {
          count = await playMove(activePage);
        } catch (error) {
          if (await onlineMenu.isVisible()) return;
          throw error;
        }
        await expect(placedTiles(page)).toHaveCount(count);
        await expect(placedTiles(guest)).toHaveCount(count);
        playedColors.add(color);
      });
    }

    await expect(onlineMenu).toBeVisible();
    await expect(guest.getByRole('button', { name: 'Online Menu' })).toBeVisible();
    await expect(page.locator('.results-row')).toHaveCount(2);
    await expect(guest.locator('.results-row')).toHaveCount(2);
    const finalCount = await placedTiles(page).count();
    expect(finalCount).toBeGreaterThanOrEqual(tileCount - initiallyDealtTiles);
    expect(playedColors).toEqual(new Set(['Red', 'Blue']));
    await expect(placedTiles(guest)).toHaveCount(finalCount);
    const scores = await page.locator('.results-row').allTextContents();
    await expect(guest.locator('.results-row')).toHaveText(scores);
    await expect(guest.locator('.results-winner-label')).toHaveText(
      (await page.locator('.results-winner-label').textContent()) ?? '',
    );
    await expect(page.getByText('0 in bag')).toBeVisible();
    await expect(guest.getByText('0 in bag')).toBeVisible();
  } finally {
    await guestContext.close();
  }
});
