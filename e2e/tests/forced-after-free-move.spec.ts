import { readFileSync } from 'node:fs';
import path from 'node:path';
import { expect, test, type Page } from '@playwright/test';
import { placedTiles } from './play-move';

/**
 * A seeded local game (`/game?seed=N` always deals the same bag and turn
 * order) scripted move by move. The Rust tests replay the same fixture
 * through the app state and the room server, so a change to dealing fails
 * there first; regenerate it with
 * `cargo test --workspace print_forced_after_free_move_fixture -- --ignored --nocapture`.
 */
interface FixtureMove {
  player: string;
  tile: string;
  position: { x: number; y: number };
  rotation: string;
  forced: boolean;
  /** Whose turn it is after this move. */
  next: string;
}

const fixture: { seed: number; moves: FixtureMove[] } = JSON.parse(
  readFileSync(path.join(__dirname, '..', 'fixtures', 'forced-after-free-move.json'), 'utf8'),
);

async function startSeededGame(page: Page, seed: number) {
  await page.goto(`/game?seed=${seed}`);
  const start = page.getByRole('button', { name: 'Start Game' });
  // Server-rendered controls appear before their client-side handlers are attached.
  await expect(start).toHaveAttribute('data-dioxus-id', /\d+/);
  await start.click();
}

/** Place exactly the fixture's tile, position and rotation. */
async function playFixtureMove(page: Page, m: FixtureMove) {
  const placed = placedTiles(page);
  const before = await placed.count();
  const at = `${m.position.x},${m.position.y}`;
  const position = page.locator(`.board-empty:not(.faint)[data-position="${at}"]`);
  const rotation = page.locator(`.rotation-bar-slot[data-rotation="${m.rotation}"]`);

  await page.locator(`.player-hand.active .hand-tile[data-tile="${m.tile}"]`).click();

  // A tile or position with a single legal placement commits right away.
  await expect
    .poll(async () => (await placed.count()) > before || (await position.count()) > 0)
    .toBe(true);
  if ((await placed.count()) === before) {
    // The keyboard reaches the hex even if the camera leaves it off-screen.
    await position.press('Enter');
    await expect
      .poll(async () => (await placed.count()) > before || (await rotation.count()) > 0)
      .toBe(true);
    if ((await placed.count()) === before) {
      await rotation.click();
    }
  }

  await expect(placed).toHaveCount(before + 1);
  await expect(page.locator(`.board-tile:not(.tile-preview)[data-position="${at}"]`)).toBeVisible();
}

test('a free move that creates a forced space keeps the turn until it is filled', async ({ page }) => {
  await startSeededGame(page, fixture.seed);
  const currentPlayer = page.locator('.hud-chip-name');
  const activeHand = page.locator('.player-hand.active');

  for (const m of fixture.moves) {
    const kind = m.forced ? 'fills a forced space' : 'makes a free move';
    await test.step(`${m.player} ${kind} at ${m.position.x},${m.position.y}`, async () => {
      await expect(currentPlayer).toHaveText(m.player);
      if (m.forced) {
        await expect(page.getByText('Forced move! You must fill a forced space.')).toBeVisible();
        await expect(activeHand.locator(`.hand-tile.forced[data-tile="${m.tile}"]`)).toBeVisible();
      }

      await playFixtureMove(page, m);

      await expect(currentPlayer).toHaveText(m.next);
      await expect(activeHand).toHaveClass(new RegExp(`color-${m.next.toLowerCase()}`));
    });
  }

  // The scripted game shows the rule, not just a sequence of moves.
  expect(
    fixture.moves.some((m, i) => !m.forced && m.next === m.player && fixture.moves[i + 1]?.forced),
  ).toBe(true);
});
