import { Show } from "solid-js";
import { Dynamic } from "solid-js/web";
import { useLocation, useNavigate } from "@solidjs/router";
import type { NavItemDef } from "./nav";
import { leaveViaNav } from "../screens/smartClean/phase";
import { isNavBlocked, setSmartCleanState } from "../screens/smartClean/store";

/** The Smart Clean route. */
const SMART_CLEAN_ROUTE = "/";

export interface NavItemProps {
  item: NavItemDef;
  /** Badge text; rendered only when the item carries a badge and this is non-empty. */
  badge?: string;
}

/**
 * One sidebar entry. Active = the current route, exposed as
 * `aria-current="page"` and styled by it (accent pill, bg-coloured text,
 * weight 600 — snapshot `n.style`; REQ-DCLEAN-011.A3).
 *
 * While Smart Clean scans or cleans (REQ-DCLEAN-011.A1, A2) the entry is
 * `aria-disabled` with the design-system disabled treatment, and activation
 * by pointer, Enter or Space changes nothing. It stays focusable, so it
 * remains discoverable (REQ-DCLEAN-016.A3). Navigating resets Smart Clean
 * from done to idle (A4).
 *
 * @implements REQ-DCLEAN-011
 */
export function NavItem(props: NavItemProps) {
  const location = useLocation();
  const navigate = useNavigate();
  const active = () => location.pathname === props.item.route;

  function go() {
    if (isNavBlocked()) return;
    setSmartCleanState((s) => leaveViaNav(s, props.item.route === SMART_CLEAN_ROUTE));
    navigate(props.item.route);
  }

  return (
    <button
      type="button"
      class="nav-item"
      aria-current={active() ? "page" : undefined}
      aria-disabled={isNavBlocked() ? "true" : undefined}
      onClick={go}
    >
      <Dynamic component={props.item.icon} class="nav-icon" strokeWidth={2.75} aria-hidden="true" />
      <span class="nav-label">{props.item.label}</span>
      <Show when={props.item.badge && props.badge}>
        <span class="nav-badge">{props.badge}</span>
      </Show>
    </button>
  );
}
