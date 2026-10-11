import { expect, test } from '@playwright/test';

test('loading paints before Wasm and hydration unlocks the menu without moving it', async ({ page }) => {
  const stylesheetRequests: string[] = [];
  page.on('request', (request) => {
    if (request.resourceType() === 'stylesheet') stylesheetRequests.push(request.url());
  });
  let releaseWasm!: () => void;
  const download = new Promise<void>((resolve) => { releaseWasm = resolve; });
  await page.route(/\.wasm(?:\?|$)/, async (route) => {
    await download;
    await route.continue();
  });

  await page.goto('/', { waitUntil: 'domcontentloaded' });
  const styles = page.locator('head style#app-styles');
  await expect(styles).toHaveCount(1);
  const css = await styles.textContent();
  expect(css).toContain('.loading-ribbon');
  expect(css).toContain('.board-stage');
  await expect(page.locator('link[rel="stylesheet"]')).toHaveCount(0);
  await expect(page.locator('#loading-status')).toHaveText('Loading…');
  await expect(page.locator('#startup-loading')).toBeVisible();
  await expect(page.locator('.app-content')).toHaveAttribute('inert');
  await expect(page.locator('.app-root')).toHaveAttribute('data-app-ready', 'false');

  // Let the existing title entrance finish before comparing page geometry.
  await page.locator('.title-buttons').evaluate(async (element) => {
    await Promise.all(element.getAnimations().map((animation) => animation.finished));
  });
  const button = page.locator('.title-btn').filter({ hasText: 'Local Game' });
  const before = await button.boundingBox();
  expect(before).not.toBeNull();
  await page.mouse.click(before!.x + before!.width / 2, before!.y + before!.height / 2);
  await expect(page).toHaveURL(/\/$/);

  releaseWasm();
  await expect(page.locator('.app-root')).toHaveAttribute('data-app-ready', 'true');
  await expect(page.locator('.app-content')).not.toHaveAttribute('inert');
  await expect(page.locator('#startup-loading')).toBeHidden();
  await expect(styles).toHaveCount(1);
  expect(await styles.textContent()).toBe(css);
  // The native dev client may load Google Fonts; app CSS needs no request.
  expect(stylesheetRequests.filter((url) =>
    new URL(url).origin === new URL(page.url()).origin,
  )).toEqual([]);
  expect(await button.boundingBox()).toEqual(before);
  await page.getByRole('button', { name: 'Local Game', exact: true }).click();
  await expect(page).toHaveURL(/\/game/);
});

test('a failed Wasm download leaves a usable reload action', async ({ page }) => {
  await page.route(/\.wasm(?:\?|$)/, (route) => route.abort());
  await page.goto('/', { waitUntil: 'domcontentloaded' });
  await expect(page.locator('#loading-status')).toHaveText('Loading…');
  await expect(page.getByRole('link', { name: 'Reload', exact: true })).toBeVisible({ timeout: 20_000 });
  await expect(page.locator('.app-content')).toHaveAttribute('inert');
});

test('reduced motion keeps loading visible without a moving ribbon', async ({ page }) => {
  await page.emulateMedia({ reducedMotion: 'reduce' });
  let releaseWasm!: () => void;
  const download = new Promise<void>((resolve) => { releaseWasm = resolve; });
  await page.route(/\.wasm(?:\?|$)/, async (route) => {
    await download;
    await route.continue();
  });
  await page.goto('/', { waitUntil: 'domcontentloaded' });
  await expect(page.locator('#loading-status')).toHaveText('Loading…');
  expect(await page.locator('.loading-ribbon').evaluate((element) =>
    getComputedStyle(element).animationName,
  )).toBe('none');
  releaseWasm();
  await expect(page.locator('#startup-loading')).toBeHidden();
});
