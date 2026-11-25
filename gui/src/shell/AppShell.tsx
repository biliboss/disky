import type { ParentProps } from "solid-js";
import { Sidebar, type SidebarProps } from "./Sidebar";
import { TitleBar } from "./TitleBar";
import { SHELL_FIXTURE } from "./fixtures";
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
  return (
    <div class="app-window" data-screen-label="App window">
      <TitleBar />
      <div class="app-body">
        <Sidebar {...(props.sidebar ?? SHELL_FIXTURE)} />
        <main class="app-main">{props.children}</main>
      </div>
    </div>
  );
}
