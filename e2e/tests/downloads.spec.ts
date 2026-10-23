import { expect, test } from '@playwright/test';
import { openApp } from './open-app';

const releaseApi = 'https://api.github.com/repos/LouisDISPA/tantrix-game/releases/latest';
const assetBase = 'https://github.com/LouisDISPA/tantrix-game/releases/download/tantrix-app-v1.2.3/';

test('stable release downloads offer matching packages and preserve the selected link', async ({ page }) => {
  const names = ['Tantrix-1.2.3-macos-aarch64.dmg', 'Tantrix-1.2.3-android-aarch64.apk'];
  await page.route(releaseApi, (route) => route.fulfill({ json: {
    tag_name: 'tantrix-app-v1.2.3', draft: false, prerelease: false,
    assets: [
      ...names.map((name) => ({ name, browser_download_url: `${assetBase}${name}`, size: 1024 })),
      { name: 'Tantrix-1.2.3-windows-x86_64-setup.exe', browser_download_url: `${assetBase}different.exe`, size: 1024 },
    ],
  } }));
  await openApp(page, '/download');
  await expect(page.getByText('v1.2.3', { exact: true })).toBeVisible();
  const choices = page.getByRole('combobox', { name: 'Platform and architecture' });
  await expect(choices.locator('option')).toHaveCount(3);
  await choices.selectOption(names[1]);
  await expect(page.locator('[data-download]')).toHaveAttribute('href', `${assetBase}${names[1]}`);
});

test('an invalid release version shows the download fallback', async ({ page }) => {
  await page.route(releaseApi, (route) => route.fulfill({ json: {
    tag_name: 'tantrix-app-v1..2', draft: false, prerelease: false, assets: [],
  } }));
  await openApp(page, '/download');
  await expect(page.getByText('Downloads are unavailable right now.')).toBeVisible();
  await expect(page.locator('[data-download]')).toHaveCount(0);
  await expect(page.getByRole('link', { name: 'Releases & changelog' })).toBeVisible();
});
