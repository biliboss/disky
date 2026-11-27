import { execFileSync } from "node:child_process";
import { resolve } from "node:path";
import { describe, expect, it } from "vitest";

/** The static lints, run as tests so `npm test` fails with them. */
const GUI = resolve(__dirname, "../..");
const run = (script: string) => execFileSync(process.execPath, [resolve(GUI, "scripts", script)], { cwd: GUI, encoding: "utf8" });

/** REQ-DCLEAN-031.A1: no literal colour or pixel size in component code. */
function lintTokensFindsNoLiterals() {
  expect(run("lint-tokens.mjs")).toMatch(/no literal colours or px sizes/);
}

/** REQ-DCLEAN-012 … 015: every control themed, with hover, pressed and focus states. */
function lintControlsFindsNoUnthemedControl() {
  expect(run("lint-controls.mjs")).toMatch(/every control is themed/);
}

describe("static lints", () => {
  it("component code references tokens only (REQ-DCLEAN-031.A1)", lintTokensFindsNoLiterals);
  it("every control uses the design-system classes and states (REQ-DCLEAN-012.A1, A2)", lintControlsFindsNoUnthemedControl);
});
