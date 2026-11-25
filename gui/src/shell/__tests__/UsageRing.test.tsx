import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { cleanup, render, screen } from "@solidjs/testing-library";
import { afterEach, describe, expect, it } from "vitest";
import { UsageRing } from "../UsageRing";
import { RING_CIRCUMFERENCE, ringDash } from "../../lib/meter";

const disk = { volume: "Macintosh HD", capacity_bytes: 512e9, free_bytes: 74e9 };
afterEach(cleanup);

function ringLabelShowsFreeAndCapacity() {
  render(() => <UsageRing disk={disk} phase="idle" />);
  expect(screen.getByTestId("ring-free")).toHaveTextContent(/^74 GB$/);
  expect(screen.getByTestId("ring-total")).toHaveTextContent(/^free of 512 GB$/);
  expect(screen.getByTestId("ring-fill").getAttribute("stroke-dasharray")).toBe(
    ringDash(512e9, 74e9, RING_CIRCUMFERENCE),
  );
}

function ringFillClassFollowsPhase() {
  render(() => <UsageRing disk={disk} phase="idle" />);
  expect(screen.getByTestId("ring-fill")).toHaveClass("ring-fill", "ring-fill--idle");
  cleanup();
  render(() => <UsageRing disk={disk} phase="done" />);
  expect(screen.getByTestId("ring-fill")).toHaveClass("ring-fill", "ring-fill--done");
}

function ringAndBarUseTheSpecifiedTokens() {
  // jsdom does not cascade stylesheets, so read the rules the classes bind to.
  const css = readFileSync(resolve(process.cwd(), "src/shell/shell.css"), "utf8");
  const rule = (sel: string) => css.match(new RegExp(`${sel.replace(/[.-]/g, "\\$&")}\\s*\\{([^}]*)\\}`))?.[1] ?? "";
  expect(rule(".ring-fill--idle")).toMatch(/stroke:\s*var\(--color-accent\);/);
  expect(rule(".ring-fill--done")).toMatch(/stroke:\s*var\(--color-accent-2\);/);
  expect(rule(".ring-track")).toMatch(/stroke:\s*var\(--color-surface\);/);
  expect(rule(".meter-bar")).toMatch(/background:\s*var\(--color-neutral-200\);/);
  expect(rule(".meter-fill")).toMatch(/background:\s*var\(--color-accent\);/);
}

describe("usage ring", () => {
  it("label shows the free amount and the capacity (REQ-DCLEAN-002)", ringLabelShowsFreeAndCapacity);
  it("fill class follows the phase (REQ-DCLEAN-002.A5)", ringFillClassFollowsPhase);
  it("ring and bar bind the specified colour tokens (REQ-DCLEAN-002.A5)", ringAndBarUseTheSpecifiedTokens);
});
