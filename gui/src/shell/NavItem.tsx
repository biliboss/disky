import { Show } from "solid-js";
import { Dynamic } from "solid-js/web";
import { useLocation, useNavigate } from "@solidjs/router";
import type { NavItemDef } from "./nav";

export interface NavItemProps {
  item: NavItemDef;
  /** Badge text; rendered only when the item carries a badge and this is non-empty. */
  badge?: string;
}

/**
 * One sidebar entry. Active = the current route; styled by `[aria-current=page]`
 * (accent pill, bg-coloured text, weight 600 — snapshot `n.style`).
 */
export function NavItem(props: NavItemProps) {
  const location = useLocation();
  const navigate = useNavigate();
  const active = () => location.pathname === props.item.route;

  return (
    <button
      type="button"
      class="nav-item"
      aria-current={active() ? "page" : undefined}
      onClick={() => navigate(props.item.route)}
    >
      <Dynamic component={props.item.icon} class="nav-icon" strokeWidth={2.75} aria-hidden="true" />
      <span class="nav-label">{props.item.label}</span>
      <Show when={props.item.badge && props.badge}>
        <span class="nav-badge">{props.badge}</span>
      </Show>
    </button>
  );
}
