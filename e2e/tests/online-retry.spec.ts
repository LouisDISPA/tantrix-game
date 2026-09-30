import { expect, test } from '@playwright/test';
import { attachAll, instrument } from './diagnostics';

/** A mistyped or expired code must not silently create an empty room. */
test('joining a missing room shows a useful error', async ({ page }, testInfo) => {
  const diagnostic = await instrument(page, 'joiner');
  const alphabet = 'ABCDEFGHJKLMNPRSTUVWXYZ';
  const code = Array.from({ length: 6 }, () => alphabet[Math.floor(Math.random() * alphabet.length)]).join('');

  try {
    await page.goto(`/online/${code}`);
    await expect(page.getByText(/Room not found/i)).toBeVisible({ timeout: 20_000 });
    await expect(page.getByRole('button', { name: 'Back to online menu' })).toBeVisible();
    await page.getByRole('button', { name: 'Back to online menu' }).click();
    await expect(page.getByRole('button', { name: 'Create a room' })).toBeVisible();
  } catch (error) {
    await attachAll(testInfo, diagnostic);
    throw error;
  }
});
