import { expect, test } from '@playwright/test';
import { attachAll, instrument } from './diagnostics';
import { openOnline } from './open-app';
import { placedTiles, playMove } from './play-move';

test('the room survives its creator leaving and another player starts', async ({ page, browser }, testInfo) => {
  test.setTimeout(90_000);
  const creatorLog = await instrument(page, 'creator');
  const firstContext = await browser.newContext();
  const secondContext = await browser.newContext();
  const first = await firstContext.newPage();
  const second = await secondContext.newPage();
  const firstLog = await instrument(first, 'first');
  const secondLog = await instrument(second, 'second');

  try {
    await openOnline(page);
    await page.getByRole('button', { name: 'Create a room' }).click();
    await expect(page.getByRole('heading', { name: 'LOBBY' })).toBeVisible();
    const code = new URL(page.url()).pathname.match(/^\/online\/([A-Z]{6})$/)?.[1] ?? '';
    expect(code).toMatch(/^[A-Z]{6}$/);

    await first.goto(`/join/${code}`);
    await second.goto(`/join/${code}`);
    await expect(first.getByRole('heading', { name: 'LOBBY' })).toBeVisible();
    await expect(second.getByRole('heading', { name: 'LOBBY' })).toBeVisible();
    await page.close();

    await first.getByRole('button', { name: /Red.*Open/ }).click();
    await second.getByRole('button', { name: /Blue.*Open/ }).click();
    await expect(first.getByRole('button', { name: 'Start game' })).toBeEnabled();
    await first.getByRole('button', { name: 'Start game' }).click();
    await expect(first.locator('.hud-chip-phase')).toBeVisible();
    await expect(second.locator('.hud-chip-phase')).toBeVisible();

    const color = (await first.locator('.hud-chip-name').textContent())?.trim();
    const mover = color === 'Red' ? first : second;
    const count = await playMove(mover);
    await expect(placedTiles(first)).toHaveCount(count);
    await expect(placedTiles(second)).toHaveCount(count);
    expect(firstLog.problems()).toEqual([]);
    expect(secondLog.problems()).toEqual([]);
  } catch (error) {
    await attachAll(testInfo, creatorLog, firstLog, secondLog);
    throw error;
  } finally {
    await firstContext.close();
    await secondContext.close();
  }
});

test('a late visitor watches, then takes over a disconnected player\'s seat', async ({ page, browser }, testInfo) => {
  test.setTimeout(90_000);
  const creatorLog = await instrument(page, 'creator');
  const secondContext = await browser.newContext();
  const lateContext = await browser.newContext();
  const second = await secondContext.newPage();
  const late = await lateContext.newPage();
  const lateLog = await instrument(late, 'late');

  try {
    await openOnline(page);
    await page.getByRole('button', { name: 'Create a room' }).click();
    await expect(page.getByRole('heading', { name: 'LOBBY' })).toBeVisible();
    const code = new URL(page.url()).pathname.match(/^\/online\/([A-Z]{6})$/)?.[1] ?? '';

    await second.goto(`/join/${code}`);
    await expect(second.getByRole('heading', { name: 'LOBBY' })).toBeVisible();
    await page.getByRole('button', { name: /Red.*Open/ }).click();
    await second.getByRole('button', { name: /Blue.*Open/ }).click();
    await expect(page.getByRole('button', { name: 'Start game' })).toBeEnabled();
    await page.getByRole('button', { name: 'Start game' }).click();
    await expect(page.locator('.hud-chip-phase')).toBeVisible();

    // Blue walks away mid-game, which would stall every Blue turn.
    await secondContext.close();

    // A newcomer joins the running game as a watcher and sees Blue offline.
    await late.goto(`/join/${code}`);
    const takeover = late.getByRole('group', { name: 'Take a seat' });
    await expect(takeover).toBeVisible({ timeout: 20_000 });
    const blue = takeover.getByRole('button', { name: /Blue/ });
    await expect(blue).toContainText('offline');
    await expect(page.getByRole('group', { name: 'Take a seat' })).toBeHidden();

    await blue.click();
    const dialog = late.getByRole('dialog');
    await expect(dialog).toContainText('Take over Blue?');
    await dialog.getByRole('button', { name: 'Take seat' }).click();
    await expect(takeover).toBeHidden();

    // The newcomer now plays Blue's hand; the game carries on for both.
    const players: Record<string, typeof page> = { Red: page, Blue: late };
    let count = 0;
    for (let turn = 0; turn < 2; turn++) {
      const color = (await page.locator('.hud-chip-name').textContent())?.trim() ?? '';
      await expect(late.locator('.hud-chip-name')).toHaveText(color);
      count = await playMove(players[color]);
      await expect(placedTiles(page)).toHaveCount(count);
      await expect(placedTiles(late)).toHaveCount(count);
    }
    expect(count).toBe(2);
    expect(creatorLog.problems()).toEqual([]);
    expect(lateLog.problems()).toEqual([]);
  } catch (error) {
    await attachAll(testInfo, creatorLog, lateLog);
    throw error;
  } finally {
    await lateContext.close();
  }
});
