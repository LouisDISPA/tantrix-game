import { mkdirSync } from 'node:fs';
import path from 'node:path';
import { expect, test, type WebSocketRoute } from '@playwright/test';
import { attachAll, instrument } from './diagnostics';
import { openApp } from './open-app';
import { placedTiles, playMove } from './play-move';

test('game rejections replace the HUD hint until the next room update', async ({ page, browser }) => {
  let socket: WebSocketRoute;
  await page.routeWebSocket(/\/room\//, route => {
    socket = route;
    route.connectToServer();
  });
  const guestContext = await browser.newContext();
  const guest = await guestContext.newPage();
  try {
    await openApp(page, '/online');
    await page.getByRole('button', { name: 'Create a room' }).click();
    await expect(page.getByRole('heading', { name: 'LOBBY' })).toBeVisible();
    await openApp(guest, new URL(page.url()).pathname);
    await expect(guest.getByRole('heading', { name: 'LOBBY' })).toBeVisible();
    await page.getByRole('button', { name: /Red.*Open/ }).click();
    await guest.getByRole('button', { name: /Blue.*Open/ }).click();
    await page.getByRole('button', { name: 'Start game' }).click();
    await expect(page.locator('.hud-chip-name')).toBeVisible();

    for (let i = 0; i < 3; i++) {
      const mover = (await page.locator('.hud-chip-name').textContent())?.trim() === 'Red' ? page : guest;
      await playMove(mover);
      await expect(placedTiles(page)).toHaveCount(i + 1);
      await expect(placedTiles(guest)).toHaveCount(i + 1);
    }
    await expect(page.locator('.turn-banner')).toHaveCount(0);

    const reject = (code: string) => socket.send(JSON.stringify({ Error: { code, message: 'refused' } }));
    const alert = page.locator('.hud-chip-phase.alert');
    const boardBefore = await placedTiles(page).count();
    const handsBefore = await page.locator('.hand-panel').textContent();
    reject('NotYourTurn');
    await expect(alert).toHaveText("It's not your turn.");
    await expect(alert).toHaveAttribute('role', 'status');
    reject('InvalidMove');
    await expect(alert).toHaveText('The game server rejected that action.');
    await expect(placedTiles(page)).toHaveCount(boardBefore);
    await expect(page.locator('.hand-panel')).toHaveText(handsBefore!);
    await page.getByRole('tab', { name: /Blue's tiles/ }).click();
    await expect(alert).toBeVisible();
    await expect(page.locator('.hud-viewing')).toBeHidden();
    await page.getByRole('tab', { name: /Red's tiles/ }).click();

    // Inspect the longest rejection in each layout and theme.
    for (const viewport of [{ name: 'desktop', width: 1280, height: 800 }, { name: 'phone', width: 390, height: 844 }]) {
      await page.setViewportSize(viewport);
      for (const theme of ['dark', 'light']) {
        if (theme === 'light') {
          if (!await page.getByRole('button', { name: 'Switch to light theme' }).isVisible()) {
            await page.getByRole('button', { name: 'Menu', exact: true }).click();
          }
          await page.getByRole('button', { name: 'Switch to light theme' }).click();
        }
        await expect(alert).toBeVisible();
        const fits = await alert.evaluate(element => {
          const rect = element.getBoundingClientRect();
          return rect.left >= 0 && rect.right <= innerWidth && element.scrollWidth <= element.clientWidth;
        });
        expect(fits, `${viewport.name} ${theme}: rejection fits without clipping`).toBe(true);
        if (process.env.TANTRIX_SCREENSHOTS) {
          const out = path.join(__dirname, '..', 'screenshots');
          mkdirSync(out, { recursive: true });
          await page.screenshot({ path: path.join(out, `online-rejection-${viewport.name}-${theme}.png`), animations: 'disabled' });
        }
      }
      if (!await page.getByRole('button', { name: 'Switch to dark theme' }).isVisible()) {
        await page.getByRole('button', { name: 'Menu', exact: true }).click();
      }
      await page.getByRole('button', { name: 'Switch to dark theme' }).click();
    }

    const first = (await page.locator('.hud-chip-name').textContent())?.trim() === 'Red' ? page : guest;
    await playMove(first);
    await expect(placedTiles(page)).toHaveCount(boardBefore + 1);
    await expect(alert).toHaveCount(0);

    reject('InvalidMove');
    await expect(alert).toBeVisible();
    // Reconnecting publishes a roster update without placing a tile.
    await guest.reload();
    await expect(guest.locator('.app-root')).toHaveAttribute('data-app-ready', 'true');
    await expect(alert).toHaveCount(0);
    await expect(placedTiles(page)).toHaveCount(boardBefore + 1);

    reject('InvalidMove');
    await expect(alert).toBeVisible();
    await guest.getByRole('button', { name: 'End Game', exact: true }).click();
    await guest.getByRole('dialog').getByRole('button', { name: 'Yes, end it' }).click();
    await expect(page.locator('.results-row')).toHaveCount(2);
    await expect(alert).toHaveCount(0);
  } finally {
    await guestContext.close();
  }
});

test('two players join, synchronize moves, resume after reload and see results', async ({ page, browser }, testInfo) => {
  const guestContext = await browser.newContext();
  const guest = await guestContext.newPage();
  const hostDiag = await instrument(page, 'host');
  const guestDiag = await instrument(guest, 'guest');
  try {
    await openApp(page, '/online');
    await page.getByRole('button', { name: 'Create a room' }).click();
    await expect(page.getByRole('heading', { name: 'LOBBY' })).toBeVisible();
    const code = (await page.locator('.lobby-room-code').textContent())!.trim();
    expect(new URL(page.url()).pathname).toMatch(/^\/online\/game\/[a-f0-9-]{36}$/);
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
  await openApp(page, `/join/${code}`);
  await expect(page.getByText(/Room not found/i)).toBeVisible();
  await page.getByRole('button', { name: 'Back to online menu' }).click();
  await expect(page.getByRole('button', { name: 'Create a room' })).toBeVisible();
});


test('a lost creation response can be retried after reload without creating another room', async ({ page }) => {
  const createdIds: string[] = [];
  await page.route('**/api/rooms', async route => {
    const response = await route.fetch();
    createdIds.push((await response.json()).id);
    if (createdIds.length === 1) await route.abort();
    else await route.fulfill({ response });
  });
  await openApp(page, '/online');
  await page.getByRole('button', { name: 'Create a room' }).click();
  await expect(page.getByRole('alert')).toContainText("Couldn't reach the game server");
  await page.reload();
  await page.getByRole('button', { name: 'Create a room' }).click();
  await expect(page.getByRole('heading', { name: 'LOBBY' })).toBeVisible();
  expect(createdIds).toHaveLength(2);
  expect(createdIds[1]).toBe(createdIds[0]);
});
