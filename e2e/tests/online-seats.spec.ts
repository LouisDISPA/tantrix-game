import { expect, test, type Browser, type BrowserContext, type Page } from '@playwright/test';
import { attachAll, instrument, type Diagnostics } from './diagnostics';
import { openOnline } from './open-app';
import { placedTiles, playMove } from './play-move';

const lobby = (page: Page) => page.getByRole('heading', { name: 'LOBBY' });
const chip = (page: Page, color: string) => page.locator(`.lobby-color-chip.color-${color.toLowerCase()}`);

async function createRoom(page: Page): Promise<string> {
  await openOnline(page);
  await page.getByRole('button', { name: 'Create a room' }).click();
  await expect(lobby(page)).toBeVisible({ timeout: 20_000 });
  const code = new URL(page.url()).pathname.match(/^\/online\/([A-Z]{6})$/)?.[1] ?? '';
  expect(code).toMatch(/^[A-Z]{6}$/);
  return code;
}

/** A separate browser context is a separate person: its own session cookie. */
async function joinAsNewPerson(browser: Browser, code: string, label: string) {
  const context = await browser.newContext();
  const page = await context.newPage();
  const diag = await instrument(page, label);
  await page.goto(`/join/${code}`);
  await expect(lobby(page)).toBeVisible({ timeout: 20_000 });
  return { context, page, diag };
}

test('five people share four seats: a released seat goes to the fifth, the rest watch', async ({
  page,
  browser,
}, testInfo) => {
  test.setTimeout(120_000);
  const creatorDiag = await instrument(page, 'creator');
  const contexts: BrowserContext[] = [];
  const diags: Diagnostics[] = [creatorDiag];

  try {
    const code = await createRoom(page);
    const people = [page];
    for (const label of ['blue', 'yellow', 'green', 'fifth']) {
      const joined = await joinAsNewPerson(browser, code, label);
      contexts.push(joined.context);
      diags.push(joined.diag);
      people.push(joined.page);
    }
    const [red, blue, yellow, green, fifth] = people;

    const colors = ['Red', 'Blue', 'Yellow', 'Green'];
    for (const [i, color] of colors.entries()) {
      await people[i].getByRole('button', { name: new RegExp(`${color}.*Open`) }).click();
      await expect(chip(people[i], color)).toHaveClass(/mine/);
    }

    // All four seats are taken; the fifth person waits in the room.
    for (const color of colors) await expect(chip(fifth, color)).toHaveClass(/taken/);
    await expect(fifth.locator('.lobby-unseated')).toContainText('Waiting for a seat');
    await expect(red.locator('.lobby-unseated')).toContainText('Waiting for a seat');
    await expect(fifth.getByRole('button', { name: 'Pick a color' })).toBeDisabled();

    // Red leaves the seat and the fifth person takes it.
    await red.getByRole('button', { name: 'Release my seat' }).click();
    await expect(chip(fifth, 'Red')).toHaveClass(/free/);
    await fifth.getByRole('button', { name: /Red.*Open/ }).click();
    await expect(chip(fifth, 'Red')).toHaveClass(/mine/);
    await expect(chip(red, 'Red')).toHaveClass(/taken/);
    await expect(red.getByRole('button', { name: 'Pick a color' })).toBeDisabled();

    // The newly seated player can start; the unseated one stays to watch.
    await fifth.getByRole('button', { name: 'Start game' }).click();
    for (const seated of [fifth, blue, yellow, green]) {
      await expect(seated.locator('.hud-chip-phase')).toBeVisible();
    }
    const watcher = red;
    await expect(watcher.locator('.hud-chip-name')).toBeVisible();
    await expect(watcher.locator('.hand-tile')).toHaveCount(4 * 6);

    const byColor: Record<string, Page> = { Red: fifth, Blue: blue, Yellow: yellow, Green: green };
    const current = (await fifth.locator('.hud-chip-name').textContent())?.trim() ?? '';
    const count = await playMove(byColor[current]);
    for (const person of people) await expect(placedTiles(person)).toHaveCount(count);

    for (const diag of diags) expect(diag.problems()).toEqual([]);
  } catch (error) {
    await attachAll(testInfo, ...diags);
    throw error;
  } finally {
    for (const context of contexts) await context.close();
  }
});

test('the session cookie keeps the seat across a reload, and a second tab takes over', async ({
  page,
  browser,
}, testInfo) => {
  test.setTimeout(90_000);
  const creatorDiag = await instrument(page, 'creator');
  let guestContext: BrowserContext | undefined;
  try {
    const code = await createRoom(page);
    const guest = await joinAsNewPerson(browser, code, 'guest');
    guestContext = guest.context;

    await page.getByRole('button', { name: /Red.*Open/ }).click();
    await expect(chip(guest.page, 'Red')).toHaveClass(/taken/);

    // Reload: same cookie, same person, same seat.
    await page.reload();
    await expect(lobby(page)).toBeVisible({ timeout: 20_000 });
    await expect(chip(page, 'Red')).toHaveClass(/mine/);
    await expect(chip(guest.page, 'Red')).toHaveClass(/taken/);
    const session = (await page.context().cookies()).find((c) => c.name === 'tantrix_session');
    expect(session?.httpOnly).toBe(true);

    // The guest takes Blue, then drops: the seat is held for them, shown
    // offline (anyone could take it over), and the game can't start yet.
    await guest.page.getByRole('button', { name: /Blue.*Open/ }).click();
    await expect(chip(page, 'Blue')).toHaveClass(/taken/);
    await guest.page.close();
    await expect(chip(page, 'Blue')).toHaveClass(/offline/);
    await expect(page.getByRole('button', { name: 'Waiting for reconnection…' })).toBeDisabled();

    // Coming back with the same cookie reclaims the seat.
    const guestAgain = await guest.context.newPage();
    await guestAgain.goto(`/online/${code}`);
    await expect(chip(guestAgain, 'Blue')).toHaveClass(/mine/);
    await expect(chip(page, 'Blue')).toHaveClass(/taken/);

    await page.getByRole('button', { name: 'Start game' }).click();
    await expect(page.locator('.hud-chip-phase')).toBeVisible();

    await page.reload();
    await expect(page.locator('.hud-chip-phase')).toBeVisible({ timeout: 20_000 });
    await expect(page.locator('.hand-tile')).toHaveCount(12);

    // The same person opening the room in a second tab takes it over.
    const second = await page.context().newPage();
    await second.goto(`/online/${code}`);
    await expect(second.locator('.hud-chip-phase')).toBeVisible({ timeout: 20_000 });
    await expect(page.getByText('This room was opened in another tab.')).toBeVisible();

    expect(creatorDiag.problems()).toEqual([]);
  } catch (error) {
    await attachAll(testInfo, creatorDiag);
    throw error;
  } finally {
    await guestContext?.close();
  }
});
