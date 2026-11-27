import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { describe, expect, it } from "vitest";

/** REQ-DCLEAN-026.A1: the Tauri window opens at the design size and never shrinks below it. */
const conf = JSON.parse(readFileSync(resolve(__dirname, "../../../src-tauri/tauri.conf.json"), "utf8"));
const main = conf.app.windows.find((w: { label: string }) => w.label === "main");

function windowOpensAtTheDesignSize() {
  expect([main.width, main.height]).toEqual([1200, 780]);
}

function windowMinimumIsTheDesignSize() {
  expect([main.minWidth, main.minHeight]).toEqual([1200, 780]);
  // A2: larger than the minimum is allowed, so the window stays resizable.
  expect(main.resizable ?? true).toBe(true);
}

describe("window size (REQ-DCLEAN-026)", () => {
  it("opens at 1200 × 780 (A1)", windowOpensAtTheDesignSize);
  it("its minimum size is 1200 × 780 and it may grow (A1, A2)", windowMinimumIsTheDesignSize);
});
