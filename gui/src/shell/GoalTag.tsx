import { goalStatus } from "../lib/meter";

export interface GoalTagProps {
  freeBytes: number;
  goalBytes: number;
}

/**
 * Goal tag (REQ-DCLEAN-002.A2): "Below your goal" (`tag-accent`) when free
 * space is strictly below the goal, else "Goal met" (`tag-accent-2`).
 */
export function GoalTag(props: GoalTagProps) {
  const below = () => goalStatus(props.freeBytes, props.goalBytes) === "below";
  return (
    <span class={below() ? "tag tag-accent" : "tag tag-accent-2"} data-testid="goal-tag">
      {below() ? "Below your goal" : "Goal met"}
    </span>
  );
}
