import Bot from "lucide-solid/icons/bot";
import {
  NOTIFICATION_LATER,
  NOTIFICATION_REVIEW,
  NOTIFICATION_WHEN,
  notificationSender,
} from "./idleCopy";

export interface GoalNotificationProps {
  text: string;
  /** The selected agent's display name, from the engine (REQ-DSCR-009.A5); null until it loads. */
  agentName: string | null;
  onLater: () => void;
  onReview: () => void;
}

/**
 * The below-goal notification (REQ-DCLEAN-010), laid out as the snapshot's
 * toast: top-right of the screen area, sender line, text, then "Later" and
 * "Review cleanup". A polite status region; it never takes focus (A5), so
 * it carries no autofocus, focus call or tabindex.
 *
 * @implements REQ-DCLEAN-010
 */
export function GoalNotification(props: GoalNotificationProps) {
  return (
    <div class="notify" role="status" aria-live="polite" data-testid="goal-notification">
      <div class="notify-head">
        <span class="notify-icon">
          <Bot class="notify-glyph" strokeWidth={2.75} aria-hidden="true" />
        </span>
        <span class="notify-who">{notificationSender(props.agentName)}</span>
        <span class="notify-when">{NOTIFICATION_WHEN}</span>
      </div>
      <div class="notify-text" data-testid="goal-notification-text">
        {props.text}
      </div>
      <div class="notify-actions">
        <button type="button" class="btn btn-secondary" onClick={() => props.onLater()}>
          {NOTIFICATION_LATER}
        </button>
        <button type="button" class="btn btn-primary" onClick={() => props.onReview()}>
          {NOTIFICATION_REVIEW}
        </button>
      </div>
    </div>
  );
}
