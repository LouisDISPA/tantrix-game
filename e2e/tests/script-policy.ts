import type { Page } from '@playwright/test';

/** The production script policy (the Worker's): no `eval`, so code that
 * evaluates JavaScript at run time fails as it would in production. Returns
 * the page's uncaught errors, for the test to check. */
export async function useProductionScriptPolicy(page: Page) {
  const errors: string[] = [];
  page.on('pageerror', error => errors.push(error.message));
  await page.route('**/*', async route => {
    if (!route.request().isNavigationRequest()) return route.continue();
    const response = await route.fetch();
    await route.fulfill({ response, headers: {
      ...response.headers(),
      'content-security-policy': "script-src 'self' 'unsafe-inline' 'wasm-unsafe-eval'",
    } });
  });
  return errors;
}
