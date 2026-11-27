import { defineConfig } from "vitest/config";
import solid from "vite-plugin-solid";

// Tauri expects a fixed dev port and must not have it silently changed.
export default defineConfig({
  plugins: [solid()],
  clearScreen: false,
  server: { port: 1420, strictPort: true },
  build: { target: "safari16", outDir: "dist" },
  test: {
    environment: "jsdom",
    // jsdom's user agent names the host OS ("(linux)", "(darwin)"), and
    // src/lib/os.ts words the app from it. Pin the macOS design, as
    // playwright.config.ts does, so the suite is the same on every OS
    // (REQ-XPLAT-010.A3).
    environmentOptions: {
      jsdom: {
        userAgent:
          "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/130.0.0.0 Safari/537.36",
      },
    },
    include: ["src/**/*.test.{ts,tsx}"],
    setupFiles: ["./src/test-setup.ts"],
  },
});
