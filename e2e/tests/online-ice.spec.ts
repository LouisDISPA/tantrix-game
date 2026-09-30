import { createSocket } from 'node:dgram';
import { expect, test } from '@playwright/test';
import { attachAll, instrument } from './diagnostics';
import { openApp } from './open-app';

/**
 * Regression test for the "Joining room…" hang.
 *
 * matchbox waits for ICE gathering to *complete* before it sends its SDP
 * offer/answer. If any configured STUN server never answers, Chromium keeps
 * gathering for ~40 s, so the guest is stuck on the connecting screen. Here the
 * ICE config of both browsers gets an unroutable STUN server appended, which
 * makes that stall deterministic instead of a ~1-in-40 network coincidence.
 */
test('guest reaches the lobby even though one STUN server never answers', async ({ page, browser }, testInfo) => {
  test.setTimeout(90_000);

  const hostDiag = await instrument(page, 'host', { deadStun: true });
  const guestContext = await browser.newContext();
  const guest = await guestContext.newPage();
  const guestDiag = await instrument(guest, 'guest', { deadStun: true });

  try {
    await openApp(page);
    await page.getByRole('button', { name: 'Online' }).click();
    await page.getByRole('button', { name: 'Host a new room' }).click();
    await expect(page.getByRole('heading', { name: 'LOBBY' })).toBeVisible({ timeout: 20_000 });
    const code = new URL(page.url()).pathname.match(/^\/online\/([A-Z]{6})\/host$/)?.[1] ?? '';

    await openApp(guest);
    await guest.getByRole('button', { name: 'Online' }).click();
    await guest.getByLabel('Join code').fill(code);
    await guest.getByRole('button', { name: 'Join room' }).click();

    const started = Date.now();
    await expect(guest.getByRole('heading', { name: 'LOBBY' })).toBeVisible({ timeout: 25_000 });
    testInfo.annotations.push({ type: 'time-to-lobby', description: `${Date.now() - started}ms` });
  } catch (error) {
    await attachAll(testInfo, hostDiag, guestDiag);
    throw error;
  } finally {
    await guestContext.close();
  }
});

/**
 * A STUN server that answers correctly, but only after `delayMs`: slower than the
 * bounded ICE-gathering wait, so the offer has to leave without the
 * server-reflexive candidate it will eventually produce.
 */
async function startSlowStun(delayMs: number, mapped = { ip: '203.0.113.7', port: 40000 }) {
  const socket = createSocket('udp4');
  const cookie = [0x21, 0x12, 0xa4, 0x42];
  // Chrome retransmits binding requests, so replies are still queued when the
  // test ends. A reply sent on a closed socket throws from a timer callback and
  // would be blamed on whichever test runs next, so cancel them on close.
  const pending = new Set<NodeJS.Timeout>();
  socket.on('message', (request, remote) => {
    const bindingRequest = 0x0001;
    if (request.length < 20 || request.readUInt16BE(0) !== bindingRequest) return;
    const timer = setTimeout(() => {
      pending.delete(timer);
      // XOR-MAPPED-ADDRESS (IPv4) telling the client it was seen at `mapped`.
      const attribute = Buffer.alloc(12);
      attribute.writeUInt16BE(0x0020, 0);
      attribute.writeUInt16BE(8, 2);
      attribute.writeUInt8(0x01, 5);
      attribute.writeUInt16BE(mapped.port ^ 0x2112, 6);
      mapped.ip.split('.').forEach((octet, i) => attribute.writeUInt8(Number(octet) ^ cookie[i], 8 + i));
      const header = Buffer.alloc(20);
      header.writeUInt16BE(0x0101, 0); // binding success response
      header.writeUInt16BE(attribute.length, 2);
      header.writeUInt32BE(0x2112a442, 4);
      request.copy(header, 8, 8, 20); // echo the transaction id
      socket.send(Buffer.concat([header, attribute]), remote.port, remote.address);
    }, delayMs);
    pending.add(timer);
  });
  await new Promise<void>((resolve) => socket.bind(0, '127.0.0.1', resolve));
  return {
    port: socket.address().port,
    close: () => {
      pending.forEach(clearTimeout);
      pending.clear();
      socket.close();
    },
  };
}

test('a candidate found after the gathering deadline is still trickled to the peer', async ({
  page,
  browser,
}, testInfo) => {
  test.setTimeout(90_000);

  const stun = await startSlowStun(5_000);
  const url = `stun:127.0.0.1:${stun.port}`;
  const hostDiag = await instrument(page, 'host', { extraStun: url });
  const guestContext = await browser.newContext();
  const guest = await guestContext.newPage();
  const guestDiag = await instrument(guest, 'guest', { extraStun: url });

  // On a LAN the peers connect from their first host candidates, long before a
  // slow STUN server answers, so a late candidate would never matter. Holding
  // back the guest's Answer keeps the host waiting for it (offer sent at the 3 s
  // gathering deadline, answer at ~8 s) while the 5 s candidate turns up.
  // Forwarding must tolerate a route that closed in the meantime: teardown closes
  // both ends at once, and a throw from these callbacks would be reported against
  // whichever test happens to run next.
  const quietly = (action: () => void) => {
    try {
      action();
    } catch {
      /* route already closed */
    }
  };
  await guest.routeWebSocket(/\/signaling\//, (ws) => {
    const server = ws.connectToServer();
    ws.onMessage((message) => {
      if (typeof message === 'string' && message.includes('"Answer"')) {
        setTimeout(() => quietly(() => server.send(message)), 4_000);
      } else {
        quietly(() => server.send(message));
      }
    });
    server.onMessage((message) => quietly(() => ws.send(message)));
    ws.onClose((code, reason) => quietly(() => void server.close({ code, reason })));
    server.onClose((code, reason) => quietly(() => void ws.close({ code, reason })));
  });

  try {
    await openApp(page);
    await page.getByRole('button', { name: 'Online' }).click();
    await page.getByRole('button', { name: 'Host a new room' }).click();
    await expect(page.getByRole('heading', { name: 'LOBBY' })).toBeVisible({ timeout: 20_000 });
    const code = new URL(page.url()).pathname.match(/^\/online\/([A-Z]{6})\/host$/)?.[1] ?? '';

    await openApp(guest);
    await guest.getByRole('button', { name: 'Online' }).click();
    await guest.getByLabel('Join code').fill(code);
    await guest.getByRole('button', { name: 'Join room' }).click();
    await expect(guest.getByRole('heading', { name: 'LOBBY' })).toBeVisible({ timeout: 30_000 });

    // The 5 s server-reflexive candidate must go out as a trickled IceCandidate
    // after the offer (sent at the 3 s deadline) and *before* the answer arrives.
    await hostDiag.collect();
    const events = hostDiag.events().sort((a, b) => a.at - b.at);
    const at = (pattern: RegExp) => events.findIndex((e) => pattern.test(e.text));
    const offer = at(/^ws-send \/signaling\/\S+ Signal Offer/);
    const late = at(/^ws-send \/signaling\/\S+ Signal IceCandidate srflx\/udp@203\.0\.113\.7/);
    const answer = at(/^ws-recv \/signaling\/\S+ Signal Answer/);
    expect(offer, 'host never sent an offer').toBeGreaterThanOrEqual(0);
    expect(late, 'late srflx candidate was never trickled').toBeGreaterThan(offer);
    expect(answer, 'answer should arrive after the late candidate was sent').toBeGreaterThan(late);
  } catch (error) {
    await attachAll(testInfo, hostDiag, guestDiag);
    throw error;
  } finally {
    stun.close();
    await guestContext.close();
  }
});
