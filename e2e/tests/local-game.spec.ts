import { expect, test } from '@playwright/test';
import { openApp } from './open-app';
import { placedTiles, playMove } from './play-move';

test('starts a local game, advances the turn, and can play again after ending', async ({ page }) => {
  await openApp(page);
  await page.getByRole('button', { name: 'Local Game' }).click();
  await page.getByRole('button', { name: 'Start Game' }).click();
  const current = page.locator('.hud-chip-name');
  const first = (await current.textContent())?.trim() ?? '';
  expect(first).toMatch(/^(Red|Blue)$/);
  await playMove(page);
  await expect(current).not.toHaveText(first);
  await expect(page.getByText('43 in bag')).toBeVisible();

  await page.getByRole('button', { name: 'End Game', exact: true }).click();
  await page.getByRole('dialog').getByRole('button', { name: 'Yes, end it' }).click();
  await expect(page.locator('.results-row')).toHaveCount(2);
  await page.getByRole('button', { name: 'Play Again' }).click();
  await expect(page.getByText('Set up players')).toBeVisible();
});

test('an Expert AI plays a turn in the browser', async ({ page }) => {
  await openApp(page, '/game?seed=1');
  const blue = page.locator('.player-slot', { hasText: 'Blue' });
  await blue.getByRole('button', { name: 'Player' }).click();
  await blue.getByRole('button', { name: 'Expert', exact: true }).click();
  await expect(blue.getByRole('button', { name: 'Expert', exact: true })).toHaveAttribute('aria-pressed', 'true');
  await page.getByRole('button', { name: 'Start Game' }).click();
  const redStarts = (await page.locator('.hud-chip-name').textContent())?.trim() === 'Red';
  if (redStarts) await playMove(page);
  await expect(placedTiles(page)).toHaveCount(redStarts ? 2 : 1, { timeout: 30_000 });
  await expect(page.locator('.hud-chip-name')).toHaveText('Red');
});
