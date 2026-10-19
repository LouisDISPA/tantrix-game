import { expect, test } from '@playwright/test';
import { openApp } from './open-app';

for (const route of ['/', '/online', '/download']) {
  test(`hydrates a direct visit to ${route}`, async ({ page }) => {
    const errors: string[] = [];
    page.on('pageerror', (error) => errors.push(error.message));

    await openApp(page, route);

    await expect(page.locator('.app-content')).toHaveJSProperty('inert', false);
    await expect(page.locator('#startup-loading')).toHaveAttribute('aria-hidden', 'true');
    expect(errors).toEqual([]);
  });
}
