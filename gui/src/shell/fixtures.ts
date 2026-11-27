import type { SidebarProps } from "./Sidebar";

/**
 * Shell fixture data, equal to the snapshot's prototype state (used 438 of
 * 512 GB, goal 80 GB, 4 pending Assistant suggestions). The meter shows it
 * only until the first engine view sets the shared disk state
 * (REQ-DCLEAN-002), and the badge only until the engine's Assistant view
 * loads (REQ-DSCR-009.A5).
 */
export const SHELL_FIXTURE: SidebarProps = {
  meter: {
    volume: "Macintosh HD",
    capacityBytes: 512e9,
    freeBytes: 74e9,
    goalBytes: 80e9,
  },
  assistantPending: 4,
};
