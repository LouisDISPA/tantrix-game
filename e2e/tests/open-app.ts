import { expect, type Page } from '@playwright/test';

export async function openApp(page: Page) {
  await page.goto('/');
  // Server-rendered controls appear before their client-side handlers are attached.
  await expect(page.getByRole('button', { name: 'Local Game' })).toHaveAttribute(
    'data-dioxus-id',
    /\d+/,
  );
}
