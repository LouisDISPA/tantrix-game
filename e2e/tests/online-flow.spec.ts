import { expect, test, type Page } from '@playwright/test';
import { attachAll, instrument, type Diagnostics } from './diagnostics';
import { openApp } from './open-app';
import { placedTiles, playMove } from './play-move';

/**
 * Full online multiplayer flow with runtime instrumentation.
 *
 * Unlike `full-game.spec.ts` (which only asserts the end state), this walks the
 * whole lifecycle — lobby, color seats, start, every move, game over — and after
 * *each* step cross-checks both browsers, the WebRTC connection state and the
 * signaling socket. On failure the merged network/console timeline is attached.
 */

const tileCount = 56;
const initiallyDealtTiles = 12;

interface Player {
  page: Page;
  diag: Diagnostics;
  color: 'Red' | 'Blue';
}

async function rtcHealth(page: Page) {
  return page.evaluate(() => {
    const w = window as unknown as { __rtc?: { kind: string; detail?: { state?: string } }[] };
    const events = w.__rtc ?? [];
    const bad = events.filter(
      (e) =>
        e.kind === 'dc-error' ||
        e.kind === 'dc-send-throw' ||
        (e.kind === 'pc-connection' && ['failed', 'disconnected'].includes(e.detail?.state ?? '')) ||
        (e.kind === 'pc-ice' && ['failed', 'disconnected'].includes(e.detail?.state ?? '')),
    );
    return { total: events.length, bad };
  });
}

test('plays a fully instrumented online game', async ({ page, browser }, testInfo) => {
  test.setTimeout(10 * 60_000);

  const hostDiag = await instrument(page, 'host');
  const guestContext = await browser.newContext();
  const guest = await guestContext.newPage();
  const guestDiag = await instrument(guest, 'guest');

  try {
    // 1. Host creates a room.
    await openApp(page);
    await page.getByRole('button', { name: 'Online' }).click();
    await page.getByRole('button', { name: 'Host a new room' }).click();
    await expect(page.getByRole('heading', { name: 'LOBBY' })).toBeVisible({ timeout: 20_000 });
    const code = new URL(page.url()).pathname.match(/^\/online\/([A-Z]{6})\/host$/)?.[1] ?? '';
    expect(code).toMatch(/^[A-Z]{6}$/);

    // 2. Guest joins through the entry form.
    await openApp(guest);
    await guest.getByRole('button', { name: 'Online' }).click();
    await guest.getByLabel('Join code').fill(code);
    await guest.getByRole('button', { name: 'Join room' }).click();
    await expect(guest.getByRole('heading', { name: 'LOBBY' })).toBeVisible({ timeout: 20_000 });

    // Both browsers must agree the peer connection is established.
    await expect(page.locator('.hud-chip-room, .lobby-code, body')).toContainText(code);
    await expect.poll(() => rtcHealth(page).then((h) => h.total), { timeout: 20_000 }).toBeGreaterThan(0);
    await expect.poll(() => rtcHealth(guest).then((h) => h.total), { timeout: 20_000 }).toBeGreaterThan(0);

    // 3. Seats: each side claims one, and the other side must see it.
    await page.getByRole('button', { name: /Red.*Open/ }).click();
    await guest.getByRole('button', { name: /Blue.*Open/ }).click();
    await expect(guest.getByRole('button', { name: /Red/ }).first()).not.toContainText('Open');
    await expect(page.getByRole('button', { name: /Blue/ }).first()).not.toContainText('Open');

    // 4. Host starts the game; both browsers must switch to play.
    const startGame = page.getByRole('button', { name: 'Start game' });
    await expect(startGame).toBeEnabled();
    await startGame.click();
    await expect(page.getByText('Select a tile from your hand')).toBeVisible();
    await expect(guest.getByText('Select a tile from your hand')).toBeVisible();
    await expect(page.getByText('44 in bag')).toBeVisible();
    await expect(guest.getByText('44 in bag')).toBeVisible();

    const players: Record<'Red' | 'Blue', Player> = {
      Red: { page, diag: hostDiag, color: 'Red' },
      Blue: { page: guest, diag: guestDiag, color: 'Blue' },
    };

    // 5. Play to the end, cross-checking both browsers after every move.
    const onlineMenu = page.getByRole('button', { name: 'Online Menu' });
    const played = { Red: 0, Blue: 0 };
    for (let move = 0; move < tileCount && !(await onlineMenu.isVisible()); move++) {
      await test.step(`move ${move + 1}`, async () => {
        const color = ((await page.locator('.hud-chip-name').textContent())?.trim() ?? '') as
          | 'Red'
          | 'Blue';
        expect(color).toMatch(/^(Red|Blue)$/);
        // Guest must agree on whose turn it is *before* anyone moves.
        await expect(guest.locator('.hud-chip-name')).toHaveText(color);

        let count: number;
        try {
          count = await playMove(players[color].page);
        } catch (error) {
          if (await onlineMenu.isVisible()) return;
          throw error;
        }
        played[color]++;
        await expect(placedTiles(page)).toHaveCount(count);
        await expect(placedTiles(guest)).toHaveCount(count);

        // Bag counters stay in lockstep on both sides.
        const hostBag = (await page.getByText(/\d+ in bag/).first().textContent()) ?? '';
        await expect(guest.getByText(hostBag).first()).toBeVisible();

        // The roster badge must never claim someone is offline while the data
        // channel is demonstrably alive (that would be a stale `connected` flag).
        for (const [who, p] of [['host', page], ['guest', guest]] as const) {
          const badge = await p.locator('.online-participants-toggle').textContent();
          expect(badge, `${who} players badge after move ${move + 1}`).not.toMatch(/offline/i);
        }

        // No dropped connection at any point mid-game.
        expect((await rtcHealth(page)).bad, 'host rtc bad events').toEqual([]);
        expect((await rtcHealth(guest)).bad, 'guest rtc bad events').toEqual([]);
        await expect(page.locator('.hud-chip-conn-label')).toHaveText(/Connected|Game over/);
        await expect(guest.locator('.hud-chip-conn-label')).toHaveText(/Connected|Game over/);
      });
    }

    // 6. Both browsers land on the same results screen, and nobody was disconnected.
    await expect(onlineMenu).toBeVisible();
    await expect(guest.getByRole('button', { name: 'Online Menu' })).toBeVisible();
    await expect(page.locator('.hud-chip-conn-label')).toHaveText('Game over');
    await expect(guest.locator('.hud-chip-conn-label')).toHaveText('Game over');
    await expect(page.locator('.results-row')).toHaveCount(2);
    await expect(guest.locator('.results-row')).toHaveCount(2);
    expect(played.Red).toBeGreaterThan(0);
    expect(played.Blue).toBeGreaterThan(0);
    const finalCount = await placedTiles(page).count();
    expect(finalCount).toBeGreaterThanOrEqual(tileCount - initiallyDealtTiles);
    await expect(placedTiles(guest)).toHaveCount(finalCount);
    await expect(guest.locator('.results-row')).toHaveText(
      await page.locator('.results-row').allTextContents(),
    );

    expect(hostDiag.problems(), 'host runtime problems').toEqual([]);
    expect(guestDiag.problems(), 'guest runtime problems').toEqual([]);
  } catch (error) {
    await attachAll(testInfo, hostDiag, guestDiag);
    throw error;
  } finally {
    await guestContext.close();
  }
});

test('guest can reload mid-game and resume with the same seat', async ({ page, browser }, testInfo) => {
  test.setTimeout(3 * 60_000);

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

    await guest.goto(`/join/${code}`);
    await expect(guest.getByRole('heading', { name: 'LOBBY' })).toBeVisible({ timeout: 20_000 });
    await page.getByRole('button', { name: /Red.*Open/ }).click();
    await guest.getByRole('button', { name: /Blue.*Open/ }).click();
    await page.getByRole('button', { name: 'Start game' }).click();
    await expect(guest.getByText('Select a tile from your hand')).toBeVisible();

    // Red (host) plays one move so the board is non-empty when the guest reloads.
    const firstColor = (await page.locator('.hud-chip-name').textContent())?.trim();
    const first = firstColor === 'Red' ? page : guest;
    const count = await playMove(first);
    await expect(placedTiles(page)).toHaveCount(count);
    await expect(placedTiles(guest)).toHaveCount(count);

    await guest.reload();
    await expect(placedTiles(guest)).toHaveCount(count, { timeout: 30_000 });
    await expect(guest.locator('.hud-chip-conn-label')).toHaveText('Connected');
    await expect(guest.locator('.hud-chip-name')).toHaveText(
      (await page.locator('.hud-chip-name').textContent())?.trim() ?? '',
    );

    // The rebound guest can still make its own move.
    const nextColor = (await page.locator('.hud-chip-name').textContent())?.trim();
    const next = nextColor === 'Red' ? page : guest;
    const nextCount = await playMove(next);
    await expect(placedTiles(page)).toHaveCount(nextCount);
    await expect(placedTiles(guest)).toHaveCount(nextCount);
  } catch (error) {
    await attachAll(testInfo, hostDiag, guestDiag);
    throw error;
  } finally {
    await guestContext.close();
  }
});
