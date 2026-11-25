import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { afterEach, beforeEach, describe, expect, it } from "vitest";
import { cleanup, fireEvent, render, screen, waitFor, within } from "@solidjs/testing-library";
import { App } from "../../App";
import { NAV_ITEMS } from "../nav";

// vitest runs with gui/ as its root.
const guiFile = (rel: string) => resolve(process.cwd(), rel);

function renderApp() {
  render(() => <App />);
}

function navButton(label: string) {
  return screen.getByRole("button", { name: new RegExp(`^${label}`) });
}

function brandAndTitleBarReadDisky() {
  renderApp();
  expect(screen.getByTestId("titlebar")).toHaveTextContent(/^disky$/);
  expect(document.querySelector(".brand-word")).toHaveTextContent(/^disky$/);
  expect(screen.getByRole("img", { name: "disky logo" })).toBeInTheDocument();
  expect(document.body.innerHTML).not.toMatch(/diskee/i);
}

function windowTitleIsDisky() {
  const html = readFileSync(guiFile("index.html"), "utf8");
  expect(html).toMatch(/<title>disky<\/title>/);
  const conf = JSON.parse(readFileSync(guiFile("src-tauri/tauri.conf.json"), "utf8"));
  expect(conf.productName).toBe("disky");
  expect(conf.app.windows[0].title).toBe("disky");
  expect(JSON.stringify(conf)).not.toMatch(/diskee/i);
}

async function secondaryEntriesOpenComingSoonPlaceholders() {
  renderApp();
  for (const item of NAV_ITEMS.slice(1)) {
    fireEvent.click(navButton(item.label));
    const main = screen.getByRole("main");
    // Router navigation commits in a transition, so wait for the new screen.
    expect(await within(main).findByRole("heading", { level: 1, name: item.label })).toBeInTheDocument();
    expect(within(main).getByText("Coming soon")).toBeInTheDocument();
    await waitFor(() => expect(navButton(item.label)).toHaveAttribute("aria-current", "page"));
  }
  fireEvent.click(navButton("Smart Clean"));
  await waitFor(() => expect(navButton("Smart Clean")).toHaveAttribute("aria-current", "page"));
  expect(within(screen.getByRole("main")).queryByText("Coming soon")).toBeNull();
}

function sidebarShowsSevenEntriesAndAssistantBadge() {
  renderApp();
  const nav = screen.getByRole("navigation", { name: "Main" });
  expect(within(nav).getAllByRole("button")).toHaveLength(7);
  expect(navButton("Assistant")).toHaveTextContent("Assistant4");
  expect(navButton("Smart Clean")).toHaveAttribute("aria-current", "page");
}

function meterShowsFixtureFreeSpaceAndGoal() {
  renderApp();
  const meter = screen.getByTestId("disk-meter");
  expect(meter).toHaveTextContent("Macintosh HD");
  expect(meter).toHaveTextContent("74 GB free");
  expect(meter).toHaveTextContent("Goal: keep 80 GB free");
  expect(screen.getByTestId("goal-marker").style.left).toBe(
    `calc(${(432 / 512) * 100}% - var(--meter-marker-nudge))`,
  );
}

describe("app shell", () => {
  beforeEach(() => {
    window.location.hash = "";
  });
  afterEach(cleanup);

  it("brand and title bar read disky, never Diskee (REQ-DCLEAN-032.A2)", brandAndTitleBarReadDisky);
  it("window title is disky (REQ-DCLEAN-032.A2)", windowTitleIsDisky);
  it("secondary entries open a Coming soon placeholder (REQ-DCLEAN-032.A3)", secondaryEntriesOpenComingSoonPlaceholders);
  it("sidebar shows seven entries and the assistant badge (REQ-DCLEAN-032)", sidebarShowsSevenEntriesAndAssistantBadge);
  it("meter shows fixture free space and goal (REQ-DCLEAN-032)", meterShowsFixtureFreeSpaceAndGoal);
});
