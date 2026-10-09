import { expect, test } from '@playwright/test';
import { attachAll, instrument } from './diagnostics';
import { openApp } from './open-app';

/**
 * Anyone can take a seat someone else holds, so a player who walks away
 * can't block it. Because that removes someone, the UI asks first.
 */
test('taking an occupied seat asks for confirmation, then replaces its holder', async ({
  page,
  browser,
}, testInfo) => {
  test.setTimeout(60_000);

  const creatorDiag = await instrument(page, 'creator');
  const guestContext = await browser.newContext();
  const guest = await guestContext.newPage();
  const guestDiag = await instrument(guest, 'guest');

  try {
    await openApp(page);
    await page.getByRole('button', { name: 'Online' }).click();
    await page.getByRole('button', { name: 'Create a room' }).click();
    await expect(page.getByRole('heading', { name: 'LOBBY' })).toBeVisible({ timeout: 20_000 });
    const code = new URL(page.url()).pathname.match(/^\/online\/([A-Z]{6})$/)?.[1] ?? '';

    await openApp(guest);
    await guest.getByRole('button', { name: 'Online' }).click();
    await guest.getByLabel('Join code').fill(code);
    await guest.getByRole('button', { name: 'Join room' }).click();
    await expect(guest.getByRole('heading', { name: 'LOBBY' })).toBeVisible({ timeout: 20_000 });

    // The creator sits in Red.
    await page.getByRole('button', { name: /Red.*Open/ }).click();
    const redOnGuest = guest.locator('.lobby-color-chip.color-red');
    await expect(redOnGuest).toHaveClass(/taken/);
    const dialog = guest.getByRole('dialog');

    // Cancel and Escape both leave the seat alone.
    await redOnGuest.click();
    await expect(dialog).toContainText('Take the Red seat?');
    await dialog.getByRole('button', { name: 'Cancel' }).click();
    await expect(dialog).toBeHidden();
    await redOnGuest.click();
    await guest.keyboard.press('Escape');
    await expect(dialog).toBeHidden();
    await expect(page.locator('.lobby-color-chip.color-red')).toHaveClass(/mine/);

    // Confirming takes it: the guest now holds Red, the creator is unseated
    // but still in the room.
    await redOnGuest.click();
    await dialog.getByRole('button', { name: 'Take seat' }).click();
    await expect(redOnGuest).toHaveClass(/mine/);
    await expect(page.locator('.lobby-color-chip.color-red')).toHaveClass(/taken/);
    await expect(page.locator('.lobby-unseated')).toContainText('Waiting for a seat');

    // The replaced creator picks another color and the game starts.
    await page.getByRole('button', { name: /Blue.*Open/ }).click();
    const startGame = page.getByRole('button', { name: 'Start game' });
    await expect(startGame).toBeEnabled();
    await startGame.click();
    await expect(page.locator('.hud-chip-phase')).toBeVisible();
    await expect(guest.locator('.hud-chip-phase')).toBeVisible();

    expect(creatorDiag.problems(), 'creator runtime problems').toEqual([]);
    expect(guestDiag.problems(), 'guest runtime problems').toEqual([]);
  } catch (error) {
    await attachAll(testInfo, creatorDiag, guestDiag);
    throw error;
  } finally {
    await guestContext.close();
  }
});
