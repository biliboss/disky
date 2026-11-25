import { HashRouter, type RouteDefinition } from "@solidjs/router";
import type { JSX } from "solid-js";
import { AppShell } from "./shell/AppShell";
import { NAV_ITEMS } from "./shell/nav";
import { Assistant } from "./screens/Assistant";
import { Duplicates } from "./screens/Duplicates";
import { History } from "./screens/History";
import { LargeFiles } from "./screens/LargeFiles";
import { Settings } from "./screens/Settings";
import { SmartClean } from "./screens/SmartClean";
import { SpaceMap } from "./screens/SpaceMap";
import type { NavId } from "./shell/nav";

/**
 * Every sidebar entry's screen. The record is total over {@link NavId}, so
 * a new entry without its screen fails to type-check: no placeholder screen
 * remains (REQ-DSCR-017.A3).
 */
export const SCREENS: Record<NavId, () => JSX.Element> = {
  clean: () => <SmartClean />,
  assistant: () => <Assistant />,
  map: () => <SpaceMap />,
  large: () => <LargeFiles />,
  dupes: () => <Duplicates />,
  history: () => <History />,
  settings: () => <Settings />,
};

/** Routes: one per nav entry, each opening its real screen (REQ-DCLEAN-032, REQ-DSCR-017.A3). */
export const ROUTES: RouteDefinition[] = NAV_ITEMS.map((item) => ({
  path: item.route,
  component: SCREENS[item.id],
}));

export function App() {
  return <HashRouter root={AppShell}>{ROUTES}</HashRouter>;
}
