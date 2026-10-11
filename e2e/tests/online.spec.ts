import { expect, test } from '@playwright/test';
import { attachAll, instrument } from './diagnostics';
import { openApp } from './open-app';
import { placedTiles, playMove } from './play-move';

test('two players join, synchronize moves, resume after reload and see results', async ({ page, browser }, testInfo) => {
  const guestContext = await browser.newContext();
  const guest = await guestContext.newPage();
  const hostDiag = await instrument(page, 'host');
  const guestDiag = await instrument(guest, 'guest');
  try {
    await openApp(page, '/online');
    await page.getByRole('button', { name: 'Create a room' }).click();
    await expect(page.getByRole('heading', { name: 'LOBBY' })).toBeVisible();
    const code = new URL(page.url()).pathname.split('/').at(-1)!;
    expect(code).toMatch(/^[A-Z]{6}$/);
    await openApp(guest, '/online');
    await guest.getByLabel('Join code').fill(code);
    await guest.getByRole('button', { name: 'Join room' }).click();
    await expect(guest.getByRole('heading', { name: 'LOBBY' })).toBeVisible();
    await page.getByRole('button', { name: /Red.*Open/ }).click();
    await guest.getByRole('button', { name: /Blue.*Open/ }).click();
    await page.getByRole('button', { name: 'Start game' }).click();
    await expect(page.locator('.hud-chip-name')).toBeVisible();
    await expect(guest.locator('.hud-chip-name')).toHaveText((await page.locator('.hud-chip-name').textContent())!.trim());
    const first = (await page.locator('.hud-chip-name').textContent())?.trim() === 'Red' ? page : guest;
    await playMove(first);
    await expect(placedTiles(page)).toHaveCount(1);
    await expect(placedTiles(guest)).toHaveCount(1);

    await guest.reload();
    await expect(guest.locator('.app-root')).toHaveAttribute('data-app-ready', 'true', { timeout: 30_000 });
    await expect(placedTiles(guest)).toHaveCount(1);
    await expect(guest.locator('.hud-chip-conn-label')).toHaveText('Connected');
    // Blue must actually play after reloading, even when Red started second.
    if ((await page.locator('.hud-chip-name').textContent())?.trim() === 'Red') {
      await playMove(page);
      await expect(placedTiles(guest)).toHaveCount(2);
    }
    await expect(guest.locator('.hud-chip-name')).toHaveText('Blue');
    const count = await playMove(guest);
    await expect(placedTiles(page)).toHaveCount(count);
    await expect(placedTiles(guest)).toHaveCount(count);

    await guest.getByRole('button', { name: 'End Game', exact: true }).click();
    await guest.getByRole('dialog').getByRole('button', { name: 'Yes, end it' }).click();
    await expect(page.locator('.results-row')).toHaveCount(2);
    await expect(guest.locator('.results-row')).toHaveText(await page.locator('.results-row').allTextContents());
    await guest.getByRole('button', { name: 'Online Menu' }).click();
    await expect(guest.getByRole('button', { name: 'Create a room' })).toBeVisible();
  } catch (error) {
    await attachAll(testInfo, hostDiag, guestDiag);
    throw error;
  } finally {
    await guestContext.close();
  }
});

test('joining a missing room shows an error and returns to the online menu', async ({ page }) => {
  const alphabet = 'ABCDEFGHJKLMNPRSTUVWXYZ';
  const code = Array.from({ length: 6 }, () => alphabet[Math.floor(Math.random() * alphabet.length)]).join('');
  await openApp(page, `/online/${code}`);
  await expect(page.getByText(/Room not found/i)).toBeVisible();
  await page.getByRole('button', { name: 'Back to online menu' }).click();
  await expect(page.getByRole('button', { name: 'Create a room' })).toBeVisible();
});
