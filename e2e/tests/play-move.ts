import { readFileSync } from 'node:fs';
import path from 'node:path';
import { expect, type Page } from '@playwright/test';
import { openApp } from './open-app';

export function placedTiles(page: Page) {
  return page.locator('.board-tile:not(.tile-preview)');
}

/** Play one legal move through the same controls a player uses. */
export async function playMove(page: Page) {
  const placed = placedTiles(page);
  const before = await placed.count();
  const positions = page.locator('.board-empty:not(.faint)');
  const rotation = page.getByRole('button', { name: 'Choose rotation' }).first();
  await page.locator('.player-hand.active .hand-tile:not(.disabled)').first().click();
  await expect.poll(async () => (await placed.count()) > before || (await positions.count()) > 0).toBe(true);
  if ((await placed.count()) === before) {
    await positions.first().click();
    await expect.poll(async () => (await placed.count()) > before || (await rotation.count()) > 0).toBe(true);
    if ((await placed.count()) === before) await rotation.click();
  }
  await expect(placed).toHaveCount(before + 1);
  return before + 1;
}

export async function startLocalGame(page: Page, players = 2, seed = 5) {
  await openApp(page, `/game?seed=${seed}`);
  for (const color of ['Yellow', 'Green'].slice(0, Math.max(0, players - 2))) {
    await page.locator('.player-slot', { hasText: color }).locator('.player-toggle').click();
  }
  await page.getByRole('button', { name: 'Start Game' }).click();
  await expect(page.locator('.game-layout')).toBeVisible();
}

interface FixtureMove {
  player: string;
  tile: string;
  position: { x: number; y: number };
  rotation: string;
  forced: boolean;
  next: string;
}

export const fixture: { seed: number; moves: FixtureMove[] } = JSON.parse(
  readFileSync(path.join(__dirname, '..', 'fixtures', 'forced-after-free-move.json'), 'utf8'),
);

/** The same four placements are also replayed by the Rust tests. */
export async function playFixtureMove(page: Page, move: FixtureMove) {
  const placed = placedTiles(page);
  const before = await placed.count();
  const at = `${move.position.x},${move.position.y}`;
  const position = page.locator(`.board-empty:not(.faint)[data-position="${at}"]`);
  const rotation = page.locator(`.orientation-option[data-rotation="${move.rotation}"]`);
  await page.locator(`.player-hand.active .hand-tile[data-tile="${move.tile}"]`).click();
  await expect.poll(async () => (await placed.count()) > before || (await position.count()) > 0).toBe(true);
  if ((await placed.count()) === before) {
    await position.click();
    await expect.poll(async () => (await placed.count()) > before || (await rotation.count()) > 0).toBe(true);
    if ((await placed.count()) === before) await rotation.click();
  }
  await expect(placed).toHaveCount(before + 1);
}
