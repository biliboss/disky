import { HashRouter, type RouteDefinition } from "@solidjs/router";
import type { JSX } from "solid-js";
import { AppShell } from "./shell/AppShell";
import { NAV_ITEMS } from "./shell/nav";
import { History } from "./screens/History";
import { LargeFiles } from "./screens/LargeFiles";
import { Placeholder } from "./screens/Placeholder";
import { Settings } from "./screens/Settings";
import { SmartClean } from "./screens/SmartClean";
import { SpaceMap } from "./screens/SpaceMap";
import type { NavId } from "./shell/nav";

/** Screens that are implemented; every other nav entry is a placeholder. */
const SCREENS: Partial<Record<NavId, () => JSX.Element>> = {
  clean: () => <SmartClean />,
  map: () => <SpaceMap />,
  large: () => <LargeFiles />,
  history: () => <History />,
  settings: () => <Settings />,
};

/**
 * Routes: one per nav entry. Entries whose own spec isn't implemented yet
 * open a placeholder (REQ-DCLEAN-032.A3).
 */
export const ROUTES: RouteDefinition[] = NAV_ITEMS.map((item) => ({
  path: item.route,
  component: SCREENS[item.id] ?? (() => <Placeholder title={item.label} />),
}));

export function App() {
  return <HashRouter root={AppShell}>{ROUTES}</HashRouter>;
}
