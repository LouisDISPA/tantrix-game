import { expect, test, type Page } from '@playwright/test';
import { openApp } from './open-app';
import { placedTiles, playMove, startLocalGame } from './play-move';

/**
 * The game screen on a desktop window (`desk.css`): the board is the screen,
 * the shown seat's rack sits under it, a card for every other player beside
 * it, and the tools in the bar.
 */
test.use({ viewport: { width: 1280, height: 720 } });

const SEED = 3;

type Box = { left: number; top: number; right: number; bottom: number };

async function openGame(page: Page, players: number) {
  await startLocalGame(page, players, SEED);
  for (let move = 0; move < 4; move++) await playMove(page);
}

/** The box of the `index`th element matching `selector`, on the screen. */
async function boxOf(page: Page, selector: string, index = 0): Promise<Box> {
  return page.locator(selector).nth(index).evaluate((el) => {
    const r = el.getBoundingClientRect();
    return { left: r.left, top: r.top, right: r.right, bottom: r.bottom };
  });
}

const overlap = (a: Box, b: Box) => a.left < b.right && b.left < a.right && a.top < b.bottom && b.top < a.bottom;

const colorOf = async (hand: ReturnType<Page['locator']>) => (await hand.getAttribute('class'))!.match(/color-(\w+)/)![1];

test('every player is on screen: the rack under the board, a card each beside it, the tools in the bar', async ({ page }) => {
  await openApp(page);
  await openGame(page, 3);

  // The screen is the window: nothing scrolls.
  const scroll = await page.evaluate(() => [document.documentElement.scrollWidth, document.documentElement.scrollHeight]);
  expect(scroll[0]).toBeLessThanOrEqual(1280);
  expect(scroll[1]).toBeLessThanOrEqual(720);

  // A rack or a card for each, six tiles and a score on each.
  const hands = page.locator('.player-hand');
  await expect(hands).toHaveCount(3);
  for (let seat = 0; seat < 3; seat++) {
    await expect(hands.nth(seat).locator('.hand-tile')).toHaveCount(6);
    await expect(hands.nth(seat).locator('.hand-score')).toBeVisible();
  }

  // The rack is under the board's cell, the rail beside it, and they never meet.
  const stage = await boxOf(page, '.board-stage');
  const rack = await boxOf(page, '.player-hand.shown');
  const rail = await boxOf(page, '.hand-panel');
  expect(rack.top).toBeGreaterThanOrEqual(stage.bottom - 1);
  expect(rail.right).toBeLessThanOrEqual(stage.left + 1);
  expect(overlap(rack, rail)).toBe(false);

  // The hint is there, and every tool is in the bar, clickable.
  await expect(page.locator('.hud-chip-phase')).toBeVisible();
  const bar = await boxOf(page, '.game-hud');
  for (const selector of ['[title="Zoom out"]', '[title="Zoom in"]', '[title="Fit board"]', '.end-game-btn', '.hud-menu-item']) {
    const tool = await boxOf(page, selector);
    expect(tool.top, selector).toBeGreaterThanOrEqual(bar.top - 1);
    expect(tool.bottom, selector).toBeLessThanOrEqual(bar.bottom + 1);
    const clickable = await page.locator(selector).evaluate((el) => {
      const r = el.getBoundingClientRect();
      return el.contains(document.elementFromPoint(r.x + r.width / 2, r.y + r.height / 2));
    });
    expect(clickable, `${selector} is covered`).toBe(true);
  }
});

test('a click anywhere on a card looks at that player, and the card of the mover goes back', async ({ page }) => {
  await openApp(page);
  await openGame(page, 3);
  const mover = page.locator('.player-hand.active');
  const color = await colorOf(mover);
  await expect(page.locator('.player-hand.shown')).toHaveClass(new RegExp(`color-${color}`));

  // On the card's tiles, not its header.
  const other = page.locator('.player-hand:not(.active)').first();
  const otherColor = await colorOf(other);
  await other.locator('.hand-tile').nth(2).click();
  await expect(page.locator('.player-hand.shown')).toHaveClass(new RegExp(`color-${otherColor}`));
  await expect(page.locator('.player-hand.shown')).toHaveCount(1);

  // The hint says whose tiles are looked at, in place of the turn's.
  await expect(page.locator('.hud-viewing')).toHaveText(new RegExp(`${otherColor}'s tiles`, 'i'));
  await expect(page.locator('.hud-chip-phase')).toBeHidden();

  // Their tiles are not for playing.
  const placed = await placedTiles(page).count();
  await page.locator('.player-hand.shown .hand-tile').first().click({ force: true });
  await page.waitForTimeout(150);
  expect(await placedTiles(page).count()).toBe(placed);
  await expect(page.locator('.board-empty:not(.faint)')).toHaveCount(0);

  // Back to the mover's, by their card.
  await mover.locator('.hand-tiles').click();
  await expect(page.locator('.player-hand.shown')).toHaveClass(new RegExp(`color-${color}`));
  await expect(page.locator('.hud-viewing')).toHaveCount(0);
  await expect(page.locator('.hud-chip-phase')).toBeVisible();

  // The seat's name does the same from the keyboard.
  await other.locator('.hand-header').focus();
  await page.keyboard.press('Enter');
  await expect(page.locator('.player-hand.shown')).toHaveClass(new RegExp(`color-${otherColor}`));
});

test('the board is the screen: a zoomed board runs under the cards, which stay on top, and the window never scrolls', async ({
  page,
}) => {
  await openApp(page);
  await openGame(page, 3);

  // The window clips what runs past it and cannot be scrolled (focusing a hex
  // out there must not slide the screen).
  const scrolled = await page.evaluate(() => {
    const layout = document.querySelector('.game-layout')!;
    layout.scrollTo(400, 400);
    return [layout.scrollLeft, layout.scrollTop];
  });
  expect(scrolled).toEqual([0, 0]);

  // Zoomed in, the board runs under the rail and the rack…
  for (let step = 0; step < 7; step++) await page.getByTitle('Zoom in').click();
  await page.mouse.move(1, 1);
  await page.waitForTimeout(600);
  const rail = await boxOf(page, '.hand-panel');
  const rack = await boxOf(page, '.player-hand.shown');
  const under = await page.evaluate(
    ([rail, rack]) => {
      const tiles = [...document.querySelectorAll('.board-tile:not(.tile-preview)')].map((tile) => tile.getBoundingClientRect());
      const hits: { card: boolean; tile: boolean }[] = [];
      for (const box of [rail, rack]) {
        for (let i = 1; i < 8; i++) {
          for (let j = 1; j < 8; j++) {
            const x = box.left + ((box.right - box.left) * i) / 8;
            const y = box.top + ((box.bottom - box.top) * j) / 8;
            if (!tiles.some((t) => x > t.left && x < t.right && y > t.top && y < t.bottom)) continue;
            // …and it is the card (or the hint over the rack), not the tile, that is at that point.
            const top = document.elementFromPoint(x, y);
            hits.push({ card: !!top?.closest('.hand-panel, .hud-chip-phase'), tile: !!top?.closest('.board-tile') });
          }
        }
      }
      return hits;
    },
    [rail, rack] as const,
  );
  expect(under.length, 'no tile ran under a card').toBeGreaterThan(0);
  expect(under.every((hit) => hit.card && !hit.tile), 'a tile was above a card').toBe(true);
});

test('an online game: the mover is told what to do, the other waits, and the room is in the bar', async ({ page, browser }) => {
  await openApp(page);
  await page.getByRole('button', { name: 'Online' }).click();
  await page.getByRole('button', { name: 'Create a room' }).click();
  await expect(page.getByRole('heading', { name: 'LOBBY' })).toBeVisible({ timeout: 20_000 });
  const code = new URL(page.url()).pathname.match(/^\/online\/([A-Z]{6})$/)?.[1];
  expect(code).toBeDefined();

  const guestContext = await browser.newContext({ viewport: { width: 1280, height: 720 } });
  try {
    const guest = await guestContext.newPage();
    await guest.goto(`/join/${code}`);
    await expect(guest.getByRole('heading', { name: 'LOBBY' })).toBeVisible({ timeout: 20_000 });
    await page.getByRole('button', { name: /Red.*Open/ }).click();
    await guest.getByRole('button', { name: /Blue.*Open/ }).click();
    await page.getByRole('button', { name: 'Start game' }).click();
    await expect(page.locator('.hud-chip-phase')).toBeVisible();

    for (const [who, view] of [['host', page], ['guest', guest]] as const) {
      // The room's code is in the bar.
      await expect(view.locator('.online-hud-badge .hud-chip-room'), who).toHaveText(code!);
      // The rack is this device's seat, marked "you"; the other seat is a card with its player's name.
      await expect(view.locator('.player-hand.mine'), who).toHaveClass(/shown/);
      await expect(view.locator('.player-hand.mine .hand-header-you'), who).toBeVisible();
      await expect(view.locator('.player-hand:not(.mine) .hand-header-name'), who).toBeVisible();
    }

    // The hint over a rack is for its player: the mover is told what to do, the other waits.
    const mover = (await page.locator('.hud-chip-name').textContent())?.trim();
    const [moving, waiting] = mover === 'Red' ? [page, guest] : [guest, page];
    await expect(moving.locator('.hud-chip-phase')).toContainText('Select a tile');
    await expect(waiting.locator('.hud-chip-phase')).toContainText('Waiting');
  } finally {
    await guestContext.close();
  }
});
