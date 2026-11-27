import { formatBytes } from "../lib/format";
import { goalPos, usedFraction } from "../lib/meter";

export interface DiskMeterProps {
  volume: string;
  freeBytes: number;
  capacityBytes: number;
  goalBytes: number;
}

/**
 * Sidebar disk-usage meter (REQ-DCLEAN-002): volume, free space, used-share
 * bar, goal marker and the goal line. The shell feeds it the shared disk
 * state (`lib/diskState.ts`): physical bytes from the engine. The bar is an
 * image whose text equivalent is "<free> free of <capacity>" (REQ-DCLEAN-021.A4).
 *
 * @implements REQ-DCLEAN-002
 * @implements REQ-DCLEAN-021
 */
export function DiskMeter(props: DiskMeterProps) {
  const usedPct = () => `${usedFraction(props.capacityBytes, props.freeBytes) * 100}%`;
  const markerLeft = () =>
    `calc(${goalPos(props.capacityBytes, props.goalBytes) * 100}% - var(--meter-marker-nudge))`;

  return (
    <div class="meter" data-testid="disk-meter">
      <div class="meter-row">
        <span class="meter-volume">{props.volume}</span>
        <span class="meter-free">{formatBytes(props.freeBytes)} free</span>
      </div>
      <div
        class="meter-bar"
        role="img"
        aria-label={`${formatBytes(props.freeBytes)} free of ${formatBytes(props.capacityBytes)}`}
        data-testid="meter-bar"
      >
        <div class="meter-fill" style={{ width: usedPct() }} />
        <div class="meter-marker" data-testid="goal-marker" style={{ left: markerLeft() }} />
      </div>
      <div class="meter-goal">Goal: keep {formatBytes(props.goalBytes)} free</div>
    </div>
  );
}
