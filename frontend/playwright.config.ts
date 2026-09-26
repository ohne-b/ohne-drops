import { defineConfig } from '@playwright/test';
export default defineConfig({
  testDir: './tests',
  testMatch: '**/*.spec.ts',
  fullyParallel: false,
  workers: 1,
  use: { baseURL: 'http://127.0.0.1:8765', browserName: 'chromium', trace: 'retain-on-failure' },
  webServer: {
    command:
      'cargo run --manifest-path ../Cargo.toml --locked --features dashboard-fixture --bin dashboard-fixture',
    url: 'http://127.0.0.1:8765/__test/health',
    reuseExistingServer: false,
    timeout: 120_000,
  },
});
