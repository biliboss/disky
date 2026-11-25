import type { ParentProps } from "solid-js";
import { Sidebar, type SidebarProps } from "./Sidebar";
import { TitleBar } from "./TitleBar";
import { SHELL_FIXTURE } from "./fixtures";
import { diskState } from "../lib/diskState";
import "./shell.css";

export interface AppShellProps extends ParentProps {
  sidebar?: SidebarProps;
}

/**
 * The window frame around every screen (REQ-DCLEAN-032): a title bar of
 * `--layout-titlebar-h` over a `--layout-sidebar-w` sidebar and the screen area.
 * The window is 1200 × 780 minimum (tauri.conf.json); extra space goes to main.
 */
export function AppShell(props: AppShellProps) {
  // REQ-DCLEAN-002: the meter follows the shared disk state; the fixture is
  // shown only until the first engine view arrives.
  const sidebar = (): SidebarProps => {
    if (props.sidebar) return props.sidebar;
    const live = diskState();
    if (!live) return SHELL_FIXTURE;
    return {
      ...SHELL_FIXTURE,
      meter: {
        volume: live.disk.volume,
        freeBytes: live.disk.free_bytes,
        capacityBytes: live.disk.capacity_bytes,
        goalBytes: live.goalBytes,
      },
    };
  };
  return (
    <div class="app-window" data-screen-label="App window">
      <TitleBar />
      <div class="app-body">
        <Sidebar meter={sidebar().meter} assistantPending={sidebar().assistantPending} />
        <main class="app-main">{props.children}</main>
      </div>
    </div>
  );
}
