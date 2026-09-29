import { expect, test } from '@playwright/test';
import { openApp } from './open-app';
import { placedTiles, playMove } from './play-move';

const tileCount = 56;
const initiallyDealtTiles = 12;

test('completes a local game and shows the final scores', async ({ page }) => {
  test.setTimeout(5 * 60_000);

  await openApp(page);
  await page.getByRole('button', { name: 'Local Game' }).click();
  await page.getByRole('button', { name: 'Start Game' }).click();

  const playAgain = page.getByRole('button', { name: 'Play Again' });
  for (let move = 0; move < tileCount && !(await playAgain.isVisible()); move++) {
    await test.step(`Place local tile ${move + 1}`, async () => {
      try {
        await playMove(page);
      } catch (error) {
        if (!(await playAgain.isVisible())) throw error;
      }
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
  await page.getByRole('button', { name: 'Host a new room' }).click();
  await expect(page.getByRole('heading', { name: 'LOBBY' })).toBeVisible({ timeout: 20_000 });

  const code = new URL(page.url()).pathname.match(/^\/online\/([A-Z]{6})\/host$/)?.[1] ?? '';
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
