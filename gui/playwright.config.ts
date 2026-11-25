import { defineConfig, devices } from "@playwright/test";

/**
 * Visual tests for the app shell (REQ-DCLEAN-032). The app is served from the
 * production Vite build; the reference is specs/disk-cleanup/snapshot.html,
 * rendered at test time into tests/visual/__baseline__/ (gitignored).
 */
export default defineConfig({
  testDir: "tests/visual",
  snapshotPathTemplate: "{testDir}/__baseline__/{arg}{ext}",
  updateSnapshots: "none",
  fullyParallel: false,
  workers: 1,
  reporter: [["list"]],
  use: {
    ...devices["Desktop Chrome"],
    baseURL: "http://localhost:4173",
    viewport: { width: 1200, height: 780 },
    deviceScaleFactor: 1,
  },
  webServer: {
    command: "npm run build && npm run preview",
    url: "http://localhost:4173",
    reuseExistingServer: true,
    timeout: 180_000,
  },
});
