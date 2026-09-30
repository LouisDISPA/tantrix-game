import { expect, type Page } from '@playwright/test';

export async function openApp(page: Page) {
  await page.goto('/');
  // Server-rendered controls appear before their client-side handlers are attached.
  await expect(page.getByRole('button', { name: 'Local Game' })).toHaveAttribute(
    'data-dioxus-id',
    /\d+/,
  );
}

/** Open `/online` and wait until its buttons are hydrated and clickable. */
export async function openOnline(page: Page) {
  await page.goto('/online');
  await expect(page.getByRole('button', { name: 'Create a room' })).toHaveAttribute(
    'data-dioxus-id',
    /\d+/,
  );
}
