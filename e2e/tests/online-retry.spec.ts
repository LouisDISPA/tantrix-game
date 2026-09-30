import { expect, test } from '@playwright/test';
import { attachAll, instrument } from './diagnostics';

/**
 * A guest that cannot reach a host must not be left on "Joining room…" forever:
 * the connecting screen offers Retry after a few seconds, the attempt times out
 * with a readable message, and Retry recovers once a host shows up.
 */
test('joining an empty room times out, offers Retry, and recovers when a host appears', async ({
  page,
  browser,
}, testInfo) => {
  test.setTimeout(90_000);

  const guestDiag = await instrument(page, 'guest');
  const hostContext = await browser.newContext();
  const host = await hostContext.newPage();
  const hostDiag = await instrument(host, 'host');

  // Random room code from the game's alphabet (no I, O, Q).
  const alphabet = 'ABCDEFGHJKLMNPRSTUVWXYZ';
  const code = Array.from({ length: 6 }, () => alphabet[Math.floor(Math.random() * alphabet.length)]).join('');

  try {
    // 1. Nobody is hosting: the guest connects to signaling and then waits.
    await page.goto(`/online/${code}/client`);
    await expect(page.getByRole('heading', { name: 'Joining room…' })).toBeVisible();

    // 2. The plain spinner grows a way out after a few seconds.
    await expect(page.getByText('Still connecting…')).toBeVisible({ timeout: 15_000 });
    const retryWhileConnecting = page.getByRole('button', { name: 'Retry' });
    await expect(retryWhileConnecting).toBeVisible();

    // 3. Retry restarts the attempt: back to the plain spinner, no stale hint.
    await retryWhileConnecting.click();
    await expect(page.getByRole('heading', { name: 'Joining room…' })).toBeVisible();
    await expect(page.getByText('Still connecting…')).toBeHidden();

    // 4. Left alone, the attempt gives up with an explanation instead of spinning.
    await expect(page.getByRole('heading', { name: "Couldn't connect" })).toBeVisible({
      timeout: 30_000,
    });
    await expect(page.getByText(/direct connection to the host/)).toBeVisible();
    await expect(page.getByRole('button', { name: 'Retry' })).toBeVisible();
    await expect(page.getByRole('button', { name: 'Back to online menu' })).toBeVisible();

    // 5. A host opens the room; Retry from the failure screen now succeeds.
    await host.goto(`/online/${code}/host`);
    await expect(host.getByRole('heading', { name: 'LOBBY' })).toBeVisible({ timeout: 20_000 });

    await page.getByRole('button', { name: 'Retry' }).click();
    await expect(page.getByRole('heading', { name: 'LOBBY' })).toBeVisible({ timeout: 20_000 });
  } catch (error) {
    await attachAll(testInfo, guestDiag, hostDiag);
    throw error;
  } finally {
    await hostContext.close();
  }
});
