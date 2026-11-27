import { For, Show, onMount, type Component } from "solid-js";
import { Dynamic } from "solid-js/web";
import { useNavigate } from "@solidjs/router";
import type { LucideProps } from "lucide-solid";
import Bot from "lucide-solid/icons/bot";
import File from "lucide-solid/icons/file";
import { IDLE_INTERACTIVE_MARK } from "../../lib/budgets";
import type { CleanupView } from "../../lib/contract";
import type { DiskState } from "../../lib/diskState";
import { GoalTag } from "../../shell/GoalTag";
import { UsageRing } from "../../shell/UsageRing";
import { SpaceMapIcon } from "../../shell/icons";
import { actionableCount } from "../assistant/store";
import { GoalNotification } from "./GoalNotification";
import {
  IDLE_HEADING,
  SPACE_MAP_SHORTCUT,
  assistantShortcut,
  idleDescription,
  largeFilesShortcut,
  lastScanKicker,
} from "./idleCopy";

export const SPACE_MAP_ROUTE = "/space-map";

export interface IdleProps {
  view: CleanupView;
  /** Shared disk state; the ring and tag read it so a clean updates them (REQ-DCLEAN-002.A4). */
  disk: DiskState;
  onStartScan: () => void;
  /** The below-goal notification, when it should show (REQ-DCLEAN-010). */
  notification?: { text: string; agentName: string | null };
  onLater?: () => void;
  onReview?: () => void;
}

interface Shortcut {
  id: string;
  label: string;
  desc: string;
  icon: Component<LucideProps>;
  route: string;
}

/**
 * Idle phase (REQ-DCLEAN-003): kicker, heading, a mode-truthful description,
 * "Start scan" and "Explore disk", three shortcut cards, and the usage ring
 * with its goal tag. Layout follows snapshot.html "Smart clean · home".
 * The below-goal notification (REQ-DCLEAN-010) sits over the screen area's
 * top-right corner when the parent passes one; it comes last in the markup,
 * as in the snapshot, so it paints above the ring.
 *
 * The first idle render sets the {@link IDLE_INTERACTIVE_MARK} performance
 * mark that the latency budget is measured against (REQ-DCLEAN-030.A1).
 *
 * @implements REQ-DCLEAN-003
 * @implements REQ-DCLEAN-010
 * @implements REQ-DCLEAN-030
 * @implements REQ-DSCR-009
 */
export function Idle(props: IdleProps) {
  const navigate = useNavigate();
  onMount(() => {
    if (performance.getEntriesByName(IDLE_INTERACTIVE_MARK).length === 0) performance.mark(IDLE_INTERACTIVE_MARK);
  });
  const shortcuts = (): Shortcut[] => [
    // REQ-DSCR-009.A5: the feed's actionable cards, as the sidebar badge counts them.
    { id: "assistant", label: "Assistant", desc: assistantShortcut(actionableCount() ?? 0), icon: Bot, route: "/assistant" },
    { id: "map", label: "Space Map", desc: SPACE_MAP_SHORTCUT, icon: SpaceMapIcon, route: SPACE_MAP_ROUTE },
    {
      id: "large",
      label: "Large files",
      desc: largeFilesShortcut(props.view.large_files_count),
      icon: File,
      route: "/large-files",
    },
  ];

  return (
    <div class="idle" data-screen-label="Smart clean · home" data-testid="idle">
      <div class="idle-main">
        <div class="idle-kicker" data-testid="last-scan">
          {lastScanKicker(props.view.last_scan_at)}
        </div>
        <h1 class="idle-title" tabIndex={-1} data-phase-heading>{IDLE_HEADING}</h1>
        <p class="idle-desc" data-testid="idle-description">
          {idleDescription(props.view.preferences.trash_by_default)}
        </p>
        <div class="idle-actions">
          <button type="button" class="btn btn-primary idle-start" onClick={() => props.onStartScan()}>
            Start scan
          </button>
          <button type="button" class="btn btn-secondary idle-explore" onClick={() => navigate(SPACE_MAP_ROUTE)}>
            Explore disk
          </button>
        </div>
        <div class="idle-shortcuts">
          <For each={shortcuts()}>
            {(s) => (
              <button
                type="button"
                class="card shortcut"
                data-testid={`shortcut-${s.id}`}
                onClick={() => navigate(s.route)}
              >
                <span class="shortcut-icon">
                  <Dynamic component={s.icon} class="shortcut-glyph" strokeWidth={2.75} aria-hidden="true" />
                </span>
                <span class="card-title shortcut-title">{s.label}</span>
                <span class="card-body" data-testid={`shortcut-${s.id}-desc`}>
                  {s.desc}
                </span>
              </button>
            )}
          </For>
        </div>
      </div>
      <div class="idle-rail">
        <UsageRing disk={props.disk.disk} phase="idle" />
        <GoalTag freeBytes={props.disk.disk.free_bytes} goalBytes={props.disk.goalBytes} />
      </div>
      {/* Last, as in the snapshot's main: paints over the ring rail. */}
      <Show when={props.notification}>
        {(n) => (
          <GoalNotification
            text={n().text}
            agentName={n().agentName}
            onLater={() => props.onLater?.()}
            onReview={() => props.onReview?.()}
          />
        )}
      </Show>
    </div>
  );
}
