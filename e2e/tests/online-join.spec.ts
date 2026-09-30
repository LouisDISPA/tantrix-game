import { expect, test } from '@playwright/test';
import { attachAll, instrument } from './diagnostics';
import { openApp } from './open-app';

/**
 * Join-handshake reliability. A guest connects to the room Durable Object,
 * sends Hello, and receives Welcome + Lobby. Repeat to catch intermittent
 * connection or state-loading failures.
 */
const attempts = Number(process.env.JOIN_ATTEMPTS ?? 5);

for (let attempt = 1; attempt <= attempts; attempt++) {
  test(`guest reaches the lobby (attempt ${attempt})`, async ({ page, browser }, testInfo) => {
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
      expect(code).toMatch(/^[A-Z]{6}$/);

      await openApp(guest);
      await guest.getByRole('button', { name: 'Online' }).click();
      await guest.getByLabel('Join code').fill(code);
      await guest.getByRole('button', { name: 'Join room' }).click();
      await expect(guest.getByRole('heading', { name: 'LOBBY' })).toBeVisible({ timeout: 20_000 });
    } catch (error) {
      await attachAll(testInfo, creatorDiag, guestDiag);
      throw error;
    } finally {
      await guestContext.close();
    }
  });
}
