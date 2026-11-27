/**
 * Shared Assistant state (REQ-DSCR-009): the engine's last Assistant view
 * and the cards dismissed this session. Owned by its own root, so the
 * sidebar badge, the Smart Clean shortcut and the Assistant screen read the
 * same feed, and a dismissal survives a trip to another screen. The
 * dismissals live in memory only: a restart brings every card back (A4).
 *
 * The view is the engine's (`load_assistant`, or one the proactive checks
 * send, REQ-DSCR-011.A1); this module only filters out what was dismissed.
 */
import { createRoot, createSignal } from "solid-js";
import { loadAssistant } from "../../lib/engine";
import { setFromView } from "../../lib/diskState";
import { parseEngineError, type AssistantView, type EngineError, type Suggestion, type SuggestionKey } from "../../lib/contract";

export interface AssistantState {
  /** The engine's last view; null until one loads. */
  view: AssistantView | null;
  /** Cards hidden with "Not now" / "Dismiss" this session. */
  dismissed: ReadonlySet<SuggestionKey>;
  /** Why the last load failed, if it did. */
  error: EngineError | null;
}

const INITIAL: AssistantState = { view: null, dismissed: new Set(), error: null };

const [state, setState] = createRoot(() => createSignal<AssistantState>(INITIAL));

/** The current Assistant state. */
export const assistantState = state;

/**
 * Take a view from the engine. Its disk reading and goal feed the shared
 * disk state too, so the meter and goal tags follow (REQ-DSCR-010.A2).
 */
export function setAssistantView(view: AssistantView): void {
  setState((s) => ({ ...s, view, error: null }));
  setFromView(view);
}

/** Load the view from the engine. A failure keeps the last view and is reported. */
export async function reloadAssistant(): Promise<void> {
  try {
    setAssistantView(await loadAssistant());
  } catch (e) {
    setState((s) => ({ ...s, error: parseEngineError(e) }));
  }
}

/**
 * The feed as shown: the engine's suggestions minus the dismissed ones
 * (REQ-DSCR-009.A4).
 *
 * @implements REQ-DSCR-009
 */
export function visibleSuggestions(): Suggestion[] {
  const s = state();
  return (s.view?.suggestions ?? []).filter((x) => !s.dismissed.has(x.key));
}

/**
 * Cards with a review action — the sidebar badge and the Smart Clean
 * shortcut's count (REQ-DSCR-009.A5). Null until the engine's view loads.
 *
 * @implements REQ-DSCR-009
 */
export function actionableCount(): number | null {
  if (!state().view) return null;
  return visibleSuggestions().filter((x) => x.actionable).length;
}

/** The selected agent's display name, once the engine has named it. */
export function agentName(): string | null {
  return state().view?.agent_name ?? null;
}

/** "Not now" / "Dismiss": hide that card for the rest of the session (REQ-DSCR-009.A4). */
export function dismiss(key: SuggestionKey): void {
  setState((s) => ({ ...s, dismissed: new Set([...s.dismissed, key]) }));
}

/** A new space goal brings a dismissed goal card back (REQ-DSCR-010.A2). */
export function restoreGoalCard(): void {
  setState((s) => {
    const dismissed = new Set(s.dismissed);
    dismissed.delete("goal");
    return { ...s, dismissed };
  });
}

/** Back to the initial state (tests). */
export function resetAssistantStore(): void {
  setState(INITIAL);
}
