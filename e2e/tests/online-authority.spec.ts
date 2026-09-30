import { expect, test, type Page } from '@playwright/test';

type Message = Record<string, any>;

/**
 * Open a raw room socket from `page`. Identity comes from that browser
 * context's session cookie, so each player needs its own context.
 */
async function connect(page: Page, code: string, create: boolean): Promise<void> {
  await page.evaluate(
    async ({ code, create }) => {
      const scheme = location.protocol === 'https:' ? 'wss:' : 'ws:';
      const socket = new WebSocket(`${scheme}//${location.host}/room/${code}${create ? '?create=1' : ''}`);
      const messages: Record<string, any>[] = [];
      socket.addEventListener('message', (event) =>
        messages.push(event.data === 'pong' ? { Pong: true } : JSON.parse(event.data)));
      await new Promise<void>((resolve, reject) => {
        socket.addEventListener('open', () => resolve(), { once: true });
        socket.addEventListener('error', () => reject(new Error('room socket failed')), { once: true });
      });
      socket.send(JSON.stringify({ Hello: { nickname: create ? 'Creator' : 'Guest' } }));
      (window as any).room = { socket, messages };
    },
    { code, create },
  );
}

/** Send a protocol message (JSON-encoded, so `'StartGame'` becomes `"StartGame"`). */
async function send(page: Page, message: unknown): Promise<void> {
  await sendRaw(page, JSON.stringify(message));
}

async function sendRaw(page: Page, text: string): Promise<void> {
  await page.evaluate((text) => (window as any).room.socket.send(text), text);
}

/** Messages received so far. */
async function messages(page: Page): Promise<Message[]> {
  return page.evaluate(() => (window as any).room.messages);
}

async function until(page: Page, predicate: (message: Message) => boolean): Promise<Message> {
  const deadline = Date.now() + 15_000;
  let received: Message[] = [];
  while (Date.now() < deadline) {
    received = await messages(page);
    const found = received.find(predicate);
    if (found) return found;
    await page.waitForTimeout(50);
  }
  throw new Error(`timed out waiting for a room message; received ${JSON.stringify(received)}`);
}

/** Exercise the room protocol directly so browser click guards cannot mask a server bug. */
test('server rejects out-of-turn and invalid moves without changing the board', async ({ page, browser }) => {
  const guestContext = await browser.newContext();
  const guest = await guestContext.newPage();
  try {
    await page.goto('/');
    await guest.goto('/');
    const code = await page.evaluate(() => {
      const alphabet = 'ABCDEFGHJKLMNPRSTUVWXYZ';
      return Array.from(crypto.getRandomValues(new Uint8Array(6)), (v) => alphabet[v % alphabet.length]).join('');
    });
    await connect(page, code, true);
    await until(page, (m) => 'Welcome' in m);
    await connect(guest, code, false);
    await until(guest, (m) => 'Welcome' in m);

    await send(page, { PickColor: { color: 'Red' } });
    await send(guest, { PickColor: { color: 'Blue' } });
    await until(page, (m) => {
      const participants = m.LobbyUpdate?.lobby?.participants;
      return Array.isArray(participants) &&
        participants.some((p: Message) => p.seated_color === 'Red') &&
        participants.some((p: Message) => p.seated_color === 'Blue');
    });
    // Picking a held seat is refused (only an explicit ReplaceSeat takes it),
    // so two players racing for one free seat can't replace each other.
    await send(guest, { PickColor: { color: 'Red' } });
    await until(guest, (m) => m.Error?.code === 'ColorTaken');

    await send(page, 'StartGame');
    const started = await until(page, (m) => 'GameStarted' in m);
    const current = started.GameStarted.snapshot.current_player as 'Red' | 'Blue';
    const [right, wrong] = current === 'Red' ? [page, guest] : [guest, page];
    // A placement carries no `forced` flag: the server's board decides it.
    const invalidPlacement = { tile: 'RYYBRB', position: { x: 99, y: 99 }, rotation: 'None' };

    await send(wrong, { PlayMove: { placement: invalidPlacement } });
    await until(wrong, (m) => m.Error?.code === 'NotYourTurn');
    await send(right, { PlayMove: { placement: invalidPlacement } });
    await until(right, (m) => m.Error?.code === 'InvalidMove');

    const before = (await messages(right)).filter((m) => 'GameStarted' in m).length;
    await send(right, 'RequestSnapshot');
    await expect
      .poll(async () => (await messages(right)).filter((m) => 'GameStarted' in m).length)
      .toBeGreaterThan(before);
    const snapshot = (await messages(right)).filter((m) => 'GameStarted' in m).at(-1)!;
    expect(snapshot.GameStarted.snapshot.board.moves).toHaveLength(0);

    // Keepalives are answered by the runtime even mid-game.
    await sendRaw(right, 'ping');
    await until(right, (m) => m.Pong === true);

    // The private identity is an HttpOnly cookie: script can't read it and
    // no broadcast ever carries it.
    for (const context of [page.context(), guestContext]) {
      const session = (await context.cookies()).find((c) => c.name === 'tantrix_session');
      expect(session?.httpOnly).toBe(true);
      expect(session?.sameSite).toBe('Lax');
      for (const received of [...(await messages(page)), ...(await messages(guest))]) {
        expect(JSON.stringify(received)).not.toContain(session!.value);
      }
    }
    expect(await page.evaluate(() => document.cookie)).not.toContain('tantrix_session');
  } finally {
    await guestContext.close();
  }
});

test('room sockets from another site are refused', async ({ page }) => {
  await page.goto('/online');
  await page.getByRole('button', { name: 'Create a room' }).click();
  await expect(page.getByRole('heading', { name: 'LOBBY' })).toBeVisible();
  const roomUrl = new URL(page.url());
  const code = roomUrl.pathname.split('/').at(-1);
  expect(code).toMatch(/^[A-Z]{6}$/);

  // This document has a foreign Origin while its browser context still has
  // the room cookie. The Dioxus proxy may answer the upgrade itself, so test
  // that the room never accepts Hello.
  const attacker = await page.context().newPage();
  try {
    await attacker.route('http://evil.example/', (route) =>
      route.fulfill({ contentType: 'text/html', body: '<!doctype html><title>Other site</title>' }));
    await attacker.goto('http://evil.example/');
    const wsUrl = `${roomUrl.protocol === 'https:' ? 'wss:' : 'ws:'}//${roomUrl.host}/room/${code}`;
    const result = await attacker.evaluate((url) => new Promise<{ accepted: boolean; closed: boolean }>((resolve) => {
      const socket = new WebSocket(url);
      let accepted = false;
      let done = false;
      const finish = (closed: boolean) => {
        if (!done) {
          done = true;
          resolve({ accepted, closed });
        }
      };
      socket.addEventListener('open', () =>
        socket.send(JSON.stringify({ Hello: { nickname: 'Cross-origin visitor' } })));
      socket.addEventListener('message', (event) => {
        if (event.data.includes('Welcome')) {
          accepted = true;
          socket.close();
          finish(false);
        }
      });
      socket.addEventListener('close', () => finish(true));
      setTimeout(() => {
        socket.close();
        finish(false);
      }, 5000);
    }), wsUrl);
    expect(result).toEqual({ accepted: false, closed: true });
  } finally {
    await attacker.close();
  }
});
