import { onCleanup, onMount, type ParentProps } from "solid-js";
import { useNavigate } from "@solidjs/router";
import { Sidebar, type SidebarProps } from "./Sidebar";
import { TitleBar } from "./TitleBar";
import { SHELL_FIXTURE } from "./fixtures";
import { diskState } from "../lib/diskState";
import { onAssistantView, onFocusHome } from "../lib/engine";
import { actionableCount, reloadAssistant, setAssistantView } from "../screens/assistant/store";
import { isNavBlocked } from "../screens/smartClean/store";
import "./shell.css";

export interface AppShellProps extends ParentProps {
  sidebar?: SidebarProps;
}

/**
 * The window frame around every screen (REQ-DCLEAN-032): a title bar of
 * `--layout-titlebar-h` over a `--layout-sidebar-w` sidebar and the screen area.
 * The window is 1200 × 780 minimum (tauri.conf.json); extra space goes to main.
 *
 * The Assistant badge counts the feed's actionable cards (REQ-DSCR-009.A5).
 * The shell loads the engine's Assistant view at launch and takes every view
 * the proactive checks send (REQ-DSCR-011.A1); a click on the below-goal
 * notification lands on Smart Clean (REQ-DSCR-011.A3).
 *
 * @implements REQ-DSCR-009
 * @implements REQ-DSCR-011
 */
export function AppShell(props: AppShellProps) {
  const navigate = useNavigate();
  const unlisten: (() => void)[] = [];
  onMount(() => {
    void reloadAssistant();
    void onAssistantView(setAssistantView)
      .then((u) => unlisten.push(u))
      .catch(() => undefined);
    void onFocusHome(() => {
      if (!isNavBlocked()) navigate("/");
    })
      .then((u) => unlisten.push(u))
      .catch(() => undefined);
  });
  onCleanup(() => unlisten.forEach((u) => u()));

  // REQ-DCLEAN-002: the meter follows the shared disk state; the fixture is
  // shown only until the first engine view arrives. The badge likewise shows
  // the fixture count until the engine's Assistant view loads.
  const sidebar = (): SidebarProps => {
    if (props.sidebar) return props.sidebar;
    const live = diskState();
    const pending = actionableCount() ?? SHELL_FIXTURE.assistantPending;
    if (!live) return { ...SHELL_FIXTURE, assistantPending: pending };
    return {
      meter: {
        volume: live.disk.volume,
        freeBytes: live.disk.free_bytes,
        capacityBytes: live.disk.capacity_bytes,
        goalBytes: live.goalBytes,
      },
      assistantPending: pending,
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
