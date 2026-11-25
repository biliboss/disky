import type { Disk } from "../lib/contract";
import { formatBytes } from "../lib/format";
import { RING_CIRCUMFERENCE, ringDash } from "../lib/meter";

export interface UsageRingProps {
  disk: Disk;
  /** Fill colour: `--color-accent` on idle, `--color-accent-2` on done (REQ-DCLEAN-002.A5). */
  phase: "idle" | "done";
}

/**
 * Used-share ring of the idle and done phases (REQ-DCLEAN-002), labelled with
 * the free amount and the capacity. Geometry is the snapshot's: a 120-unit
 * viewBox, r = 50, drawn from 12 o'clock; its size and stroke are tokens.
 */
export function UsageRing(props: UsageRingProps) {
  return (
    <div
      class="ring"
      data-testid="usage-ring"
      data-phase={props.phase}
      role="img"
      aria-label={`${formatBytes(props.disk.free_bytes)} free of ${formatBytes(props.disk.capacity_bytes)}`}
    >
      <svg class="ring-svg" viewBox="0 0 120 120" aria-hidden="true">
        <circle class="ring-track" cx="60" cy="60" r="50" fill="none" />
        <circle
          class={`ring-fill ring-fill--${props.phase}`}
          data-testid="ring-fill"
          cx="60"
          cy="60"
          r="50"
          fill="none"
          stroke-dasharray={ringDash(props.disk.capacity_bytes, props.disk.free_bytes, RING_CIRCUMFERENCE)}
        />
      </svg>
      <div class="ring-label">
        <span class="ring-free num" data-testid="ring-free">
          {formatBytes(props.disk.free_bytes)}
        </span>
        {/* Idle: "free of <capacity>"; done: "now free", as the snapshot (REQ-DCLEAN-009). */}
        <span class="ring-total" data-testid="ring-total">
          {props.phase === "done" ? "now free" : `free of ${formatBytes(props.disk.capacity_bytes)}`}
        </span>
      </div>
    </div>
  );
}
