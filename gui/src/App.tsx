import { HashRouter, type RouteDefinition } from "@solidjs/router";
import { AppShell } from "./shell/AppShell";
import { NAV_ITEMS } from "./shell/nav";
import { Placeholder } from "./screens/Placeholder";
import { SmartClean } from "./screens/SmartClean";

/**
 * Routes: "/" is Smart Clean; every other nav entry opens a placeholder until
 * its own spec is implemented (REQ-DCLEAN-032.A3).
 */
export const ROUTES: RouteDefinition[] = [
  { path: "/", component: SmartClean },
  ...NAV_ITEMS.filter((n) => n.route !== "/").map((item) => ({
    path: item.route,
    component: () => <Placeholder title={item.label} />,
  })),
];

export function App() {
  return <HashRouter root={AppShell}>{ROUTES}</HashRouter>;
}
