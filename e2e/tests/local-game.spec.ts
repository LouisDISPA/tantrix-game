import { expect, test } from '@playwright/test';
import { openApp } from './open-app';
import { placedTiles, playMove } from './play-move';

test('starts a local game and advances after a move', async ({ page }) => {
  await openApp(page);
  await page.getByRole('button', { name: 'Local Game' }).click();

  await expect(page.getByText('Set up players')).toBeVisible();
  await page.getByRole('button', { name: 'Start Game' }).click();

  const currentPlayer = page.locator('.hud-chip-name');
  const firstPlayer = (await currentPlayer.textContent())?.trim() ?? '';
  expect(firstPlayer).toMatch(/^(Red|Blue)$/);
  await page.getByRole('button', { name: 'Select tile' }).first().click();

  await expect(currentPlayer).not.toHaveText(firstPlayer);
  await expect(page.getByText('43 in bag')).toBeVisible();
});

test('an AI seat picks its difficulty, Medium by default', async ({ page }) => {
  // The tightest layout: the chips must fit on a row of their own.
  await page.setViewportSize({ width: 390, height: 844 });
  await openApp(page);
  await page.getByRole('button', { name: 'Local Game' }).click();

  const levels = page.getByRole('group', { name: 'AI difficulty' });
  const blue = page.locator('.player-slot', { hasText: 'Blue' });
  await expect(levels).toHaveCount(0);

  await blue.getByRole('button', { name: 'Player' }).click();
  const chip = (name: string) => blue.getByRole('button', { name, exact: true });
  await expect(levels).toHaveCount(1);
  await expect(chip('Medium')).toHaveAttribute('aria-pressed', 'true');
  const toggle = (await blue.getByRole('button', { name: 'AI', exact: true }).boundingBox())!;
  const row = (await levels.boundingBox())!;
  const seat = (await blue.boundingBox())!;
  expect(row.y).toBeGreaterThanOrEqual(toggle.y + toggle.height);
  expect(row.x).toBeGreaterThanOrEqual(seat.x);
  expect(row.x + row.width).toBeLessThanOrEqual(seat.x + seat.width);

  await chip('Hard').click();
  await expect(chip('Hard')).toHaveAttribute('aria-pressed', 'true');
  await expect(chip('Medium')).toHaveAttribute('aria-pressed', 'false');

  // Turning the seat off takes its levels away.
  await blue.getByRole('button', { name: 'AI', exact: true }).click();
  await expect(levels).toHaveCount(0);
});

test('an Expert AI answers in the browser', async ({ page }) => {
  // A fixed deal keeps the game, and so this test, the same on every run.
  await page.goto('/game?seed=1');
  const start = page.getByRole('button', { name: 'Start Game' });
  await expect(start).toHaveAttribute('data-dioxus-id', /\d+/);

  const blue = page.locator('.player-slot', { hasText: 'Blue' });
  await blue.getByRole('button', { name: 'Player' }).click();
  await blue.getByRole('button', { name: 'Expert', exact: true }).click();
  await start.click();

  // Whoever starts, Blue's AI plays one turn and hands Red the move.
  const redStarts = (await page.locator('.hud-chip-name').textContent())?.trim() === 'Red';
  if (redStarts) await playMove(page);
  const thinking = page.getByText('Blue is thinking...');
  await expect(thinking).toBeVisible();
  await expect(thinking).toBeHidden({ timeout: 30_000 });

  await expect(page.locator('.hud-chip-name')).toHaveText('Red');
  await expect(placedTiles(page)).toHaveCount(redStarts ? 2 : 1);
});
