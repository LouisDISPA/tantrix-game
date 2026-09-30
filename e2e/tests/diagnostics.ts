import { writeFile } from 'node:fs/promises';
import type { Page, TestInfo } from '@playwright/test';

/**
 * Runtime instrumentation for the online multiplayer tests.
 *
 * Every browser page gets:
 *  - console / pageerror / failed-request capture,
 *  - signaling WebSocket frame capture (Playwright's `websocket` event),
 *  - an init script that wraps `RTCPeerConnection` so ICE / connection state
 *    changes and every data-channel open / close / error / message are logged
 *    into `window.__rtc` (read back with `collect()`).
 *
 * The timeline is attached to the test on failure so the network + runtime
 * behaviour leading up to it can be read without re-running.
 */

const RTC_HOOK = () => {
  interface Entry {
    t: number;
    kind: string;
    detail?: unknown;
  }
  const w = window as unknown as { __rtc: Entry[]; __rtcStats: Record<string, number> };
  if (w.__rtc) return;
  w.__rtc = [];
  w.__rtcStats = { sent: 0, received: 0, sentBytes: 0, receivedBytes: 0 };
  const log = (kind: string, detail?: unknown) =>
    w.__rtc.push({ t: Date.now(), kind, detail });

  const Native = window.RTCPeerConnection;
  if (!Native) return;

  const size = (data: unknown) =>
    typeof data === 'string'
      ? data.length
      : data instanceof ArrayBuffer
        ? data.byteLength
        : ArrayBuffer.isView(data)
          ? data.byteLength
          : 0;

  const watchChannel = (channel: RTCDataChannel, origin: string) => {
    const id = `${origin}:${channel.label}`;
    log('dc-created', { id, readyState: channel.readyState, ordered: channel.ordered });
    channel.addEventListener('open', () => log('dc-open', { id }));
    channel.addEventListener('close', () => log('dc-close', { id }));
    channel.addEventListener('closing', () => log('dc-closing', { id }));
    channel.addEventListener('error', (e) =>
      log('dc-error', { id, message: (e as unknown as { error?: Error }).error?.message }),
    );
    channel.addEventListener('message', (e) => {
      w.__rtcStats.received++;
      w.__rtcStats.receivedBytes += size(e.data);
      log('dc-recv', { id, bytes: size(e.data) });
    });
    const nativeSend = channel.send.bind(channel);
    channel.send = (data: never) => {
      w.__rtcStats.sent++;
      w.__rtcStats.sentBytes += size(data);
      log('dc-send', { id, bytes: size(data), readyState: channel.readyState });
      try {
        return nativeSend(data);
      } catch (error) {
        log('dc-send-throw', { id, message: String(error) });
        throw error;
      }
    };
  };

  const opts = window as unknown as { __stripIce?: boolean; __deadStun?: string };
  const Wrapped = function (this: unknown, ...args: ConstructorParameters<typeof RTCPeerConnection>) {
    const config = args[0] ? { ...args[0] } : undefined;
    if (opts.__stripIce && config) config.iceServers = [];
    // A STUN server that silently drops every packet: Chromium's ICE gathering
    // then only finishes when the STUN request times out (~40 s).
    if (opts.__deadStun && config) {
      config.iceServers = [...(config.iceServers ?? []), { urls: opts.__deadStun }];
    }
    const pc = new Native(...(config ? [config, ...args.slice(1)] : args));
    const id = Math.random().toString(36).slice(2, 6);
    log('pc-new', {
      id,
      iceServers: (config?.iceServers ?? []).flatMap((s) => s.urls),
    });
    pc.addEventListener('icecandidate', (e) => {
      const c = e.candidate;
      log('pc-candidate', c ? { id, type: c.type, protocol: c.protocol, address: c.address ?? null } : { id, end: true });
    });
    pc.addEventListener('icecandidateerror', (e) => {
      const ev = e as RTCPeerConnectionIceErrorEvent;
      log('pc-candidate-error', { id, url: ev.url, code: ev.errorCode, text: ev.errorText, address: ev.address });
    });
    pc.addEventListener('connectionstatechange', () =>
      log('pc-connection', { id, state: pc.connectionState }),
    );
    pc.addEventListener('iceconnectionstatechange', () =>
      log('pc-ice', { id, state: pc.iceConnectionState }),
    );
    pc.addEventListener('signalingstatechange', () =>
      log('pc-signaling', { id, state: pc.signalingState }),
    );
    pc.addEventListener('icegatheringstatechange', () =>
      log('pc-gathering', { id, state: pc.iceGatheringState }),
    );
    pc.addEventListener('datachannel', (e) => watchChannel(e.channel, `remote/${id}`));
    const nativeCreate = pc.createDataChannel.bind(pc);
    pc.createDataChannel = (...a: Parameters<RTCPeerConnection['createDataChannel']>) => {
      const channel = nativeCreate(...a);
      watchChannel(channel, `local/${id}`);
      return channel;
    };
    return pc;
  } as unknown as typeof RTCPeerConnection;
  Wrapped.prototype = Native.prototype;
  Object.assign(Wrapped, Native);
  window.RTCPeerConnection = Wrapped;
};

export interface Diagnostics {
  /** Epoch-timestamped events captured for this page. */
  events(): { at: number; label: string; text: string }[];
  /** Human-readable, time-ordered log of everything captured for this page. */
  lines(): string[];
  /** Console errors, page errors and failed requests seen so far. */
  problems(): string[];
  /** Pull RTC events out of the page and merge them into the timeline. */
  collect(): Promise<void>;
  /** Attach the full timeline to the test report. */
  attach(testInfo: TestInfo): Promise<void>;
}

/**
 * One-line summary of a signaling frame. SDP offers/answers are reduced to the
 * candidate lines they carry: which candidates each side advertised is exactly
 * what decides whether ICE can connect.
 */
function summarizeFrame(payload: string | Buffer): string {
  const text = String(payload);
  try {
    const msg = JSON.parse(text) as {
      Signal?: { data?: { Offer?: string; Answer?: string; IceCandidate?: string } };
    };
    const data = msg.Signal?.data;
    const sdp = data?.Offer ?? data?.Answer;
    if (sdp) {
      const kind = data?.Offer ? 'Offer' : 'Answer';
      const candidates = sdp
        .split(/\r?\n/)
        .filter((l) => l.startsWith('a=candidate:'))
        .map((l) => {
          const f = l.split(' ');
          return `${f[7]}/${f[2]}@${f[4]}:${f[5]}`;
        });
      return `Signal ${kind} candidates=[${candidates.join(', ')}]`;
    }
    if (data?.IceCandidate) {
      const c = JSON.parse(data.IceCandidate) as { candidate?: string };
      const f = (c.candidate ?? '').split(' ');
      return `Signal IceCandidate ${f[7] ?? 'end'}/${f[2] ?? ''}@${f[4] ?? ''}:${f[5] ?? ''}`;
    }
  } catch {
    /* not JSON: fall through */
  }
  return text.slice(0, 160);
}

export interface InstrumentOptions {
  /** Add an unreachable STUN server to every RTCPeerConnection's ICE config. */
  deadStun?: boolean;
  /** Add this STUN server URL (e.g. `stun:127.0.0.1:3478`) to every RTCPeerConnection's ICE config. */
  extraStun?: string;
}

/** RFC 5737 TEST-NET-1: never routable, so packets are dropped silently. */
const DEAD_STUN_URL = 'stun:192.0.2.1:3478';

export async function instrument(
  page: Page,
  label: string,
  options: InstrumentOptions = {},
): Promise<Diagnostics> {
  const extraStun = options.extraStun ?? (options.deadStun ? DEAD_STUN_URL : undefined);
  if (extraStun) {
    await page.addInitScript((url) => ((window as never as { __deadStun: string }).__deadStun = url), extraStun);
  }
  // `E2E_NO_STUN=1` removes every ICE server so a run can prove/disprove that
  // STUN gathering is what stalls the handshake.
  if (process.env.E2E_NO_STUN) await page.addInitScript(() => ((window as never as { __stripIce: boolean }).__stripIce = true));
  const timeline: { at: number; text: string }[] = [];
  const problems: string[] = [];
  const add = (text: string, at = Date.now()) => timeline.push({ at, text });

  await page.addInitScript(RTC_HOOK);

  page.on('console', (msg) => {
    const text = `console.${msg.type()}: ${msg.text()}`;
    add(text);
    if (msg.type() === 'error') problems.push(text);
  });
  page.on('pageerror', (error) => {
    const text = `pageerror: ${error.message}`;
    add(text);
    problems.push(text);
  });
  page.on('requestfailed', (request) => {
    const text = `requestfailed: ${request.method()} ${request.url()} ${request.failure()?.errorText}`;
    add(text);
    if (!request.url().includes('/_dioxus')) problems.push(text);
  });
  page.on('framenavigated', (frame) => {
    if (frame === page.mainFrame()) add(`navigated: ${frame.url()}`);
  });
  page.on('websocket', (ws) => {
    const url = ws.url();
    const short = url.replace(/^wss?:\/\/[^/]+/, '');
    add(`ws-open ${short}`);
    ws.on('framesent', (f) => add(`ws-send ${short} ${summarizeFrame(f.payload)}`));
    ws.on('framereceived', (f) => add(`ws-recv ${short} ${summarizeFrame(f.payload)}`));
    ws.on('socketerror', (e) => {
      add(`ws-error ${short} ${e}`);
      if (short.includes('/signaling')) problems.push(`ws-error ${short} ${e}`);
    });
    ws.on('close', () => add(`ws-close ${short}`));
  });

  const rtc: { at: number; text: string }[] = [];
  return {
    events() {
      return [...timeline, ...rtc].map((e) => ({ ...e, label }));
    },
    lines() {
      return format(this.events());
    },
    problems: () => problems.map((p) => `[${label}] ${p}`),
    async collect() {
      if (page.isClosed()) return;
      const entries = await page
        .evaluate(() => {
          const w = window as unknown as { __rtc?: { t: number; kind: string; detail?: unknown }[] };
          return w.__rtc ?? [];
        })
        .catch(() => []);
      rtc.length = 0;
      for (const e of entries) {
        rtc.push({ at: e.t, text: `rtc ${e.kind} ${JSON.stringify(e.detail ?? {})}` });
      }
    },
    async attach(testInfo) {
      await this.collect();
      await testInfo.attach(`${label}-timeline.log`, {
        body: this.lines().join('\n'),
        contentType: 'text/plain',
      });
    },
  };
}

function format(events: { at: number; label: string; text: string }[]) {
  const sorted = [...events].sort((a, b) => a.at - b.at);
  const t0 = sorted[0]?.at ?? 0;
  return sorted.map((e) => `+${String(e.at - t0).padStart(6)}ms [${e.label}] ${e.text}`);
}

export async function attachAll(testInfo: TestInfo, ...all: Diagnostics[]) {
  for (const d of all) await d.collect();
  const merged = format(all.flatMap((d) => d.events()));
  // Write to disk (not just the HTML report) so failures can be read from the terminal.
  const path = testInfo.outputPath('merged-timeline.log');
  await writeFile(path, merged.join('\n'));
  await testInfo.attach('merged-timeline.log', { path, contentType: 'text/plain' });
}
