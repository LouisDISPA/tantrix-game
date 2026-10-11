import { expect, test } from '@playwright/test';
import { fixture, playFixtureMove, startLocalGame } from './play-move';

test('a free move that creates a forced space keeps the turn until it is filled', async ({ page }) => {
  await startLocalGame(page, 2, fixture.seed);
  const current = page.locator('.hud-chip-name');
  for (const move of fixture.moves) {
    await expect(current).toHaveText(move.player);
    if (move.forced) {
      await expect(page.locator('.hud-chip-phase')).toContainText('Forced move!');
      await expect(page.locator(`.player-hand.active .hand-tile.forced[data-tile="${move.tile}"]`)).toBeVisible();
    }
    await playFixtureMove(page, move);
    await expect(current).toHaveText(move.next);
  }
});
