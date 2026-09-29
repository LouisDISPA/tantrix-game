import { expect, test } from '@playwright/test';
import { openApp } from './open-app';

test('rejects an invalid join code', async ({ page }) => {
  await openApp(page);
  await page.getByRole('button', { name: 'Online' }).click();

  await page.getByLabel('Join code').fill('BAD');
  await page.getByRole('button', { name: 'Join room' }).click();

  await expect(page.getByText('Room codes use 6 letters and exclude I, O, and Q.')).toBeVisible();
});

test('starts an online game with two browsers', async ({ page, browser }) => {
  await openApp(page);
  await page.getByRole('button', { name: 'Online' }).click();
  await page.getByRole('button', { name: 'Host a new room' }).click();
  await expect(page.getByRole('heading', { name: 'LOBBY' })).toBeVisible({ timeout: 20_000 });

  const code = new URL(page.url()).pathname.match(/^\/online\/([A-Z]{6})\/host$/)?.[1];
  expect(code).toBeDefined();

  const guestContext = await browser.newContext();
  try {
    const guest = await guestContext.newPage();
    await guest.goto(`/join/${code}`);
    await expect(guest.getByRole('heading', { name: 'LOBBY' })).toBeVisible({ timeout: 20_000 });

    await page.getByRole('button', { name: /Red.*Open/ }).click();
    await guest.getByRole('button', { name: /Blue.*Open/ }).click();

    const startGame = page.getByRole('button', { name: 'Start game' });
    await expect(startGame).toBeEnabled();
    await startGame.click();

    await expect(page.getByText('Select a tile from your hand')).toBeVisible();
    await expect(guest.getByText('Select a tile from your hand')).toBeVisible();
  } finally {
    await guestContext.close();
  }
});
