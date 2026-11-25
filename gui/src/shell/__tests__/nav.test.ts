import { describe, expect, it } from "vitest";
import { NAV_ITEMS } from "../nav";

const SNAPSHOT_ORDER = [
  "Smart Clean",
  "Assistant",
  "Space Map",
  "Large files",
  "Duplicates",
  "History",
  "Settings",
];

function navHasSevenEntriesInSnapshotOrder() {
  expect(NAV_ITEMS.map((n) => n.label)).toEqual(SNAPSHOT_ORDER);
}

function onlyAssistantCarriesBadge() {
  expect(NAV_ITEMS.filter((n) => n.badge).map((n) => n.id)).toEqual(["assistant"]);
}

function navRoutesAreUniqueAndSmartCleanIsHome() {
  const routes = NAV_ITEMS.map((n) => n.route);
  expect(new Set(routes).size).toBe(routes.length);
  expect(NAV_ITEMS[0].route).toBe("/");
}

describe("NAV_ITEMS", () => {
  it("nav has seven entries in snapshot order (REQ-DCLEAN-032)", navHasSevenEntriesInSnapshotOrder);
  it("only the Assistant entry carries a badge (REQ-DCLEAN-032)", onlyAssistantCarriesBadge);
  it("nav routes are unique and Smart Clean is home (REQ-DCLEAN-032.A3)", navRoutesAreUniqueAndSmartCleanIsHome);
});
