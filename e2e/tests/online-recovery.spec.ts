import { mkdirSync } from 'node:fs';
import path from 'node:path';
import { expect, test, type Page, type WebSocketRoute } from '@playwright/test';
import { openApp } from './open-app';
import { placedTiles, playMove } from './play-move';

async function startRoom(host: Page, guest: Page) {
  await openApp(host, '/online');
  await host.getByRole('button', { name: 'Create a room' }).click();
  await expect(host.getByRole('heading', { name: 'LOBBY' })).toBeVisible();
  await openApp(guest, new URL(host.url()).pathname);
  await expect(guest.getByRole('heading', { name: 'LOBBY' })).toBeVisible();
  await host.getByRole('button', { name: /Red.*Open/ }).click();
  await guest.getByRole('button', { name: /Blue.*Open/ }).click();
  await host.getByRole('button', { name: 'Start game' }).click();
  await expect(host.locator('.hud-chip-name')).toBeVisible();
  await expect(guest.locator('.hud-chip-name')).toBeVisible();
}

async function controlConnection(page: Page) {
  const sockets: { client: WebSocketRoute; server?: WebSocketRoute }[] = [];
  const pending: (() => void)[] = [];
  let mode: 'normal' | 'fail' | 'hold' | 'silent' = 'normal';
  const commands: string[] = [];
  await page.routeWebSocket(/\/room\//, client => {
    if (mode === 'fail') {
      sockets.push({ client });
      client.close({ code: 1011, reason: 'Test interruption' });
      return;
    }
    const server = client.connectToServer();
    sockets.push({ client, server });
    client.onMessage(message => {
      commands.push(String(message));
      server.send(message);
    });
    server.onMessage(message => {
      const text = String(message);
      if (mode === 'silent' && text === 'pong') return;
      if (mode === 'hold' && /"(GameStarted|StateUpdate|GameEnded)"/.test(text)) {
        pending.push(() => client.send(message));
      } else client.send(message);
    });
  });
  return {
    sockets, commands, pending,
    mode(value: typeof mode) { mode = value; },
    flush() { pending.splice(0).forEach(send => send()); },
    async drop() {
      const socket = sockets.at(-1)!;
      await socket.client.close({ code: 1011, reason: 'Test interruption' });
      await socket.server?.close();
    },
  };
}

async function captureLayouts(page: Page, state: string) {
  const notice = page.locator('.connection-notice');
  for (const viewport of [{ name: 'desktop', width: 1280, height: 800 }, { name: 'phone', width: 390, height: 844 }]) {
    await page.setViewportSize(viewport);
    for (const theme of ['dark', 'light']) {
      // Use the app's own theme control in both responsive layouts.
      const toggle = page.getByRole('button', { name: `Switch to ${theme} theme` });
      if (await page.locator('.app-root').getAttribute('data-theme') !== theme) {
        if (!await toggle.isVisible()) await page.getByRole('button', { name: 'Menu', exact: true }).click();
        await toggle.click();
      }
      await expect(page.locator('.app-root')).toHaveAttribute('data-theme', theme);
      await expect(notice).toBeVisible();
      expect(await notice.evaluate(element => {
        const box = element.getBoundingClientRect();
        return box.left >= 0 && box.right <= innerWidth && box.top >= 0 && box.bottom <= innerHeight
          && element.scrollWidth <= element.clientWidth && document.documentElement.scrollWidth <= innerWidth;
      }), `${state} ${viewport.name} ${theme} fits`).toBe(true);
      const bounds = (await notice.boundingBox())!;
      expect(bounds.width, `${state} stays compact`).toBeLessThanOrEqual(380);
      expect(Math.abs(bounds.x + bounds.width / 2 - viewport.width / 2), `${state} is centered`).toBeLessThan(1);
      await expect(page.locator('.turn-banner')).toBeHidden();
      const hand = (await page.locator('.player-hand.shown .hand-header').boundingBox())!;
      expect(bounds.y + bounds.height, `${state} leaves the hand accessible`).toBeLessThan(hand.y);
      if (process.env.TANTRIX_SCREENSHOTS) {
        const out = path.join(__dirname, '..', 'screenshots');
        mkdirSync(out, { recursive: true });
        await page.screenshot({ path: path.join(out, `online-${state}-${viewport.name}-${theme}.png`), animations: 'disabled' });
      }
    }
  }
}

test('recovery keeps the board mounted and waits for a fresh snapshot', async ({ page, browser }) => {
  const connection = await controlConnection(page);
  const guestContext = await browser.newContext();
  const guest = await guestContext.newPage();
  try {
    await startRoom(page, guest);
    for (let i = 0; i < 3; i++) {
      await playMove((await page.locator('.hud-chip-name').textContent())?.trim() === 'Red' ? page : guest);
      await expect(placedTiles(page)).toHaveCount(i + 1);
    }
    const board = page.locator('.board-viewport');
    await board.evaluate(element => element.setAttribute('data-preserved', 'yes'));
    const stage = (await page.locator('.board-stage').boundingBox())!;
    const at = { x: stage.x + stage.width * 0.8, y: stage.y + stage.height * 0.6 };
    await page.mouse.move(at.x, at.y);
    await page.mouse.wheel(0, -120);
    await expect(board).not.toHaveAttribute('style', '');
    const pose = await board.getAttribute('style');
    const count = await placedTiles(page).count();
    const hands = await page.locator('.hand-panel').textContent();
    // An open confirmation must close immediately on disconnection.
    await page.getByRole('button', { name: 'End Game', exact: true }).click();
    await expect(page.getByRole('dialog')).toBeVisible();
    connection.mode('hold');
    await connection.drop();
    await expect(page.getByText('Reconnecting… Attempt 1 of 3', { exact: true })).toBeVisible();
    await expect(page.getByRole('dialog')).toHaveCount(0);
    await expect.poll(() => connection.sockets.length).toBe(2);
    // Welcome has arrived, but its lobby cannot replace the cached game.
    await expect(page.locator('.hud-chip-conn-label')).toHaveText('Reconnecting');
    await expect(page.getByRole('heading', { name: 'LOBBY' })).toHaveCount(0);
    await expect(placedTiles(page)).toHaveCount(count);
    await expect(page.locator('.hand-panel')).toHaveText(hands!);
    await expect(board).toHaveAttribute('data-preserved', 'yes');
    await expect(board).toHaveAttribute('style', pose!);
    await expect(page.getByRole('button', { name: 'End Game', exact: true })).toHaveCount(0);
    await expect(page.locator('.hand-tile[data-draggable=true]')).toHaveCount(0);
    expect(connection.sockets[1].client.url()).toBe(connection.sockets[0].client.url());
    const before = connection.commands.filter(command => /PlayMove|EndGame|ReplaceSeat/.test(command));
    await page.mouse.move(at.x, at.y);
    await page.mouse.down();
    await page.mouse.move(at.x + 40, at.y + 30, { steps: 4 });
    await page.mouse.up();
    await expect(board).not.toHaveAttribute('style', pose!);
    const recoveryPose = await board.getAttribute('style');
    await page.getByRole('tab', { name: /Blue's tiles/ }).click();
    await page.getByRole('tab', { name: /Red's tiles/ }).click();
    await captureLayouts(page, 'reconnecting');
    expect(connection.commands.filter(command => /PlayMove|EndGame|ReplaceSeat/.test(command))).toEqual(before);
    connection.mode('normal');
    connection.flush();
    await page.setViewportSize({ width: 1280, height: 800 });
    await expect(page.locator('.hud-chip-conn-label')).toHaveText('Connected');
    await expect(page.locator('.connection-notice')).toHaveCount(0);
    await expect(board).toHaveAttribute('data-preserved', 'yes');
    await expect(board).toHaveAttribute('style', recoveryPose!);
    await expect(page.getByRole('button', { name: 'End Game', exact: true })).toBeEnabled();
  } finally { await guestContext.close(); }
});

test('failed reconnects stop after three attempts and manual retry starts a fresh cycle', async ({ page, browser }) => {
  const connection = await controlConnection(page);
  const guestContext = await browser.newContext();
  const guest = await guestContext.newPage();
  try {
    await startRoom(page, guest);
    connection.mode('fail');
    await connection.drop();
    await expect(page.getByText("Couldn't reconnect", { exact: true })).toBeVisible({ timeout: 15_000 });
    expect(connection.sockets).toHaveLength(4);
    await captureLayouts(page, 'stopped');
    expect(connection.sockets).toHaveLength(4);
    connection.mode('normal');
    await page.setViewportSize({ width: 1280, height: 800 });
    await page.getByRole('button', { name: 'Retry', exact: true }).click();
    await expect(page.locator('.hud-chip-conn-label')).toHaveText('Connected');
    expect(connection.sockets).toHaveLength(5);
    await page.getByRole('button', { name: 'End Game', exact: true }).click();
    await page.getByRole('dialog').getByRole('button', { name: 'Yes, end it' }).click();
    await expect(page.locator('.results-row')).toHaveCount(2);
    await connection.drop();
    await expect(page.locator('.results-row')).toHaveCount(2);
    expect(connection.sockets).toHaveLength(5);
  } finally { await guestContext.close(); }
});

test('a second tab deliberately takes over and neither tab retries automatically', async ({ page, browser, context }) => {
  const connection = await controlConnection(page);
  const guestContext = await browser.newContext();
  const guest = await guestContext.newPage();
  try {
    await startRoom(page, guest);
    const second = await context.newPage();
    const replacement = await controlConnection(second);
    await openApp(second, new URL(page.url()).pathname);
    await expect(second.locator('.hud-chip-conn-label')).toHaveText('Connected');
    await expect(page.getByText('This room is active in another tab', { exact: true })).toBeVisible();
    await captureLayouts(page, 'another-tab');
    expect(connection.sockets).toHaveLength(1);
    expect(replacement.sockets).toHaveLength(1);
    await page.getByRole('button', { name: 'Resume here' }).click();
    await expect(page.locator('.hud-chip-conn-label')).toHaveText('Connected');
    await expect(second.getByText('This room is active in another tab', { exact: true })).toBeVisible();
    expect(connection.sockets).toHaveLength(2);
    expect(replacement.sockets).toHaveLength(1);
    await second.getByRole('button', { name: 'Back to online menu' }).click();
    await expect(second.getByRole('button', { name: 'Create a room' })).toBeVisible();
  } finally { await guestContext.close(); }
});

test('reconnecting restores results when the game ended during the outage', async ({ page, browser }) => {
  const connection = await controlConnection(page);
  const guestContext = await browser.newContext();
  const guest = await guestContext.newPage();
  try {
    await startRoom(page, guest);
    connection.mode('fail');
    await connection.drop();
    await expect(page.locator('.connection-notice')).toBeVisible();
    await guest.getByRole('button', { name: 'End Game', exact: true }).click();
    await guest.getByRole('dialog').getByRole('button', { name: 'Yes, end it' }).click();
    await expect(guest.locator('.results-row')).toHaveCount(2);
    connection.mode('normal');
    await expect(page.locator('.results-row')).toHaveCount(2, { timeout: 15_000 });
    await expect(page.locator('.results-row')).toHaveText(await guest.locator('.results-row').allTextContents());
  } finally { await guestContext.close(); }
});

test('silent transport and background suspension trigger one bounded recovery, and leaving cancels it', async ({ page, browser }) => {
  await page.clock.install();
  const connection = await controlConnection(page);
  const guestContext = await browser.newContext();
  const guest = await guestContext.newPage();
  try {
    await startRoom(page, guest);
    connection.mode('silent');
    await page.clock.runFor(30_000);
    await expect.poll(() => connection.commands.filter(command => command === 'ping').length).toBe(1);
    // Resume after the outstanding pong deadline. Missed timer ticks must not pile up.
    await page.clock.fastForward(120_000);
    await expect(page.getByText('Reconnecting… Attempt 1 of 3', { exact: true })).toBeVisible();
    expect(connection.commands.filter(command => command === 'ping')).toHaveLength(1);
    expect(connection.sockets).toHaveLength(1);
    await page.getByRole('button', { name: 'Back to online menu' }).click();
    await expect(page.getByRole('button', { name: 'Create a room' })).toBeVisible();
    await page.clock.fastForward(120_000);
    expect(connection.sockets).toHaveLength(1);
  } finally { await guestContext.close(); }
});

test('a lost seat resumes as a spectator with a fresh board and no automatic reclaim', async ({ page, browser }) => {
  const connection = await controlConnection(page);
  const guestContext = await browser.newContext();
  const watcherContext = await browser.newContext();
  const guest = await guestContext.newPage();
  const watcher = await watcherContext.newPage();
  try {
    await startRoom(page, guest);
    await openApp(watcher, new URL(page.url()).pathname);
    await expect(watcher.getByRole('group', { name: 'Take a seat' })).toBeVisible();
    connection.mode('fail');
    await connection.drop();
    await expect(page.locator('.connection-notice')).toBeVisible();
    await watcher.getByRole('button', { name: /Red ·/ }).click();
    await watcher.getByRole('dialog').getByRole('button', { name: 'Take seat' }).click();
    await expect(watcher.getByRole('group', { name: 'Take a seat' })).toHaveCount(0);
    await playMove((await guest.locator('.hud-chip-name').textContent())?.trim() === 'Red' ? watcher : guest);
    await expect(placedTiles(guest)).toHaveCount(1);
    await expect(page.getByText("Couldn't reconnect", { exact: true })).toBeVisible({ timeout: 15_000 });
    // A displaced offline member gets a new public identity on Welcome.
    // Lose that attempt before its snapshot, then recover on the next one.
    connection.mode('hold');
    await page.getByRole('button', { name: 'Retry', exact: true }).click();
    await expect.poll(() => connection.pending.length).toBeGreaterThan(0);
    await expect(page.locator('.player-hand.mine')).toHaveCount(1);
    await connection.drop();
    connection.mode('normal');
    await expect(page.locator('.hud-chip-conn-label')).toHaveText('Connected', { timeout: 15_000 });
    await expect(placedTiles(page)).toHaveCount(1);
    await expect(page.getByText('Your seat changed', { exact: true })).toBeVisible();
    await expect(page.getByRole('group', { name: 'Take a seat' })).toBeVisible();
    await expect(page.getByRole('button', { name: 'End Game', exact: true })).toHaveCount(0);
    expect(connection.commands.filter(command => command.includes('ReplaceSeat'))).toHaveLength(0);
    // A takeover confirmation also closes on connection loss.
    await page.getByRole('button', { name: /Red ·/ }).click();
    await expect(page.getByRole('dialog')).toBeVisible();
    await connection.drop();
    await expect(page.getByRole('dialog')).toHaveCount(0);
    await expect(page.getByRole('group', { name: 'Take a seat' })).toHaveCount(0);
  } finally { await guestContext.close(); await watcherContext.close(); }
});

test('a welcomed connection without a snapshot times out and suspension cannot extend the recovery deadline', async ({ page, browser }) => {
  await page.clock.install();
  const connection = await controlConnection(page);
  const guestContext = await browser.newContext();
  const guest = await guestContext.newPage();
  try {
    await startRoom(page, guest);
    connection.mode('hold');
    await connection.drop();
    await expect(page.getByText('Reconnecting… Attempt 1 of 3', { exact: true })).toBeVisible();
    await page.clock.runFor(1_000);
    await expect.poll(() => connection.sockets.length).toBe(2);
    await page.clock.runFor(20_000);
    await expect(page.getByText('Reconnecting… Attempt 2 of 3', { exact: true })).toBeVisible();
    await page.clock.fastForward(120_000);
    await expect(page.getByText("Couldn't reconnect", { exact: true })).toBeVisible();
    expect(connection.sockets).toHaveLength(2);
    await expect(page.locator('.game-layout')).toBeVisible();
    // Late frames on the retired route cannot restore play.
    connection.flush();
    await expect(page.locator('.hud-chip-conn-label')).toHaveText('Disconnected');
  } finally { await guestContext.close(); }
});
