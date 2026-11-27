import { readFileSync } from "node:fs";
import { join } from "node:path";
import { describe, expect, it } from "vitest";
import { NUMERIC_CLASS, formatBytes } from "./format";

function formatsBoundaries() {
  expect(formatBytes(0)).toBe("0 MB");
  expect(formatBytes(1)).toBe("1 MB"); // any non-zero value shows at least 1 MB
  expect(formatBytes(499_999)).toBe("1 MB");
  expect(formatBytes(1_500_000)).toBe("2 MB"); // whole MB below 1 GB
  expect(formatBytes(999_999_999)).toBe("1000 MB"); // still below 1 GB
  expect(formatBytes(1e9)).toBe("1 GB");
  expect(formatBytes(1_250_000_000)).toBe("1.3 GB"); // one decimal below 100 GB
  expect(formatBytes(99.4e9)).toBe("99.4 GB");
  expect(formatBytes(99.95e9)).toBe("100 GB"); // rounds up to 100: no decimal
  expect(formatBytes(100e9)).toBe("100 GB"); // none at or above 100 GB
  expect(formatBytes(512.6e9)).toBe("513 GB");
}

function usesDecimalBase() {
  // 1 GB = 10^9 bytes, as Finder reports; a binary GiB would read 1.1 GB.
  expect(formatBytes(2 ** 30)).toBe("1.1 GB");
  expect(formatBytes(10 ** 6)).toBe("1 MB");
  expect(formatBytes(10 ** 9)).toBe("1 GB");
}

function numericClassUsesTabularFigures() {
  const css = readFileSync(join(process.cwd(), "src/styles/app.css"), "utf8");
  const tokens = readFileSync(join(process.cwd(), "src/styles/gui-tokens.css"), "utf8");
  expect(css).toMatch(new RegExp(`\\.${NUMERIC_CLASS}\\s*\\{[^}]*font-variant-numeric:\\s*var\\(--numeric-figures\\)`));
  expect(tokens).toMatch(/--numeric-figures:\s*tabular-nums/);
}

describe("size formatting (REQ-DCLEAN-028)", () => {
  it("GB with one decimal below 100 GB, whole MB below 1 GB (REQ-DCLEAN-028.A1)", formatsBoundaries);
  it("uses the decimal base, 1 GB = 10^9 bytes (REQ-DCLEAN-028.A2)", usesDecimalBase);
  it("numeric cells use tabular figures (REQ-DCLEAN-028.A4)", numericClassUsesTabularFigures);
});
