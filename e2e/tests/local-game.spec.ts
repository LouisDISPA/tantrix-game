import { expect, test } from '@playwright/test';
import { openApp } from './open-app';

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
