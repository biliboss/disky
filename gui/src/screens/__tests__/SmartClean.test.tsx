import { cleanup, render, screen } from "@solidjs/testing-library";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

const invoke = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({ invoke: (...args: unknown[]) => invoke(...args) }));

import { SmartClean } from "../SmartClean";

function engineView(overrides: Record<string, unknown> = {}) {
  return {
    disk: { volume: "Macintosh HD", capacity_bytes: 512e9, free_bytes: 74e9 },
    goal_bytes: 80e9,
    last_scan_at: null,
    snapshot_taken_at: null,
    is_stale: false,
    preferences: { trash_by_default: true, safe_only: true, notifications: true, assistant: true },
    categories: [
      {
        id: "engine_cat_a",
        name: "Engine label A",
        description: "Engine description A",
        safety: "safe",
        bytes: 4_200_000_000,
        items: [{ name: "x", path: "/x", bytes: 4_200_000_000 }],
      },
      {
        id: "engine_cat_b",
        name: "Engine label B",
        description: "Engine description B",
        safety: "review",
        bytes: 600_000_000,
        items: [{ name: "y", path: "/y", bytes: 600_000_000 }],
      },
    ],
    ...overrides,
  };
}

beforeEach(() => invoke.mockReset());
afterEach(cleanup);

async function rendersExactlyTheEngineCategories() {
  invoke.mockResolvedValue(engineView());
  render(() => <SmartClean />);
  const rows = await screen.findAllByRole("row");
  expect(rows.map((r) => r.getAttribute("data-category-id"))).toEqual(["engine_cat_a", "engine_cat_b"]);
  expect(rows[0]).toHaveTextContent("Engine label A");
  expect(rows[0]).toHaveTextContent("Engine description A");
  expect(rows[0]).toHaveTextContent("Safe");
  expect(rows[0]).toHaveTextContent("4.2 GB");
  expect(rows[1]).toHaveTextContent("Review");
  expect(rows[1]).toHaveTextContent("600 MB");
  expect(rows[1].lastElementChild).toHaveClass("num");
  expect(invoke).toHaveBeenCalledWith("load_cleanup_view", { snapshot: null, goalBytes: null });
}

async function malformedDataShowsTheErrorStateNotAPartialList() {
  const bad = engineView();
  (bad.categories as any[])[1].bytes = -5;
  invoke.mockResolvedValue(bad);
  render(() => <SmartClean />);
  expect(await screen.findByRole("alert")).toHaveTextContent("view.categories[1].bytes");
  expect(screen.queryByTestId("categories")).toBeNull();
  expect(screen.getByRole("button", { name: "Try again" })).toBeInTheDocument();
}

async function staleSnapshotWarnsBeforeCleaning() {
  invoke.mockResolvedValue(engineView({ is_stale: true }));
  render(() => <SmartClean />);
  expect(await screen.findByTestId("stale-warning")).toHaveTextContent("more than a day old");
}

async function emptyViewShowsNothingToClean() {
  invoke.mockResolvedValue(engineView({ categories: [] }));
  render(() => <SmartClean />);
  expect(await screen.findByText("Nothing to clean right now.")).toBeInTheDocument();
}

describe("Smart Clean consumes the engine view", () => {
  it("renders exactly the engine's categories (REQ-DCLEAN-027.A4)", rendersExactlyTheEngineCategories);
  it("malformed data renders the error state, not a partial list (REQ-DCLEAN-029.A4)", malformedDataShowsTheErrorStateNotAPartialList);
  it("a stale snapshot warns before cleaning (REQ-DCLEAN-029.A2)", staleSnapshotWarnsBeforeCleaning);
  it("an empty view says nothing to clean (REQ-DCLEAN-027)", emptyViewShowsNothingToClean);
});
