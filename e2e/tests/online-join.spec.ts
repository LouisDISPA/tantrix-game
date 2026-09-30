import { expect, test } from '@playwright/test';
import { attachAll, instrument } from './diagnostics';
import { openApp } from './open-app';

/**
 * Join-handshake reliability. The guest has to (1) get a signaling id, (2) be
 * offered a WebRTC connection by the host, (3) open the data channel, and
 * (4) get a Welcome + Lobby back. Any of those stalling leaves the guest on
 * "Joining room…" forever, so run the handshake many times and dump the full
 * network timeline whenever it does not complete.
 */
const attempts = Number(process.env.JOIN_ATTEMPTS ?? 5);

for (let attempt = 1; attempt <= attempts; attempt++) {
  test(`guest reaches the lobby (attempt ${attempt})`, async ({ page, browser }, testInfo) => {
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
      expect(code).toMatch(/^[A-Z]{6}$/);

      await openApp(guest);
      await guest.getByRole('button', { name: 'Online' }).click();
      await guest.getByLabel('Join code').fill(code);
      await guest.getByRole('button', { name: 'Join room' }).click();
      await expect(guest.getByRole('heading', { name: 'LOBBY' })).toBeVisible({ timeout: 20_000 });
    } catch (error) {
      await attachAll(testInfo, hostDiag, guestDiag);
      throw error;
    } finally {
      await guestContext.close();
    }
  });
}
