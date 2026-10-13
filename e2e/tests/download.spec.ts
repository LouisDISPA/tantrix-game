import { expect, test } from '@playwright/test';
import { openApp } from './open-app';

const releaseApi = 'https://api.github.com/repos/LouisDISPA/tantrix-game/releases/latest';
const release = {
  tag_name: 'tantrix-app-v0.1.0', draft: false, prerelease: false,
  assets: ['macos-aarch64.dmg', 'macos-x86_64.dmg', 'windows-x86_64-setup.exe', 'linux-x86_64.AppImage'].map((suffix) => ({
    name: `Tantrix-0.1.0-${suffix}`, size: 3 * 1024 * 1024,
    browser_download_url: `https://github.com/LouisDISPA/tantrix-game/releases/download/tantrix-app-v0.1.0/Tantrix-0.1.0-${suffix}`,
  })),
};

test('SSR reserves the complete download layout with skeletons', async ({ browser }) => {
  const context = await browser.newContext({ javaScriptEnabled: false });
  const page = await context.newPage();
  await page.goto('/download');
  await expect(page.getByRole('button', { name: 'Back', exact: false })).toBeDisabled();
  await expect(page.getByRole('combobox')).toBeHidden();
  await expect(page.locator('[data-download]')).toBeHidden();
  await expect(page.getByRole('button', { name: 'Install the web app', exact: true })).toBeHidden();
  await expect(page.locator('.download-skeleton-action')).toHaveCount(3);
  await expect(page.locator('.download-content')).toHaveAttribute('inert', /.+/);
  await expect(page.getByRole('link', { name: 'Releases & changelog' })).toBeAttached();
  await expect(page.locator('details')).toHaveCount(0);
  await context.close();
});

test('menu pill opens latest downloads, preserves theme, and returns to menu', async ({ page }) => {
  await page.route(releaseApi, (route) => route.fulfill({ json: release }));
  await openApp(page);
  await page.getByRole('button', { name: 'Switch to light theme' }).click();
  await page.getByRole('link', { name: 'Get the app' }).click();
  await expect(page).toHaveURL(/\/download$/);
  await expect(page.getByRole('heading', { name: 'Get Tantrix' })).toBeVisible();
  await expect(page.locator('.app-root')).toHaveAttribute('data-theme', 'light');
  await expect(page.getByText('v0.1.0')).toBeVisible();
  await page.getByRole('combobox').selectOption('Tantrix-0.1.0-macos-aarch64.dmg');
  await expect(page.locator('[data-download]')).toHaveAttribute('href', /macos-aarch64\.dmg$/);
  await expect(page.getByRole('link', { name: 'Releases & changelog' })).toBeVisible();
  expect(await page.locator('.download-release-links').evaluate((element) => !!(element.compareDocumentPosition(document.querySelector('[data-web]')!) & Node.DOCUMENT_POSITION_FOLLOWING))).toBe(true);
  await page.getByRole('button', { name: 'Back', exact: false }).click();
  await expect(page.getByRole('link', { name: 'Get the app' })).toBeVisible();
  await expect(page.locator('.app-root')).toHaveAttribute('data-theme', 'light');
});

test('unknown Mac architecture requires a choice; mobile shows short install instructions', async ({ browser }) => {
  for (const device of [
    { userAgent: 'Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/605.1.15 Version/17.0 Safari/605.1.15', mobile: false },
    { userAgent: 'Mozilla/5.0 (iPhone; CPU iPhone OS 18_0 like Mac OS X) AppleWebKit/605.1.15 Version/18.0 Mobile/15E148 Safari/604.1', mobile: true },
  ]) {
    const context = await browser.newContext({ userAgent: device.userAgent, viewport: { width: 320, height: 740 } });
    await context.addInitScript(() => Object.defineProperty(navigator, 'userAgentData', { value: undefined }));
    const page = await context.newPage();
    await page.route(releaseApi, (route) => route.fulfill({ json: release }));
    await openApp(page, '/download');
    await expect(page.getByText('v0.1.0')).toBeVisible();
    if (device.mobile) {
      await expect(page.getByText(/Safari: Share → Add to Home Screen/)).toBeVisible();
      await expect(page.locator('[data-download]')).toBeHidden();
      await expect(page.getByText('Not available for this device.', { exact: true })).toBeVisible();
    } else {
      await expect(page.getByRole('combobox')).toHaveValue('');
      await expect(page.locator('[data-download]')).toBeHidden();
      await page.getByRole('combobox').selectOption('Tantrix-0.1.0-macos-x86_64.dmg');
      await expect(page.locator('[data-download]')).toHaveAttribute('href', /macos-x86_64\.dmg$/);
    }
    expect(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth)).toBe(true);
    await context.close();
  }
});

test('release failure keeps a usable fallback', async ({ page }) => {
  await page.route(releaseApi, (route) => route.fulfill({ status: 503, body: 'Unavailable' }));
  await openApp(page, '/download');
  await expect(page.getByText('Downloads are unavailable right now.')).toBeVisible();
  await expect(page.locator('[data-download]')).toBeHidden();
  await expect(page.getByRole('link', { name: 'Releases & changelog' })).toHaveAttribute('href', /releases\/latest$/);
});

test('skeleton, detected download, and unavailable message keep the same card size', async ({ browser }) => {
  for (const { detected, failed, width } of [
    { detected: true, failed: false },
    { detected: false, failed: false },
    { detected: true, failed: true },
  ].flatMap((scenario) => [320, 1280].map((width) => ({ ...scenario, width })))) {
    const context = await browser.newContext({
      viewport: { width, height: 900 },
      userAgent: detected ? 'Mozilla/5.0 (Windows NT 10.0; Win64; x64) Chrome/130.0' : 'Unknown browser',
      reducedMotion: 'reduce',
    });
    await context.addInitScript((detected) => Object.defineProperty(navigator, 'userAgentData', {
      value: detected ? { getHighEntropyValues: async () => ({ architecture: 'x86', bitness: '64' }) } : undefined,
    }), detected);
    const page = await context.newPage();
    let finish!: () => void;
    const gate = new Promise<void>((resolve) => { finish = resolve; });
    await page.route(releaseApi, async (route) => {
      await gate;
      await route.fulfill(failed ? { status: 503, body: 'Unavailable' } : { json: release });
    });
    await openApp(page, '/download');
    await expect(page.locator('[data-native]')).toHaveAttribute('aria-busy', 'true');
    const before = await page.locator('.menu-card').boundingBox();
    const ssrContext = await browser.newContext({ javaScriptEnabled: false, reducedMotion: 'reduce', viewport: { width, height: 900 } });
    const ssrPage = await ssrContext.newPage();
    await ssrPage.goto('/download');
    const ssr = await ssrPage.locator('.menu-card').boundingBox();
    expect(before!.height).toBeCloseTo(ssr!.height, 0);
    await ssrContext.close();
    finish();
    await expect(page.locator('[data-native]')).toHaveAttribute('aria-busy', 'false');
    if (detected && !failed) await expect(page.locator('[data-download]')).toBeVisible();
    else {
      await expect(page.locator('[data-download]')).toBeHidden();
      await expect(page.getByText(failed ? 'Downloads are unavailable right now.' : 'Not available for this device.', { exact: true })).toBeVisible();
    }
    const after = await page.locator('.menu-card').boundingBox();
    expect(after!.height).toBeCloseTo(before!.height, 0);
    expect(after!.width).toBeCloseTo(before!.width, 0);
    await context.close();
  }
});

test('available PWA prompt is used once and disappears after acceptance', async ({ page }) => {
  await page.route(releaseApi, (route) => route.fulfill({ json: release }));
  await openApp(page);
  await page.evaluate(() => {
    const event = new Event('beforeinstallprompt', { cancelable: true });
    Object.assign(event, {
      prompt: async () => { document.documentElement.setAttribute('data-install-prompted', 'true'); },
      userChoice: Promise.resolve({ outcome: 'accepted' }),
    });
    window.dispatchEvent(event);
  });
  await page.getByRole('link', { name: 'Get the app' }).click();
  await page.getByRole('button', { name: 'Install the web app', exact: true }).click();
  await expect(page.locator('html')).toHaveAttribute('data-install-prompted', 'true');
  await expect(page.getByText('Already installed.')).toBeVisible();
  await expect(page.getByRole('button', { name: 'Install the web app', exact: true })).toBeHidden();
  await page.getByRole('button', { name: 'Back', exact: false }).click();
  await page.getByRole('link', { name: 'Get the app' }).click();
  await expect(page.getByText('Already installed.')).toBeVisible();
});

test('late architecture hints preserve an explicit download choice', async ({ browser }) => {
  const context = await browser.newContext({
    userAgent: 'Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 Chrome/130.0 Safari/537.36',
  });
  await context.addInitScript(() => {
    Object.defineProperty(navigator, 'userAgentData', {
      value: {
        getHighEntropyValues: () => new Promise((resolve) => {
          Object.assign(window, { resolveDevice: () => resolve({ architecture: 'arm', bitness: '64' }) });
        }),
      },
    });
  });
  const page = await context.newPage();
  await page.route(releaseApi, (route) => route.fulfill({ json: release }));
  await openApp(page, '/download');
  await expect(page.getByRole('combobox')).toHaveValue('');
  await page.getByRole('combobox').selectOption('Tantrix-0.1.0-macos-x86_64.dmg');
  await page.evaluate(() => (window as unknown as { resolveDevice: () => void }).resolveDevice());
  await expect(page.locator('[data-download]')).toHaveAttribute('href', /macos-x86_64\.dmg$/);
  await context.close();
});
