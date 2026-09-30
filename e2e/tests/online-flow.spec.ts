import { expect, test, type Page } from '@playwright/test';
import { attachAll, instrument } from './diagnostics';
import { openApp } from './open-app';
import { placedTiles, playMove } from './play-move';

/**
 * Full online multiplayer flow with runtime instrumentation.
 *
 * Unlike `full-game.spec.ts` (which only asserts the end state), this walks the
 * whole lifecycle — lobby, color seats, start, every move, game over — and after
 * *each* step cross-checks both browsers and the server WebSocket state. On
 * failure the merged network/console timeline is attached.
 */

const tileCount = 56;
const initiallyDealtTiles = 12;

interface Player {
  page: Page;
}

test('plays a fully instrumented online game', async ({ page, browser }, testInfo) => {
  test.setTimeout(10 * 60_000);

  const creatorDiag = await instrument(page, 'creator');
  const guestContext = await browser.newContext();
  const guest = await guestContext.newPage();
  const guestDiag = await instrument(guest, 'guest');

  try {
    // 1. The first player creates a room.
    await openApp(page);
    await page.getByRole('button', { name: 'Online' }).click();
    await page.getByRole('button', { name: 'Create a room' }).click();
    await expect(page.getByRole('heading', { name: 'LOBBY' })).toBeVisible({ timeout: 20_000 });
    const code = new URL(page.url()).pathname.match(/^\/online\/([A-Z]{6})$/)?.[1] ?? '';
    expect(code).toMatch(/^[A-Z]{6}$/);

    // 2. Guest joins through the entry form.
    await openApp(guest);
    await guest.getByRole('button', { name: 'Online' }).click();
    await guest.getByLabel('Join code').fill(code);
    await guest.getByRole('button', { name: 'Join room' }).click();
    await expect(guest.getByRole('heading', { name: 'LOBBY' })).toBeVisible({ timeout: 20_000 });

    // Both browsers must connect to the room WebSocket.
    await expect(page.locator('.hud-chip-room, .lobby-code, body')).toContainText(code);
    expect(creatorDiag.events().some((e) => e.text.startsWith('ws-open /room/'))).toBe(true);
    expect(guestDiag.events().some((e) => e.text.startsWith('ws-open /room/'))).toBe(true);

    // 3. Seats: each side claims one, and the other side must see it.
    await page.getByRole('button', { name: /Red.*Open/ }).click();
    await guest.getByRole('button', { name: /Blue.*Open/ }).click();
    await expect(guest.getByRole('button', { name: /Red/ }).first()).not.toContainText('Open');
    await expect(page.getByRole('button', { name: /Blue/ }).first()).not.toContainText('Open');

    // 4. A seated player starts the game; both browsers must switch to play.
    const startGame = page.getByRole('button', { name: 'Start game' });
    await expect(startGame).toBeEnabled();
    await startGame.click();
    await expect(page.getByText('Select a tile from your hand')).toBeVisible();
    await expect(guest.getByText('Select a tile from your hand')).toBeVisible();
    await expect(page.getByText('44 in bag')).toBeVisible();
    await expect(guest.getByText('44 in bag')).toBeVisible();

    const players: Record<'Red' | 'Blue', Player> = {
      Red: { page },
      Blue: { page: guest },
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
        const creatorBag = (await page.getByText(/\d+ in bag/).first().textContent()) ?? '';
        await expect(guest.getByText(creatorBag).first()).toBeVisible();

        // Neither room socket should have closed during the game.
        expect(creatorDiag.events().filter((e) => e.text.startsWith('ws-close /room/'))).toEqual([]);
        expect(guestDiag.events().filter((e) => e.text.startsWith('ws-close /room/'))).toEqual([]);
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

    expect(creatorDiag.problems(), 'creator runtime problems').toEqual([]);
    expect(guestDiag.problems(), 'guest runtime problems').toEqual([]);
  } catch (error) {
    await attachAll(testInfo, creatorDiag, guestDiag);
    throw error;
  } finally {
    await guestContext.close();
  }
});

test('guest can reload mid-game and resume with the same seat', async ({ page, browser }, testInfo) => {
  test.setTimeout(3 * 60_000);

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

    await guest.goto(`/join/${code}`);
    await expect(guest.getByRole('heading', { name: 'LOBBY' })).toBeVisible({ timeout: 20_000 });
    await page.getByRole('button', { name: /Red.*Open/ }).click();
    await guest.getByRole('button', { name: /Blue.*Open/ }).click();
    await page.getByRole('button', { name: 'Start game' }).click();
    await expect(guest.getByText('Select a tile from your hand')).toBeVisible();

    // One player moves so the board is non-empty when the guest reloads.
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
    await attachAll(testInfo, creatorDiag, guestDiag);
    throw error;
  } finally {
    await guestContext.close();
  }
});
