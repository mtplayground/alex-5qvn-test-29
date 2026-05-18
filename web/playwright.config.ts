import { defineConfig, devices } from '@playwright/test'

export default defineConfig({
  testDir: './tests',
  fullyParallel: false,
  retries: 0,
  workers: 1,
  timeout: 120_000,
  expect: {
    timeout: 15_000,
  },
  use: {
    baseURL: 'http://127.0.0.1:8080',
    trace: 'on-first-retry',
    ...devices['Desktop Chrome'],
  },
  webServer: {
    command: 'bash /workspace/web/scripts/run-e2e-stack.sh',
    url: 'http://127.0.0.1:8080/healthz',
    reuseExistingServer: !process.env.CI,
    timeout: 180_000,
  },
})
