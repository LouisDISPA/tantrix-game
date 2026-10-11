import { expect, type Page } from '@playwright/test';

/** SSR controls become interactive only after the client mounts. */
export async function openApp(page: Page, url = '/') {
  await page.goto(url);
  await expect(page.locator('.app-root')).toHaveAttribute('data-app-ready', 'true', {
    timeout: 30_000,
  });
}
