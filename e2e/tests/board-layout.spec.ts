import { expect, test } from '@playwright/test';
import { openApp } from './open-app';
import { playMove } from './play-move';

/**
 * The End Game and zoom controls used to float over the board, so a legal
 * position could sit underneath them and be impossible to click. They now live
 * in their own strip below the playfield; these checks keep it that way at the
 * sizes that matter (desktop, small laptop, phone).
 */
const viewports = [
  { width: 1280, height: 720 },
  { width: 800, height: 600 },
  { width: 390, height: 844 },
];

for (const viewport of viewports) {
  test(`board controls never cover playable positions at ${viewport.width}x${viewport.height}`, async ({
    page,
  }) => {
    await page.setViewportSize(viewport);
    await openApp(page);
    await page.getByRole('button', { name: 'Local Game' }).click();
    await page.getByRole('button', { name: 'Start Game' }).click();

    // Build up a board so legal positions exist around it.
    for (let move = 0; move < 8; move++) await playMove(page);

    const stage = await page.locator('.board-stage').boundingBox();
    const bar = await page.locator('.board-control-bar').boundingBox();
    expect(stage && bar).toBeTruthy();
    expect(stage!.y + stage!.height, 'playfield must end where the control bar starts').toBeLessThanOrEqual(
      bar!.y + 1,
    );

    // Every control is the topmost thing at its own center (i.e. clickable).
    for (const selector of ['.end-game-btn', '[title="Zoom in"]', '[title="Zoom out"]', '[title="Reset zoom"]']) {
      const clickable = await page.locator(selector).evaluate((el) => {
        const r = el.getBoundingClientRect();
        return el.contains(document.elementFromPoint(r.x + r.width / 2, r.y + r.height / 2));
      });
      expect(clickable, `${selector} is covered`).toBe(true);
    }

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
