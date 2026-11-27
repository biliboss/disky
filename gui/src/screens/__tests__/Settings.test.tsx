import { cleanup, fireEvent, render, screen, waitFor, within } from "@solidjs/testing-library";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

const invoke = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({ invoke: (...args: unknown[]) => invoke(...args) }));
vi.mock("@tauri-apps/api/event", () => ({ listen: async () => () => undefined }));

import { HashRouter } from "@solidjs/router";
import { App } from "../../App";
import { Settings } from "../Settings";
import { resetDiskState } from "../../lib/diskState";
import { resetSmartCleanStore } from "../smartClean/store";
import { SETTINGS_TOGGLES } from "../settings/settingsCopy";
import type { Preferences } from "../../lib/contract";

/** The snapshot's Settings state: all toggles on, Weekly, its three exclusions. */
function snapshotPrefs(): Preferences {
  return {
    trash_by_default: true,
    skip_recently_used: true,
    safe_only: true,
    warn_below_goal: true,
    goal_bytes: 80e9,
    checks: { enabled: true, frequency: "daily" },
    agent: "claude",
    scheduled_scan: "weekly",
    excluded: ["~/Projects/active", "~/Pictures/RAW", "~/Library/Mobile Documents"],
  };
}

/**
 * A stand-in for the engine's one preferences store: `get_prefs` reads it,
 * `set_prefs` writes one key (as `disky prefs --set` does) and every view
 * carries it, so the GUI can only ever show what the store holds.
 */
function fakeEngine(initial: Preferences, picked: string | null = "~/Movies/Edits") {
  const store = { prefs: structuredClone(initial) };
  invoke.mockImplementation(async (cmd: string, args: any) => {
    if (cmd === "get_prefs") return structuredClone(store.prefs);
    if (cmd === "pick_folder") return picked;
    if (cmd === "set_prefs") {
      const { key, value } = args as { key: string; value: string };
      const p = store.prefs as any;
      if (key === "excluded") p.excluded = JSON.parse(value);
      else if (key === "scheduled_scan") p.scheduled_scan = value;
      else if (key in p && typeof p[key] === "boolean") p[key] = value === "true";
      else throw { code: "usage", title: "invalid preference", detail: `unknown preference '${key}'`, retryable: false };
      return structuredClone(store.prefs);
    }
    if (cmd === "load_cleanup_view" || cmd === "scan") return view(store.prefs, cmd === "scan");
    throw new Error(`unexpected command ${cmd}`);
  });
  return store;
}

function view(prefs: Preferences, withItems: boolean) {
  return {
    disk: { volume: "Macintosh HD", capacity_bytes: 512e9, free_bytes: 74e9 },
    goal_bytes: prefs.goal_bytes,
    last_scan_at: null,
    snapshot_taken_at: null,
    is_stale: false,
    preferences: structuredClone(prefs),
    large_files_count: 0,
    skipped_paths: 0,
    categories: withItems
      ? [
          {
            id: "engine_cat_a",
            name: "Engine label A",
            description: "Engine description A",
            icon: "box",
            safety: "safe",
            bytes: 4_200_000_000,
            items: [{ name: "one", path: "/a/one", bytes: 4_200_000_000 }],
          },
        ]
      : [],
    catalogue: [{ id: "engine_cat_a", name: "Engine label A", icon: "box", safety: "safe" }],
  };
}

function renderSettings() {
  render(() => (
    <HashRouter>
      {[{ path: "/", component: Settings }]}
    </HashRouter>
  ));
  return screen.findByTestId("settings");
}

function setCalls() {
  return invoke.mock.calls.filter(([cmd]) => cmd === "set_prefs").map(([, a]) => a);
}

async function rendersEveryPreferenceFromTheStore() {
  fakeEngine({ ...snapshotPrefs(), safe_only: false, scheduled_scan: "monthly" });
  const s = await renderSettings();
  expect(within(s).getByRole("heading", { level: 1 })).toHaveTextContent(/^Settings$/);
  const switches = await within(s).findAllByRole("switch");
  expect(switches).toHaveLength(4);
  SETTINGS_TOGGLES.forEach((t, i) => {
    expect(switches[i]).toHaveAccessibleName(t.label);
    expect(switches[i]).toHaveAccessibleDescription(t.desc);
    expect(switches[i]).toHaveAttribute("aria-checked", t.key === "safe_only" ? "false" : "true");
  });
  expect(SETTINGS_TOGGLES.map((t) => t.label)).toEqual([
    "Move to Trash instead of deleting",
    "Skip recently used files",
    "Preselect only safe categories",
    "Warn when below my space goal",
  ]);
  const group = within(s).getByRole("radiogroup", { name: "Scheduled scan" });
  const radios = within(group).getAllByRole("radio");
  expect(radios.map((r) => r.closest("label")!.textContent)).toEqual(["Off", "Weekly", "Monthly"]);
  expect(within(group).getByRole("radio", { name: "Monthly" })).toBeChecked();
  expect(within(s).getByText("Never touch these folders")).toBeInTheDocument();
  expect(within(s).getByRole("button", { name: "Add folder" })).toHaveClass("btn", "btn-secondary");
  const chips = within(within(s).getByRole("list", { name: "Never touch these folders" })).getAllByRole("listitem");
  expect(chips.map((c) => c.textContent)).toEqual(snapshotPrefs().excluded);
  expect(invoke).toHaveBeenCalledWith("get_prefs", undefined);
}

async function eachSwitchSavesImmediately() {
  const store = fakeEngine(snapshotPrefs());
  const s = await renderSettings();
  const recent = await within(s).findByRole("switch", { name: "Skip recently used files" });
  fireEvent.click(recent);
  await waitFor(() => expect(recent).toHaveAttribute("aria-checked", "false"));
  expect(setCalls()).toEqual([{ key: "skip_recently_used", value: "false" }]);
  expect(store.prefs.skip_recently_used).toBe(false);
  fireEvent.click(recent);
  await waitFor(() => expect(recent).toHaveAttribute("aria-checked", "true"));
  expect(setCalls()[1]).toEqual({ key: "skip_recently_used", value: "true" });
}

async function scheduleSavesThePickedPeriod() {
  const store = fakeEngine(snapshotPrefs());
  const s = await renderSettings();
  const off = await within(s).findByRole("radio", { name: "Off" });
  fireEvent.click(off);
  await waitFor(() => expect(off).toBeChecked());
  expect(setCalls()).toEqual([{ key: "scheduled_scan", value: "off" }]);
  expect(store.prefs.scheduled_scan).toBe("off");
}

async function addFolderUsesThePickerAndSaves() {
  const store = fakeEngine(snapshotPrefs(), "/Volumes/Archive, 2024");
  const s = await renderSettings();
  fireEvent.click(await within(s).findByRole("button", { name: "Add folder" }));
  await within(s).findByText("/Volumes/Archive, 2024");
  expect(invoke).toHaveBeenCalledWith("pick_folder", undefined);
  expect(setCalls()).toEqual([
    { key: "excluded", value: JSON.stringify([...snapshotPrefs().excluded, "/Volumes/Archive, 2024"]) },
  ]);
  expect(store.prefs.excluded).toHaveLength(4);
}

async function cancellingThePickerSavesNothing() {
  fakeEngine(snapshotPrefs(), null);
  const s = await renderSettings();
  fireEvent.click(await within(s).findByRole("button", { name: "Add folder" }));
  await waitFor(() => expect(invoke).toHaveBeenCalledWith("pick_folder", undefined));
  expect(setCalls()).toEqual([]);
}

async function chipRemoveButtonIsNamedAndSaves() {
  const store = fakeEngine(snapshotPrefs());
  const s = await renderSettings();
  const remove = await within(s).findByRole("button", { name: "Remove ~/Pictures/RAW" });
  fireEvent.click(remove);
  await waitFor(() => expect(within(s).queryByText("~/Pictures/RAW")).toBeNull());
  expect(setCalls()).toEqual([
    { key: "excluded", value: JSON.stringify(["~/Projects/active", "~/Library/Mobile Documents"]) },
  ]);
  expect(store.prefs.excluded).toEqual(["~/Projects/active", "~/Library/Mobile Documents"]);
}

async function aFailedSaveIsReportedAndShowsTheStore() {
  fakeEngine(snapshotPrefs());
  invoke.mockImplementationOnce(async () => snapshotPrefs()); // get_prefs
  invoke.mockImplementationOnce(async () => {
    throw { code: "generic", title: "malformed preferences", detail: "preferences file is malformed", retryable: false };
  });
  const s = await renderSettings();
  const trash = await within(s).findByRole("switch", { name: "Move to Trash instead of deleting" });
  fireEvent.click(trash);
  expect(await within(s).findByRole("alert")).toHaveTextContent("preferences file is malformed");
  expect(trash).toHaveAttribute("aria-checked", "true");
}

async function aMalformedStoreIsReportedNotDefaulted() {
  invoke.mockImplementation(async () => {
    throw { code: "generic", title: "malformed preferences", detail: "preferences file x is malformed", retryable: false };
  });
  const s = await renderSettings();
  expect(await within(s).findByRole("alert")).toHaveTextContent("preferences file x is malformed");
  expect(within(s).queryAllByRole("switch")).toHaveLength(0);
}

/** REQ-DSCR-016.A4 end to end: Settings → store → Smart Clean. */
async function trashOffMakesSmartCleanDeletePermanently() {
  fakeEngine(snapshotPrefs());
  window.location.hash = "#/settings";
  render(() => <App />);
  const s = await screen.findByTestId("settings");
  const trash = await within(s).findByRole("switch", { name: "Move to Trash instead of deleting" });
  fireEvent.click(trash);
  await waitFor(() => expect(trash).toHaveAttribute("aria-checked", "false"));

  fireEvent.click(screen.getByRole("button", { name: /^Smart Clean/ }));
  fireEvent.click(within(await screen.findByTestId("idle")).getByRole("button", { name: "Start scan" }));
  await screen.findByTestId("results");
  expect(screen.getByTestId("safety-note")).toHaveTextContent(
    /^Trash is off in Settings, so items will be deleted permanently\.$/,
  );
  screen.getByTestId("clean").focus();
  fireEvent.click(screen.getByTestId("clean"));
  const mode = within(screen.getByRole("dialog")).getByRole("radiogroup", { name: "Clean mode" });
  expect(within(mode).getByRole("radio", { name: "Delete permanently" })).toBeChecked();
  expect(within(mode).getByRole("radio", { name: "Move to Trash" })).not.toBeChecked();
}

beforeEach(() => {
  invoke.mockReset();
  resetDiskState();
  resetSmartCleanStore();
  window.location.hash = "";
});
afterEach(cleanup);

describe("Settings screen (REQ-DSCR-016)", () => {
  it("renders every preference from the store (A1, A2)", rendersEveryPreferenceFromTheStore);
  it("each switch saves its preference immediately (A2)", eachSwitchSavesImmediately);
  it("the scheduled-scan group saves the picked period (A1)", scheduleSavesThePickedPeriod);
  it("Add folder uses the native picker and saves the list (A3)", addFolderUsesThePickerAndSaves);
  it("cancelling the picker saves nothing (A3)", cancellingThePickerSavesNothing);
  it("each chip has a Remove <folder> button that saves (A3)", chipRemoveButtonIsNamedAndSaves);
  it("a failed save is reported and the store's value stays (REQ-DSCR-001.A2)", aFailedSaveIsReportedAndShowsTheStore);
  it("a malformed store is reported, never defaulted (REQ-DSCR-001.A3)", aMalformedStoreIsReportedNotDefaulted);
  it("trash off makes Smart Clean default to Delete permanently (A4)", trashOffMakesSmartCleanDeletePermanently);
});
