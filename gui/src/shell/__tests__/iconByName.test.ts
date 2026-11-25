import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { describe, expect, it } from "vitest";
import { FALLBACK_ICON, hasIcon, iconByName } from "../iconByName";

/** Icon names in the engine's category table (`src/cleanup.rs` CATEGORIES). */
function engineIconNames(): string[] {
  const rs = readFileSync(resolve(__dirname, "../../../../src/cleanup.rs"), "utf8");
  const start = rs.indexOf("pub const CATEGORIES");
  const table = rs.slice(start, rs.indexOf("\n];", start));
  return [...table.matchAll(/^\s{8}icon:\s*"([^"]+)"/gm)].map((m) => m[1]);
}

function everyEngineIconHasItsOwnGlyph() {
  const names = engineIconNames();
  expect(names.length).toBeGreaterThanOrEqual(10);
  for (const name of names) expect(hasIcon(name), name).toBe(true);
}

function unknownNamesFallBackToAFolder() {
  expect(iconByName("not-a-lucide-icon")).toBe(FALLBACK_ICON);
  expect(iconByName("constructor")).toBe(FALLBACK_ICON);
  expect(iconByName("box")).not.toBe(FALLBACK_ICON);
}

describe("category icons by Lucide name (REQ-DCLEAN-005)", () => {
  it("every icon the engine table names resolves to its own glyph", everyEngineIconHasItsOwnGlyph);
  it("an unknown icon name falls back to a folder", unknownNamesFallBackToAFolder);
});
