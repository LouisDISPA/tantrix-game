import { expect, test } from '@playwright/test';
import { attachAll, instrument } from './diagnostics';
import { openApp } from './open-app';

/**
 * A pick that loses the race for a color (the other player's seat had not yet
 * reached this screen) is rejected by the host. When the *host* is the one who
 * loses, the rejection used to be sent over a data channel to itself, which
 * does not exist: matchbox panics on that and the whole session died. The host
 * must instead show the error, keep its own seat, and carry on.
 */
test('a host whose pick loses a race for a color keeps its seat and its session', async ({
  page,
  browser,
}, testInfo) => {
  test.setTimeout(60_000);

  const hostDiag = await instrument(page, 'host');
  const guestContext = await browser.newContext();
  const guest = await guestContext.newPage();
  const guestDiag = await instrument(guest, 'guest');

  try {
    await openApp(page);
    await page.getByRole('button', { name: 'Online' }).click();
    await page.getByRole('button', { name: 'Host a new room' }).click();
    await expect(page.getByRole('heading', { name: 'LOBBY' })).toBeVisible({ timeout: 20_000 });
    const code = new URL(page.url()).pathname.match(/^\/online\/([A-Z]{6})\/host$/)?.[1] ?? '';

    await openApp(guest);
    await guest.getByRole('button', { name: 'Online' }).click();
    await guest.getByLabel('Join code').fill(code);
    await guest.getByRole('button', { name: 'Join room' }).click();
    await expect(guest.getByRole('heading', { name: 'LOBBY' })).toBeVisible({ timeout: 20_000 });

    // Host sits in Red; the guest takes Blue.
    await page.getByRole('button', { name: /Red.*Open/ }).click();
    await guest.getByRole('button', { name: /Blue.*Open/ }).click();
    const blueOnHost = page.locator('.lobby-color-chip.color-blue');
    await expect(blueOnHost).toHaveClass(/taken/);

    // Simulate the stale screen: the host clicks Blue as if it still looked free.
    await blueOnHost.evaluate((el: HTMLButtonElement) => {
      el.disabled = false;
      el.click();
    });

    await expect(page.getByText('That color is taken.')).toBeVisible();
    // The failed pick must not have cost the host the seat it already held.
    await expect(page.locator('.lobby-color-chip.color-red')).toHaveClass(/mine/);
    await expect(guest.locator('.lobby-color-chip.color-red')).toHaveClass(/taken/);

    // The session is alive: the game still starts with both seats.
    const startGame = page.getByRole('button', { name: 'Start game' });
    await expect(startGame).toBeEnabled();
    await startGame.click();
    await expect(page.getByText('Select a tile from your hand')).toBeVisible();
    await expect(guest.getByText('Select a tile from your hand')).toBeVisible();

    expect(hostDiag.problems(), 'host runtime problems (a panic shows up here)').toEqual([]);
    expect(guestDiag.problems(), 'guest runtime problems').toEqual([]);
  } catch (error) {
    await attachAll(testInfo, hostDiag, guestDiag);
    throw error;
  } finally {
    await guestContext.close();
  }
});
