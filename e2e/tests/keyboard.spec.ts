import { expect, test, type Page } from '@playwright/test';
import { openApp } from './open-app';
import { placedTiles, startLocalGame } from './play-move';
import { useProductionScriptPolicy } from './script-policy';

// Under the production script policy, with no uncaught error.
let errors: string[] = [];
test.beforeEach(async ({ page }) => {
  errors = await useProductionScriptPolicy(page);
});
test.afterEach(() => {
  expect(errors).toEqual([]);
});

/** The tile of the hand to move that has the keyboard's focus. */
function focusedTile(page: Page) {
  return page.locator('.player-hand.active .hand-tile:focus');
}

function mover(page: Page) {
  return page.locator('.hud-chip-name');
}

test('plays local turns from the keyboard alone', async ({ page }) => {
  // Seed 5: Red opens, and each spot of Blue's reply takes the tile two ways.
  await openApp(page, '/game?seed=5');
  await page.getByRole('button', { name: 'Start Game' }).focus();
  await page.keyboard.press('Enter');
  await expect(page.locator('.game-layout')).toBeFocused();
  await expect(mover(page)).toHaveText('Red');

  // A key on the bare screen hands the focus to Red's tiles; focusing one
  // shows where it goes, as hovering does.
  await page.keyboard.press('ArrowRight');
  await expect(focusedTile(page)).toHaveCount(1);
  await expect(page.locator('.board-empty.faint')).toHaveCount(1);
  const first = (await focusedTile(page).getAttribute('data-tile'))!;
  await page.keyboard.press('ArrowRight');
  await expect(focusedTile(page)).not.toHaveAttribute('data-tile', first);
  await page.keyboard.press('ArrowLeft');
  await expect(focusedTile(page)).toHaveAttribute('data-tile', first);

  // The opening tile fits the centre only: Enter plays it, and Blue's
  // tiles take the focus.
  await page.keyboard.press('Enter');
  await expect(placedTiles(page)).toHaveCount(1);
  await expect(mover(page)).toHaveText('Blue');
  await expect(focusedTile(page)).toHaveCount(1);

  // Enter takes a tile: the spots, one tab stop, take the focus on their
  // first spot, ringed, with the ghost on it.
  await page.keyboard.press('Enter');
  const spots = page.getByRole('listbox', { name: 'Where to place the tile' });
  await expect(spots).toBeFocused();
  const spot = (id: string) => page.locator(`[id="${id}"]`);
  // The options come in the order the keys read them.
  const option = (n: number) => spots.getByRole('option').nth(n);
  const firstSpot = (await spots.getAttribute('aria-activedescendant'))!;
  await expect(option(0)).toHaveAttribute('id', firstSpot);
  await expect(page.locator('.spot-ring')).toBeVisible();
  const ghost = page.locator('.board-tile.tile-preview');
  await expect(ghost).toHaveAttribute('data-position', (await spot(firstSpot).getAttribute('data-position'))!);

  // → moves to the next spot, and the ghost with it.
  await page.keyboard.press('ArrowRight');
  await expect(spots).not.toHaveAttribute('aria-activedescendant', firstSpot);
  const chosen = (await spots.getAttribute('aria-activedescendant'))!;
  const at = (await spot(chosen).getAttribute('data-position'))!;
  await expect(option(1)).toHaveAttribute('id', chosen);
  await expect(ghost).toHaveAttribute('data-position', at);

  // Enter on a spot that takes the tile two ways opens the card, its first
  // option focused and shown by the ghost.
  await page.keyboard.press('Enter');
  const options = page.locator('.orientation-option');
  await expect(options).toHaveCount(2);
  await expect(options.first()).toBeFocused();
  const [one, two] = await options.evaluateAll((buttons) => buttons.map((b) => b.getAttribute('data-rotation')!));
  await expect(ghost).toHaveAttribute('data-rotation', one);

  // Escape steps back: to the spot, still active…
  await page.keyboard.press('Escape');
  await expect(options).toHaveCount(0);
  await expect(spots).toBeFocused();
  await expect(spots).toHaveAttribute('aria-activedescendant', chosen);
  // …then to the tile, no longer selected.
  await page.keyboard.press('Escape');
  await expect(focusedTile(page)).toHaveCount(1);
  await expect(page.locator('.player-hand.active .hand-tile.selected')).toHaveCount(0);

  // Once more, and the other way round: → turns the tile, Enter plays it.
  await page.keyboard.press('Enter');
  await expect(spots).toBeFocused();
  await page.keyboard.press('ArrowRight');
  await expect(spots).toHaveAttribute('aria-activedescendant', chosen);
  await page.keyboard.press('Enter');
  await expect(options.first()).toBeFocused();
  await page.keyboard.press('ArrowRight');
  await expect(options.nth(1)).toBeFocused();
  await expect(ghost).toHaveAttribute('data-rotation', two);
  await page.keyboard.press('Enter');

  await expect(placedTiles(page)).toHaveCount(2);
  await expect(page.locator(`.board-tile:not(.tile-preview)[data-position="${at}"]`)).toHaveAttribute(
    'data-rotation',
    two,
  );
  // Red's turn, from Red's tiles.
  await expect(mover(page)).toHaveText('Red');
  await expect(focusedTile(page)).toHaveCount(1);
});

test('a held key takes one step, and keys reach the game with nothing focused', async ({ page }) => {
  await openApp(page, '/game?seed=5');
  await page.getByRole('button', { name: 'Start Game' }).focus();
  await page.keyboard.press('Enter');
  await page.keyboard.press('ArrowRight');
  await page.keyboard.press('Enter');
  await expect(placedTiles(page)).toHaveCount(1);
  await expect(focusedTile(page)).toHaveCount(1);

  // Holding Enter on Blue's tile takes it once: the repeats do not run on
  // into the spots and the card.
  await page.keyboard.down('Enter');
  await page.keyboard.down('Enter');
  await page.keyboard.down('Enter');
  await page.keyboard.up('Enter');
  const spots = page.getByRole('listbox', { name: 'Where to place the tile' });
  await expect(spots).toBeFocused();
  await expect(page.locator('.orientation-option')).toHaveCount(0);
  await expect(placedTiles(page)).toHaveCount(1);

  // With nothing focused, Escape still steps back…
  const blur = () => page.evaluate(() => (document.activeElement as HTMLElement | null)?.blur());
  await blur();
  await expect.poll(() => page.evaluate(() => document.activeElement === document.body)).toBe(true);
  await page.keyboard.press('Escape');
  await expect(page.locator('.player-hand.active .hand-tile.selected')).toHaveCount(0);
  // …and an arrow key hands the focus to the hand.
  await blur();
  await page.keyboard.press('ArrowRight');
  await expect(focusedTile(page)).toHaveCount(1);
});

test('the menu and the End Game dialog hold the focus and give it back', async ({ page }) => {
  // On a phone the menu is a popover.
  await page.setViewportSize({ width: 390, height: 844 });
  await startLocalGame(page);
  const toggle = page.getByRole('button', { name: 'Menu' });
  const items = page.locator('.hud-menu-items button:visible');

  // Opening the menu moves the focus to its first item; Tab goes round.
  await toggle.focus();
  await page.keyboard.press('Enter');
  await expect(items.first()).toBeFocused();
  await page.keyboard.press('Shift+Tab');
  await expect(items.last()).toBeFocused();
  await page.keyboard.press('Tab');
  await expect(items.first()).toBeFocused();
  // Escape closes it, and the focus goes back to the menu button.
  await page.keyboard.press('Escape');
  await expect(page.locator('.hud-menu-items')).toBeHidden();
  await expect(toggle).toBeFocused();

  // End Game, from the keyboard: the dialog takes the focus on Cancel…
  await page.keyboard.press('Enter');
  const endGame = page.getByRole('button', { name: 'End Game', exact: true });
  for (let i = 0; i < 8 && !(await endGame.evaluate((b) => b === document.activeElement)); i++) {
    await page.keyboard.press('Tab');
  }
  await expect(endGame).toBeFocused();
  await page.keyboard.press('Enter');
  const dialog = page.getByRole('dialog');
  const cancel = dialog.getByRole('button', { name: 'Cancel' });
  const confirm = dialog.getByRole('button', { name: 'Yes, end it' });
  await expect(cancel).toBeFocused();
  // …and keeps it: Tab and Shift+Tab go round its two buttons.
  await page.keyboard.press('Tab');
  await expect(confirm).toBeFocused();
  await page.keyboard.press('Tab');
  await expect(cancel).toBeFocused();
  await page.keyboard.press('Shift+Tab');
  await expect(confirm).toBeFocused();
  // Escape cancels; End Game sat in the closed menu, so ⋯ gets the focus.
  await page.keyboard.press('Escape');
  await expect(dialog).toHaveCount(0);
  await expect(toggle).toBeFocused();
  await expect(page.locator('.results-row')).toHaveCount(0);
});
