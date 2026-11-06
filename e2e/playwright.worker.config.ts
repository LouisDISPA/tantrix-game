import path from 'node:path';
import { defineConfig } from '@playwright/test';
import native from './playwright.native.config';

const port = 4174;
const baseURL = process.env.BASE_URL ?? `http://127.0.0.1:${port}`;

// Use `just worker-e2e` to build matching Worker and client assets before this suite.
export default defineConfig({
  ...native,
  retries: process.env.CI ? 1 : 0,
  workers: 8,
  use: { ...native.use, baseURL },
  webServer: process.env.BASE_URL ? undefined : {
    // Keep request URLs on this address: rooms compare them with the page's Origin.
    command: `pnpm exec wrangler dev --port ${port} --ip 127.0.0.1 --local-upstream 127.0.0.1:${port}`,
    cwd: path.join(__dirname, '..', 'cloudflare'),
    url: baseURL,
    timeout: 60_000,
    reuseExistingServer: !process.env.CI,
  },
});
