import { writeFile } from 'node:fs/promises';
import type { Page, TestInfo } from '@playwright/test';

/** Console, request, and game WebSocket events from one browser page. */
export interface Diagnostics {
  events(): { at: number; label: string; text: string }[];
  problems(): string[];
  attach(testInfo: TestInfo): Promise<void>;
}

// Keep snapshots concise and never write the private session token to a
// Playwright artifact. The frame's variant and size are enough to diagnose
// whether the server received an intent and broadcast a new state.
function summarizeFrame(payload: string | Buffer): string {
  const raw = String(payload);
  try {
    const parsed = JSON.parse(raw) as string | Record<string, unknown>;
    if (typeof parsed === 'string') return parsed;
    const variant = Object.keys(parsed)[0] ?? 'unknown';
    return `${variant} (${Buffer.byteLength(raw)} bytes)`;
  } catch {
    return `unparseable frame (${Buffer.byteLength(raw)} bytes)`;
  }
}

export async function instrument(page: Page, label: string): Promise<Diagnostics> {
  const timeline: { at: number; label: string; text: string }[] = [];
  const problems: string[] = [];
  const add = (text: string) => timeline.push({ at: Date.now(), label, text });

  page.on('console', (message) => {
    const text = `console.${message.type()}: ${message.text()}`;
    add(text);
    if (message.type() === 'error') problems.push(text);
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
  page.on('websocket', (socket) => {
    const path = socket.url().replace(/^wss?:\/\/[^/]+/, '');
    if (!path.startsWith('/room/')) return;
    add(`ws-open ${path}`);
    socket.on('framesent', (frame) => add(`ws-send ${path} ${summarizeFrame(frame.payload)}`));
    socket.on('framereceived', (frame) => add(`ws-recv ${path} ${summarizeFrame(frame.payload)}`));
    socket.on('socketerror', (error) => {
      const text = `ws-error ${path} ${error}`;
      add(text);
      problems.push(text);
    });
    socket.on('close', () => add(`ws-close ${path}`));
  });

  return {
    events: () => [...timeline],
    problems: () => problems.map((problem) => `[${label}] ${problem}`),
    async attach(testInfo) {
      await testInfo.attach(`${label}-timeline.log`, {
        body: format(timeline).join('\n'),
        contentType: 'text/plain',
      });
    },
  };
}

function format(events: { at: number; label: string; text: string }[]) {
  const sorted = [...events].sort((a, b) => a.at - b.at);
  const start = sorted[0]?.at ?? 0;
  return sorted.map((event) =>
    `+${String(event.at - start).padStart(6)}ms [${event.label}] ${event.text}`,
  );
}

export async function attachAll(testInfo: TestInfo, ...diagnostics: Diagnostics[]) {
  const path = testInfo.outputPath('merged-timeline.log');
  await writeFile(path, format(diagnostics.flatMap((item) => item.events())).join('\n'));
  await testInfo.attach('merged-timeline.log', { path, contentType: 'text/plain' });
}
