import path from 'node:path';
import { defineConfig } from '@playwright/test';
import native from './playwright.native.config';

const port = 4174;
const baseURL = process.env.BASE_URL ?? `http://127.0.0.1:${port}`;

// Build the Worker and client with `just worker-build` first.
export default defineConfig({
  ...native,
  workers: 1,
  retries: process.env.CI ? 1 : 0,
  workers: process.env.CI ? 4 : 8,
  use: { ...native.use, baseURL },
  webServer: process.env.BASE_URL ? undefined : {
    command: `pnpm exec wrangler dev --port ${port} --ip 127.0.0.1`,
    cwd: path.join(__dirname, '..', 'cloudflare'),
    url: baseURL,
    timeout: 60_000,
    reuseExistingServer: !process.env.CI,
  },
});
