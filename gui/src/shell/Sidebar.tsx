import { For } from "solid-js";
import { Brand } from "./Brand";
import { DiskMeter, type DiskMeterProps } from "./DiskMeter";
import { NAV_ITEMS } from "./nav";
import { NavItem } from "./NavItem";

export interface SidebarProps {
  meter: DiskMeterProps;
  /** Pending Assistant suggestions; 0 hides the badge (snapshot `pending`). */
  assistantPending: number;
}

/** Sidebar: brand, the seven nav entries, a spacer, the disk meter (REQ-DCLEAN-032). */
export function Sidebar(props: SidebarProps) {
  const badge = () => (props.assistantPending > 0 ? String(props.assistantPending) : "");
  return (
    <aside class="sidebar">
      <Brand />
      <nav class="nav-list" aria-label="Main">
        <For each={NAV_ITEMS}>{(item) => <NavItem item={item} badge={item.badge ? badge() : undefined} />}</For>
      </nav>
      <div class="sidebar-spacer" />
      <DiskMeter {...props.meter} />
    </aside>
  );
}
