import { expect, test, type Page } from '@playwright/test';
import { openApp } from './open-app';

/** Computed colors: an unresolvable `var()` leaves a custom property empty. */
function themeColors(page: Page) {
  return page.evaluate(() => {
    const root = getComputedStyle(document.documentElement);
    return {
      body: getComputedStyle(document.body).backgroundColor,
      app: getComputedStyle(document.querySelector('.app-root')!).backgroundColor,
      glows: ['red', 'blue', 'yellow', 'green'].map((c) => root.getPropertyValue(`--glow-${c}`).trim()),
    };
  });
}

test('the page background and color glows resolve in both themes', async ({ page }) => {
  await openApp(page);

  const first = await themeColors(page);
  expect(first.glows.every((glow) => glow !== '')).toBe(true);
  expect(first.body).toBe(first.app);

  await page.locator('.theme-toggle').click();
  await expect.poll(async () => (await themeColors(page)).app).not.toBe(first.app);

  const second = await themeColors(page);
  expect(second.glows.every((glow) => glow !== '')).toBe(true);
  expect(second.body).toBe(second.app);
});
