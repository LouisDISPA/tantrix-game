import { expect, test, type Page } from '@playwright/test';
import { openApp } from './open-app';
import { playMove } from './play-move';

/**
 * The HUD and the hand panel float over the board, but the camera frames the
 * board in the cell they leave (the buttons live in the HUD: inline on a wide
 * window, in a menu on a narrow one). So a legal position can never sit
 * underneath a card or a control, at the sizes that matter (desktop, small
 * laptop, phone).
 */
const viewports = [
  { width: 1280, height: 720 },
  { width: 800, height: 600 },
  { width: 390, height: 844 },
];

/** A narrow window keeps the controls in the menu: open it. */
async function showControls(page: Page) {
  const toggle = page.locator('.hud-menu-toggle');
  if (await toggle.isVisible()) await toggle.click();
}

for (const viewport of viewports) {
  test(`the board is never covered at ${viewport.width}x${viewport.height}`, async ({ page }) => {
    await page.setViewportSize(viewport);
    await openApp(page);
    await page.getByRole('button', { name: 'Local Game' }).click();
    await page.getByRole('button', { name: 'Start Game' }).click();

    // Build up a board so legal positions exist around it.
    for (let move = 0; move < 8; move++) await playMove(page);

    // The screen is exactly the window: the board has the room between the
    // HUD row and the hand panel, and nothing scrolls.
    const rows = await page.evaluate(() => {
      const box = (selector: string) => document.querySelector(selector)!.getBoundingClientRect();
      const [hud, stage, panel] = ['.game-hud', '.board-stage', '.hand-panel'].map(box);
      return {
        hud: { top: hud.top, bottom: hud.bottom, left: hud.left, right: hud.right },
        stage: { top: stage.top, bottom: stage.bottom, left: stage.left, right: stage.right },
        panel: { top: panel.top, bottom: panel.bottom, left: panel.left, right: panel.right },
        scrollWidth: document.documentElement.scrollWidth,
        scrollHeight: document.documentElement.scrollHeight,
      };
    });
    expect(rows.scrollWidth).toBeLessThanOrEqual(viewport.width);
    expect(rows.scrollHeight).toBeLessThanOrEqual(viewport.height);
    expect(rows.stage.top).toBeGreaterThanOrEqual(rows.hud.bottom - 1);
    expect(rows.stage.bottom).toBeLessThanOrEqual(viewport.height);
    // The panel is under the board (a column) or beside it (a side column, or a desktop's rail).
    expect(
      rows.panel.top >= rows.stage.bottom - 1 ||
        rows.panel.left >= rows.stage.right - 1 ||
        rows.panel.right <= rows.stage.left + 1,
    ).toBe(true);
    expect(rows.stage.right - rows.stage.left).toBeGreaterThan(viewport.width * 0.5);

    // Every control is the topmost thing at its own centre (i.e. clickable).
    await showControls(page);
    for (const selector of ['.end-game-btn', '[title="Fit board"]']) {
      const clickable = await page.locator(selector).evaluate((el) => {
        const r = el.getBoundingClientRect();
        return el.contains(document.elementFromPoint(r.x + r.width / 2, r.y + r.height / 2));
      });
      expect(clickable, `${selector} is covered`).toBe(true);
    }
    await page.keyboard.press('Escape');
    if (await page.locator('.hud-menu.open').count()) await page.locator('.hud-menu-backdrop').click();

    // With a tile selected, no legal position that is on screen may be covered.
    // A tile with a single legal placement is played on the spot, so keep going
    // until one of them leaves the choice to the player.
    const choices = page.locator('.board-empty:not(.faint)');
    for (let attempt = 0; attempt < 12 && (await choices.count()) === 0; attempt++) {
      await page.locator('.player-hand.active .hand-tile:not(.disabled)').first().click();
      await page.waitForTimeout(150);
      if ((await choices.count()) === 0) await playMove(page);
    }
    await expect(choices.first()).toBeAttached();
    const covered = await page.evaluate(() => {
      const field = document.querySelector('.board-stage')!.getBoundingClientRect();
      return [...document.querySelectorAll('.board-empty:not(.faint)')]
        .map((el) => {
          const r = el.getBoundingClientRect();
          const x = r.x + r.width / 2;
          const y = r.y + r.height / 2;
          const onScreen = x > field.left && x < field.right && y > field.top && y < field.bottom;
          const top = document.elementFromPoint(x, y);
          return onScreen && !el.contains(top) ? (top as HTMLElement | null)?.className || top?.tagName : null;
        })
        .filter((v): v is string => v !== null);
    });
    expect(covered, 'legal positions covered by another element').toEqual([]);
  });
}
