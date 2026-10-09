import type { Page } from '@playwright/test';

export type Point = { x: number; y: number };

/**
 * Real touch input for Chromium, through the DevTools protocol: Playwright's
 * `touchscreen` can only tap. Chrome turns these into pointer events of type
 * `touch`, as a phone does. The page's context needs `hasTouch: true`.
 */
async function touch(page: Page, run: (send: (type: string, points: Array<Point & { id: number }>) => Promise<void>) => Promise<void>) {
  const client = await page.context().newCDPSession(page);
  const send = async (type: string, points: Array<Point & { id: number }>) => {
    await client.send('Input.dispatchTouchEvent', { type, touchPoints: points });
  };
  try {
    await run(send);
  } finally {
    await client.detach();
  }
}

/** Drag one finger from `from` to `to` in `steps` moves. */
export async function touchDrag(page: Page, from: Point, to: Point, steps = 8) {
  await touch(page, async (send) => {
    await send('touchStart', [{ ...from, id: 1 }]);
    for (let i = 1; i <= steps; i++) {
      const t = i / steps;
      await send('touchMove', [{ x: from.x + (to.x - from.x) * t, y: from.y + (to.y - from.y) * t, id: 1 }]);
    }
    await send('touchEnd', []);
  });
}

/** Two fingers either side of `center`, `startGap` apart, moving to `endGap` apart. */
export async function touchPinch(page: Page, center: Point, startGap: number, endGap: number, steps = 8) {
  const fingers = (gap: number) => [
    { x: center.x - gap / 2, y: center.y, id: 1 },
    { x: center.x + gap / 2, y: center.y, id: 2 },
  ];
  await touch(page, async (send) => {
    await send('touchStart', fingers(startGap));
    for (let i = 1; i <= steps; i++) {
      await send('touchMove', fingers(startGap + ((endGap - startGap) * i) / steps));
    }
    await send('touchEnd', []);
  });
}

/** The camera as the board viewport's inline style carries it. */
export async function camera(page: Page) {
  const style = (await page.locator('.board-viewport').getAttribute('style')) ?? '';
  const read = (name: string) => Number(new RegExp(`--${name}:\\s*(-?[\\d.]+)`).exec(style)?.[1]);
  return { zoom: read('zoom'), panX: read('pan-x'), panY: read('pan-y') };
}

export function centreOf(box: { x: number; y: number; width: number; height: number }): Point {
  return { x: box.x + box.width / 2, y: box.y + box.height / 2 };
}

/** A finger that stays down until it is told to lift: for pressing and holding. */
export async function holdTouch(page: Page, at: Point) {
  const client = await page.context().newCDPSession(page);
  const send = (type: string, points: Array<Point & { id: number }>) =>
    client.send('Input.dispatchTouchEvent', { type, touchPoints: points });
  await send('touchStart', [{ ...at, id: 1 }]);
  return {
    async moveTo(to: Point) {
      await send('touchMove', [{ ...to, id: 1 }]);
    },
    async lift() {
      await send('touchEnd', []);
      await client.detach();
    },
  };
}
