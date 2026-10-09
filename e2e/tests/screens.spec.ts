import { mkdirSync } from 'node:fs';
import path from 'node:path';
import { test, type Page } from '@playwright/test';
import { openApp } from './open-app';
import { openOrientationCard, playMove, startLocalGame } from './play-move';

/**
 * Screenshots of the real game screen, for looking at (not asserting). Opt in:
 *
 *   TANTRIX_SCREENSHOTS=1 pnpm exec playwright test tests/screens.spec.ts
 *
 * They land in `e2e/screenshots/<size>-<players>p-<theme>-<step>.png` (not in test-results, which Playwright
 * empties on every run).
 */
test.skip(!process.env.TANTRIX_SCREENSHOTS, 'set TANTRIX_SCREENSHOTS=1 to take screenshots');

const out = path.join(__dirname, '..', 'screenshots');
mkdirSync(out, { recursive: true });

const sizes = [
  { name: 'iphone', width: 390, height: 844, touch: true },
  { name: 'small', width: 360, height: 640, touch: true },
  { name: 'landscape', width: 844, height: 390, touch: true },
  { name: 'tablet', width: 768, height: 1024, touch: true },
  { name: 'laptop', width: 1280, height: 720, touch: false },
  { name: 'desktop', width: 1920, height: 1080, touch: false },
  { name: 'small-laptop', width: 800, height: 600, touch: false },
];

async function shoot(page: Page, name: string) {
  // The board eases for 0.4s after a move: let it settle before freezing the page.
  await page.waitForTimeout(500);
  await page.addStyleTag({ content: '*,*::before,*::after{animation:none!important;transition:none!important}' });
  await page.waitForTimeout(150);
  await page.screenshot({ path: path.join(out, `${name}.png`) });
}

for (const size of sizes) {
  for (const players of [2, 4]) {
    test.describe(`${size.name} ${players}p`, () => {
      test.use({
        viewport: { width: size.width, height: size.height },
        hasTouch: size.touch,
        isMobile: size.touch,
        deviceScaleFactor: size.touch ? 2 : 1,
      });

      test('the turn', async ({ page }) => {
        const tag = `${size.name}-${players}p`;
        await openApp(page);
        await startLocalGame(page, players);
        for (let move = 0; move < 6; move++) await playMove(page);
        await shoot(page, `${tag}-dark-1-pick`);

        await page.locator('.player-hand:not(.active) .hand-header').first().click();
        await shoot(page, `${tag}-dark-2-peek`);
        await page.locator('.player-hand.active .hand-header').click();

        await openOrientationCard(page);
        await page.mouse.move(1, 1);
        await shoot(page, `${tag}-dark-3-card`);

        // Close the card, end the game: the results take the panel's place.
        await page.keyboard.press('Escape');
        const toggle = page.locator('.hud-menu-toggle');
        if (await toggle.isVisible()) await toggle.click();
        await page.locator('.end-game-btn').click();
        await page.getByRole('dialog').getByRole('button', { name: 'Yes, end it' }).click();
        await page.locator('.results-row').first().waitFor();
        await page.mouse.move(1, 1);
        await shoot(page, `${tag}-dark-4-results`);
      });
    });
  }
}

// The light theme, at the two sizes that matter most.
for (const size of sizes.filter((s) => ['iphone', 'laptop'].includes(s.name))) {
  test.describe(`${size.name} light`, () => {
    test.use({
      viewport: { width: size.width, height: size.height },
      hasTouch: size.touch,
      isMobile: size.touch,
      deviceScaleFactor: size.touch ? 2 : 1,
    });

    test('the turn', async ({ page }) => {
      const tag = `${size.name}-3p-light`;
      await openApp(page);
      await startLocalGame(page, 3);
      for (let move = 0; move < 6; move++) await playMove(page);
      const toggle = page.locator('.hud-menu-toggle');
      if (await toggle.isVisible()) await toggle.click();
      await page.locator('.hud-menu-item').click();
      await page.mouse.move(1, 1);
      await shoot(page, `${tag}-1-pick`);
      await openOrientationCard(page);
      await page.mouse.move(1, 1);
      await shoot(page, `${tag}-3-card`);
    });
  });
}
