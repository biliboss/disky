import { describe, expect, it } from "vitest";
import { detectOs, hasFullDiskAccess, revealLabel, rootLabel, skippedHint } from "./os";

const MAC = "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/605.1.15 (KHTML, like Gecko)";
const WIN =
  "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0 Safari/537.36 Edg/120.0";
const LINUX = "Mozilla/5.0 (X11; Ubuntu; Linux x86_64) AppleWebKit/605.1.15 (KHTML, like Gecko)";

function eachOsHasItsOwnWords() {
  expect([MAC, WIN, LINUX].map(detectOs)).toEqual(["macos", "windows", "linux"]);
  expect(revealLabel("macos")).toBe("Reveal in Finder");
  expect(revealLabel("windows")).toBe("Show in Explorer");
  expect(revealLabel("linux")).toBe("Show in folder");
  expect(rootLabel("macos")).toBe("Macintosh HD");
  expect(rootLabel("linux")).toBe("File System");
  expect(hasFullDiskAccess("macos")).toBe(true);
  expect(hasFullDiskAccess("windows") || hasFullDiskAccess("linux")).toBe(false);
  expect(skippedHint("linux")).not.toMatch(/Full Disk Access/);
}

describe("os", () => {
  it("names the reveal action, root and permission hint per OS (REQ-XPLAT-008)", eachOsHasItsOwnWords);
});
