import path from 'node:path';
import { defineConfig } from '@playwright/test';
import base from './playwright.config';

// The same tests, run against the production Cloudflare Worker in `wrangler
// dev` instead of the native fullstack server: pages are server-rendered by
// `tantrix-worker`, static files come from the assets binding and signaling
// goes through the Room Durable Object. Build it first: `just worker-build`.
const port = 4174;
const baseURL = `http://127.0.0.1:${port}`;

export default defineConfig({
  ...base,
  use: { ...base.use, baseURL },
  webServer: {
    command: `npx wrangler dev --port ${port} --ip 127.0.0.1`,
    cwd: path.join(__dirname, '..', 'cloudflare'),
    url: baseURL,
    timeout: 60_000,
    reuseExistingServer: false,
  },
});
