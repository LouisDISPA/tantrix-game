import { expect, test } from '@playwright/test';
import { openApp } from './open-app';
import { placedTiles, playMove } from './play-move';

test('a seated player can end an online game early for everyone', async ({ page, browser }) => {
  await openApp(page);
  await page.getByRole('button', { name: 'Online' }).click();
  await page.getByRole('button', { name: 'Create a room' }).click();
  await expect(page.getByRole('heading', { name: 'LOBBY' })).toBeVisible({ timeout: 20_000 });

  const code = new URL(page.url()).pathname.split('/').at(-1);
  expect(code).toMatch(/^[A-Z]{6}$/);

  const guestContext = await browser.newContext();
  try {
    const guest = await guestContext.newPage();
    await openApp(guest);
    await guest.getByRole('button', { name: 'Online' }).click();
    await guest.getByLabel('Join code').fill(code!);
    await guest.getByRole('button', { name: 'Join room' }).click();
    await expect(guest.getByRole('heading', { name: 'LOBBY' })).toBeVisible({ timeout: 20_000 });

    await page.getByRole('button', { name: /Red.*Open/ }).click();
    await guest.getByRole('button', { name: /Blue.*Open/ }).click();
    await page.getByRole('button', { name: 'Start game' }).click();
    await expect(guest.getByRole('button', { name: 'End Game' })).toBeVisible();

    const current = (await page.locator('.hud-chip-name').textContent())?.trim();
    const active = current === 'Red' ? page : guest;
    await playMove(active);
    await expect(placedTiles(page)).toHaveCount(1);
    await expect(placedTiles(guest)).toHaveCount(1);

    // Ending the game for everyone asks first.
    await guest.getByRole('button', { name: 'End Game' }).click();
    await guest.getByRole('dialog').getByRole('button', { name: 'Yes, end it' }).click();
    await expect(page.getByRole('button', { name: 'Online Menu' })).toBeVisible();
    await expect(guest.getByRole('button', { name: 'Online Menu' })).toBeVisible();
    await expect(page.locator('.results-row')).toHaveCount(2);
    await expect(guest.locator('.results-row')).toHaveText(
      await page.locator('.results-row').allTextContents(),
    );
    await expect(placedTiles(page)).toHaveCount(1);
    await expect(placedTiles(guest)).toHaveCount(1);

    await guest.reload();
    await expect(guest.getByRole('button', { name: 'Online Menu' })).toBeVisible({ timeout: 20_000 });
    await expect(placedTiles(guest)).toHaveCount(1);
  } finally {
    await guestContext.close();
  }
});
