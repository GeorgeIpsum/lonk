import { defineConfig, devices } from '@playwright/test';
import * as os from 'node:os';
import * as path from 'node:path';

const PORT = 8808;
const baseURL = process.env.BASE_URL ?? `http://127.0.0.1:${PORT}`;
const tmpDb = path.join(os.tmpdir(), `lonk-e2e-${process.pid}.db`);

export default defineConfig({
  testDir: './tests',
  fullyParallel: true,
  use: { baseURL },
  projects: [{ name: 'chromium', use: { ...devices['Desktop Chrome'] } }],
  // With BASE_URL set, target that deployed instance instead of spawning.
  webServer: process.env.BASE_URL
    ? undefined
    : {
        command: 'cargo run --bin lonkd',
        cwd: '..',
        url: baseURL,
        reuseExistingServer: false,
        timeout: 180_000,
        env: {
          ...process.env,
          LONK_DB: tmpDb,
          ROCKET_ADDRESS: '127.0.0.1',
          ROCKET_PORT: String(PORT),
        },
      },
});
