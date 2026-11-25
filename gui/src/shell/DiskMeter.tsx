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
 * bar, goal marker and the goal line. Fed fixture props by the shell until
 * REQ-DCLEAN-002 wires live physical free space from the engine.
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
      <div class="meter-bar">
        <div class="meter-fill" style={{ width: usedPct() }} />
        <div class="meter-marker" data-testid="goal-marker" style={{ left: markerLeft() }} />
      </div>
      <div class="meter-goal">Goal: keep {formatBytes(props.goalBytes)} free</div>
    </div>
  );
}
