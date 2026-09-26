import { defineConfig } from '@playwright/test';
export default defineConfig({
  testDir: './tests',
  testMatch: '**/*.spec.ts',
  fullyParallel: false,
  workers: 1,
  use: { baseURL: 'http://127.0.0.1:8765', browserName: 'chromium', trace: 'retain-on-failure' },
  webServer: {
    command:
      process.platform === 'win32'
        ? 'cmd /d /c "cd .. && call env\\Scripts\\activate.bat && python -m uvicorn tests.dashboard_server:app --host 127.0.0.1 --port 8765"'
        : 'cd .. && . env/bin/activate && python -m uvicorn tests.dashboard_server:app --host 127.0.0.1 --port 8765',
    url: 'http://127.0.0.1:8765/__test/health',
    reuseExistingServer: false,
  },
});
